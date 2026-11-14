//! Drivers for the chip-based modules, and what they share.
//!
//! Each driver takes an [`I2c`] and only ever addresses its own chip: none
//! reconfigures, re-clocks or releases the bus. Every wait is a bounded
//! poll that returns [`Error::Timeout`]. Constructors do no I/O;
//! `verify()` checks the chip's identity from [`CHIPS`]
//! and fails with [`Error::WrongChip`] if something else answers.

pub mod hs3003;
pub mod lps22hb;
pub mod lsm6dsox;
pub mod ltr381rgb;
pub mod sht31;
pub mod vl53l4;

pub use lps22hb::Lps22hb;
pub use lsm6dsox::{Axes, Lsm6dsox};
pub use ltr381rgb::Ltr381rgb;
pub use vl53l4::Vl53l4;

use embedded_hal::i2c::{ErrorKind, I2c};

use crate::addr::Address;
use crate::spec::{sensirion_crc8, ChipId, ChipKind, CHIPS, NODES};
use crate::Error;

/// `Ok(true)` on success, `Ok(false)` on a NACK, `Err` for anything else.
pub(crate) fn acked<E: embedded_hal::i2c::Error>(result: Result<(), E>) -> Result<bool, Error<E>> {
    match result {
        Ok(()) => Ok(true),
        Err(e) if matches!(e.kind(), ErrorKind::NoAcknowledge(_)) => Ok(false),
        Err(e) => Err(Error::I2c(e)),
    }
}

/// Whether `e` is a bus NACK: nothing answered.
pub(crate) fn is_nack<E: embedded_hal::i2c::Error>(e: &Error<E>) -> bool {
    matches!(e, Error::I2c(b) if matches!(b.kind(), ErrorKind::NoAcknowledge(_)))
}

/// How many times the SHT3x status read is retried on a NACK. A NACKed read
/// is about 11 clock periods, so 200 tries span roughly 2 ms at 1 MHz and
/// more at slower clocks (an estimate, not a measurement), against the
/// 1 ms Sensirion's own driver waits after a command. The tries cost
/// nothing unless a device at the SHT31 address accepted the command.
const SHT3X_READ_TRIES: usize = 200;

/// Whether `byte` is a strap byte of any node kind.
fn is_node_strap(byte: u8) -> bool {
    NODES.iter().any(|n| n.straps.contains(&byte))
}

/// What an identity check found, when the bus itself did not fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdCheck {
    /// The identity matched.
    Match,
    /// Something answered with another identity: the byte read, or `None`
    /// for an SHT3x status whose CRC did not match. A node that answered
    /// the SHT3x check gives its strap byte.
    Mismatch(Option<u8>),
    /// An SHT3x accepted the command but never released its status.
    NoReply,
}

/// Run identity check `id` against the chip at `raw`. Every bus error, a
/// NACK included, is returned; callers decide what a NACK means.
/// `ChipId::None` is a 1-byte read that matches whatever answers.
pub(crate) fn check_id<I: I2c>(
    bus: &mut I,
    raw: u8,
    id: ChipId,
) -> Result<IdCheck, Error<I::Error>> {
    let compare = |byte: u8, value: u8, mask: u8| {
        if byte & mask == value {
            IdCheck::Match
        } else {
            IdCheck::Mismatch(Some(byte))
        }
    };
    let mut byte = [0u8; 1];
    match id {
        ChipId::Register8 {
            register,
            value,
            mask,
        } => {
            bus.write_read(raw, &[register], &mut byte)
                .map_err(Error::I2c)?;
            Ok(compare(byte[0], value, mask))
        }
        ChipId::Register16 {
            register,
            value,
            mask,
        } => {
            bus.write_read(raw, &register.to_be_bytes(), &mut byte)
                .map_err(Error::I2c)?;
            Ok(compare(byte[0], value, mask))
        }
        ChipId::Sht3xStatus { command } => {
            bus.write(raw, &command.to_be_bytes()).map_err(Error::I2c)?;
            let mut status = [0u8; 3];
            for _ in 0..SHT3X_READ_TRIES {
                if acked(bus.read(raw, &mut status))? {
                    if sensirion_crc8(&status[..2]) != status[2] {
                        return Ok(IdCheck::Mismatch(None));
                    }
                    // A node drops the 2-byte command, shorter than any node
                    // frame (main.c:531-561), and starts every read with its
                    // strap byte (main.c:476), so its reply can pass the CRC
                    // by chance: a Knob strapped 0x74 at 1 in 256 counts. An
                    // SHT3x status can start with a strap value too (0x04 or
                    // 0x28 fit its defined bits, SHT3x-DIS datasheet v7
                    // Table 18), so read once more: a node repeats its strap
                    // byte. An SHT3x NACKs a read with no data present after
                    // a single-shot measurement command (datasheet p. 10);
                    // that it does the same once its status has been read is
                    // assumed, unconfirmed on hardware.
                    if is_node_strap(status[0])
                        && acked(bus.read(raw, &mut byte))?
                        && byte[0] == status[0]
                    {
                        return Ok(IdCheck::Mismatch(Some(status[0])));
                    }
                    return Ok(IdCheck::Match);
                }
            }
            Ok(IdCheck::NoReply)
        }
        ChipId::None => {
            bus.read(raw, &mut byte).map_err(Error::I2c)?;
            Ok(IdCheck::Match)
        }
    }
}

