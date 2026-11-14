//! Drivers for the chip-based modules, and what they share.
//!
//! Each driver takes an [`I2c`] and only ever addresses its own chip: none
//! reconfigures, re-clocks or releases the bus. Every wait is a bounded
//! poll that returns [`Error::Timeout`]. Constructors do no I/O;
//! `verify()` checks the chip's identity from [`CHIPS`]
//! and fails with [`Error::WrongChip`] if something else answers.

pub mod lps22hb;
pub mod lsm6dsox;

use embedded_hal::i2c::{ErrorKind, I2c};

use crate::addr::Address;
use crate::spec::{sensirion_crc8, ChipId, ChipKind, CHIPS};
use crate::Error;

/// `Ok(true)` on success, `Ok(false)` on a NACK, `Err` for anything else.
pub(crate) fn acked<E: embedded_hal::i2c::Error>(result: Result<(), E>) -> Result<bool, Error<E>> {
    match result {
        Ok(()) => Ok(true),
        Err(e) if matches!(e.kind(), ErrorKind::NoAcknowledge(_)) => Ok(false),
        Err(e) => Err(Error::I2c(e)),
    }
}

/// How many times the SHT3x status read is retried on a NACK. A NACKed read
/// is about 11 clock periods, so 200 tries span roughly 2 ms at 1 MHz and
/// more at slower clocks (an estimate, not a measurement), against the
/// 1 ms Sensirion's own driver waits after a command. The tries cost
/// nothing unless a device at the SHT31 address accepted the command.
const SHT3X_READ_TRIES: usize = 200;

/// Whether the chip at `raw` passes identity check `id`. A NACK is `false`;
/// any other bus error is returned.
pub(crate) fn has_id<I: I2c>(bus: &mut I, raw: u8, id: ChipId) -> Result<bool, Error<I::Error>> {
    let mut byte = [0u8; 1];
    match id {
        ChipId::Register8 {
            register,
            value,
            mask,
        } => Ok(acked(bus.write_read(raw, &[register], &mut byte))? && byte[0] & mask == value),
        ChipId::Register16 {
            register,
            value,
            mask,
        } => Ok(
            acked(bus.write_read(raw, &register.to_be_bytes(), &mut byte))?
                && byte[0] & mask == value,
        ),
        ChipId::Sht3xStatus { command } => {
            if !acked(bus.write(raw, &command.to_be_bytes()))? {
                return Ok(false);
            }
            let mut status = [0u8; 3];
            for _ in 0..SHT3X_READ_TRIES {
                if acked(bus.read(raw, &mut status))? {
                    return Ok(sensirion_crc8(&status[..2]) == status[2]);
                }
            }
            Ok(false)
        }
        ChipId::None => Ok(false),
    }
}

/// Check that `kind` answers at `addr`. Bus errors, a NACK included, are
/// returned as they are: nothing answered. Something that answers but fails
/// the identity check is [`Error::WrongChip`]. A chip with no readable
/// identity passes if it acknowledges a 1-byte read.
pub(crate) fn verify<I: I2c>(
    bus: &mut I,
    addr: Address,
    kind: ChipKind,
) -> Result<(), Error<I::Error>> {
    let spec = CHIPS
        .iter()
        .find(|c| c.kind == kind)
        .expect("every ChipKind has a spec row");
    let raw = addr.get();
    let mut byte = [0u8; 1];
    let matches = match spec.id {
        ChipId::Register8 {
            register,
            value,
            mask,
        } => {
            bus.write_read(raw, &[register], &mut byte)
                .map_err(Error::I2c)?;
            byte[0] & mask == value
        }
        ChipId::Register16 {
            register,
            value,
            mask,
        } => {
            bus.write_read(raw, &register.to_be_bytes(), &mut byte)
                .map_err(Error::I2c)?;
            byte[0] & mask == value
        }
        ChipId::Sht3xStatus { command } => {
            bus.write(raw, &command.to_be_bytes()).map_err(Error::I2c)?;
            let mut status = [0u8; 3];
            let mut answered = false;
            for _ in 0..SHT3X_READ_TRIES {
                if acked(bus.read(raw, &mut status))? {
                    answered = true;
                    break;
                }
            }
            if !answered {
                return Err(Error::Timeout);
            }
            sensirion_crc8(&status[..2]) == status[2]
        }
        ChipId::None => {
            bus.read(raw, &mut byte).map_err(Error::I2c)?;
            true
        }
    };
    if matches {
        Ok(())
    } else {
        Err(Error::WrongChip { expected: kind })
    }
}

/// How many register reads a reset poll makes before giving up. Each read
/// is a full I2C transaction (about 0.2 ms at 100 kHz, 50 us at 400 kHz),
/// so this allows several milliseconds, well past the chips' reset times.
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