/// Scan's view of [`check_id`]: whether the chip at `raw` proves identity
/// `id`. A NACK is `false`, and a chip with no identity never proves one;
/// any other bus error is returned.
pub(crate) fn has_id<I: I2c>(bus: &mut I, raw: u8, id: ChipId) -> Result<bool, Error<I::Error>> {
    if id == ChipId::None {
        return Ok(false);
    }
    match check_id(bus, raw, id) {
        Ok(found) => Ok(found == IdCheck::Match),
        Err(Error::I2c(e)) => acked(Err(e)),
        Err(e) => Err(e),
    }
}

/// A driver's view of [`check_id`]: check that `kind` answers at `addr`.
///
/// - Every bus error, a NACK included, is returned as it is: nothing (or
///   something broken) answered.
/// - Something that answers with another identity is
///   [`Error::WrongChip`], with the byte it sent where there is one.
/// - An SHT3x that never releases its status is [`Error::Timeout`].
/// - A chip with no readable identity passes if it acknowledges a read.
pub(crate) fn verify<I: I2c>(
    bus: &mut I,
    addr: Address,
    kind: ChipKind,
) -> Result<(), Error<I::Error>> {
    let spec = CHIPS
        .iter()
        .find(|c| c.kind == kind)
        .expect("every ChipKind has a spec row");
    match check_id(bus, addr.get(), spec.id)? {
        IdCheck::Match => Ok(()),
        IdCheck::Mismatch(got) => Err(Error::WrongChip {
            expected: kind,
            got,
        }),
        IdCheck::NoReply => Err(Error::Timeout),
    }
}

/// How many register reads a reset poll makes before giving up. Each read
/// is a full register transaction of about 38 clocks (roughly 0.4 ms at
/// 100 kHz, 0.1 ms at 400 kHz; an estimate), so this allows about 4-38 ms,
/// well past the chips' reset times.
pub(crate) const RESET_POLLS: usize = 100;

/// An 8-bit-register chip at one address.
#[derive(Debug)]
pub(crate) struct Regs<I2C> {
    pub(crate) i2c: I2C,
    pub(crate) addr: Address,
}

impl<I2C: I2c> Regs<I2C> {
    pub(crate) fn read(&mut self, reg: u8) -> Result<u8, Error<I2C::Error>> {
        let mut b = [0u8; 1];
        self.read_into(reg, &mut b)?;
        Ok(b[0])
    }

    /// Read consecutive registers; the chip must auto-increment.
    pub(crate) fn read_into(&mut self, reg: u8, buf: &mut [u8]) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write_read(self.addr.get(), &[reg], buf)
            .map_err(Error::I2c)
    }

    pub(crate) fn write(&mut self, reg: u8, value: u8) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write(self.addr.get(), &[reg, value])
            .map_err(Error::I2c)
    }

    /// Read `reg` until the bits in `mask` are clear, at most
    /// [`RESET_POLLS`] times, then [`Error::Timeout`].
    pub(crate) fn wait_clear(&mut self, reg: u8, mask: u8) -> Result<(), Error<I2C::Error>> {
        for _ in 0..RESET_POLLS {
            if self.read(reg)? & mask == 0 {
                return Ok(());
            }
        }
        Err(Error::Timeout)
    }
}
