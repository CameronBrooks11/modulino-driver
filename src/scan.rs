//! Discovery: what answers on a bus, identified by evidence.
//!
//! Every address in `0x01..=0x77` is probed with a 1-byte read rather than a
//! zero-length write, because some HALs refuse zero-length writes and the
//! latch relay sits at 0x02 (design D12). An address that answers is
//! classified in this order, and never by its address alone:
//!
//! 1. At 0x70-0x77, a hub check: write an odd port mask, read it back, then
//!    restore the byte the probe read. A node drops the 1-byte write (it is
//!    shorter than any node frame) and reads back its strap byte, which is
//!    always even, so it cannot pass.
//! 2. At a chip's datasheet address, that chip's identity check
//!    ([`ChipId`]).
//! 3. The probe byte as a node strap byte.
//! 4. Otherwise [`Found::Unknown`].
//!
//! The identity checks write at most two bytes, shorter than any node frame,
//! so a node never acts on them. Another chip at one of these addresses may:
//! a register-index write sets its register pointer, and on a part with
//! 8-bit indexes the VL53L4's 2-byte index (0x01, 0x0F) writes 0x0F to
//! register 0x01. Scan a bus whose devices you know.
//!
//! An SHT3x NACKs a read while it has no data, so at its address a NACKed
//! probe still runs the status check, whose read is retried a bounded
//! number of times while the sensor prepares the reply.
//!
//! Known limits:
//!
//! - A Pixels node moved to 0x6A or 0x6B reads 0x6C for any register,
//!   which is also the LSM6DSOX `WHO_AM_I` value, so it is reported as
//!   [`ChipKind::Lsm6dsox`].
//! - The HS3003 has no readable identity, and its first data byte can
//!   equal a node strap byte, so anything at 0x44 is [`Found::Unknown`]
//!   (including a node moved there).
//! - Any other device whose first byte equals a strap byte reads as that
//!   node.
//! - Behind a hub port, the hub itself and any hub above it stay
//!   addressable. Through port `n` a hub reads back `1 << n`, which can be
//!   mistaken for a node (port 2 reads 0x04, the latch relay strap), so pass
//!   every hub above the port to [`scan_except`].
//! - A HAL that reports a NACK as something other than
//!   `ErrorKind::NoAcknowledge` makes the scan stop at the first empty
//!   address.

use embedded_hal::i2c::{ErrorKind, I2c};

use crate::addr::{Address, Strap};
use crate::hub::{HUB_MAX_ADDRESS, HUB_MIN_ADDRESS};
use crate::spec::{sensirion_crc8, ChipId, ChipKind, Kind, CHIPS};
use crate::Error;

/// One device found by [`scan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Found {
    /// A node board, identified by its strap byte.
    Node(Address, Kind),
    /// A chip module, identified by its ID check.
    Chip(Address, ChipKind),
    /// A Modulino Hub (TCA9548A), identified by its control register.
    Hub(Address),
    /// Something answered but proved no identity.
    Unknown(Address),
}

impl Found {
    /// The address it was found at.
    pub fn address(&self) -> Address {
        match *self {
            Found::Node(a, _) | Found::Chip(a, _) | Found::Hub(a) | Found::Unknown(a) => a,
        }
    }
}

/// Scan `0x01..=0x77` and write what answers into `out`, in address order.
///
/// Returns how many devices answered. If that is more than `out.len()`,
/// only the first `out.len()` were stored. A NACK means nothing is there;
/// any other bus error stops the scan and is returned.
pub fn scan<I: I2c>(bus: &mut I, out: &mut [Found]) -> Result<usize, Error<I::Error>> {
    scan_except(bus, &[], out)
}

/// [`scan`], skipping the addresses in `skip`: on a hub port, the hub and
/// any hubs above it.
pub fn scan_except<I: I2c>(
    bus: &mut I,
    skip: &[Address],
    out: &mut [Found],
) -> Result<usize, Error<I::Error>> {
    let mut count = 0;
    for raw in 0x01..=0x77u8 {
        let addr = Address::new(raw).expect("0x01..=0x77 are addresses");
        if skip.contains(&addr) {
            continue;
        }
        let mut probe = [0u8; 1];
        let found = if acked(bus.read(raw, &mut probe))? {
            classify(bus, addr, probe[0])?
        } else {
            match silent_chip(bus, raw)? {
                Some(kind) => Found::Chip(addr, kind),
                None => continue,
            }
        };
        if let Some(slot) = out.get_mut(count) {
            *slot = found;
        }
        count += 1;
    }
    Ok(count)
}

/// `Ok(true)` on success, `Ok(false)` on a NACK, `Err` for anything else.
fn acked<E: embedded_hal::i2c::Error>(result: Result<(), E>) -> Result<bool, Error<E>> {
    match result {
        Ok(()) => Ok(true),
        Err(e) if matches!(e.kind(), ErrorKind::NoAcknowledge(_)) => Ok(false),
        Err(e) => Err(Error::I2c(e)),
    }
}

/// A chip that NACKs reads until commanded: the SHT3x.
fn silent_chip<I: I2c>(bus: &mut I, raw: u8) -> Result<Option<ChipKind>, Error<I::Error>> {
    for chip in CHIPS.iter().filter(|c| c.addresses.contains(&raw)) {
        if matches!(chip.id, ChipId::Sht3xStatus { .. }) && is_chip(bus, raw, chip.id)? {
            return Ok(Some(chip.kind));
        }
    }
    Ok(None)
}

fn classify<I: I2c>(bus: &mut I, addr: Address, probe: u8) -> Result<Found, Error<I::Error>> {
    let raw = addr.get();
    if (HUB_MIN_ADDRESS..=HUB_MAX_ADDRESS).contains(&raw) && is_hub(bus, raw, probe)? {
        return Ok(Found::Hub(addr));
    }
    let mut chips = CHIPS.iter().filter(|c| c.addresses.contains(&raw));
    for chip in chips.clone() {
        if is_chip(bus, raw, chip.id)? {
            return Ok(Found::Chip(addr, chip.kind));
        }
    }
    // A chip with no identity sends data, not a strap byte: do not guess.
    if chips.any(|c| c.id == ChipId::None) {
        return Ok(Found::Unknown(addr));
    }
    Ok(match Strap::from_read(probe).kind() {
        Some(kind) => Found::Node(addr, kind),
        None => Found::Unknown(addr),
    })
}

/// Write an odd mask, read it back, and restore the probed byte whatever
/// the read-back shows, so a failed check never leaves port 0 connected.
fn is_hub<I: I2c>(bus: &mut I, raw: u8, probe: u8) -> Result<bool, Error<I::Error>> {
    const MASK: u8 = 0x01;
    if !acked(bus.write(raw, &[MASK]))? {
        return Ok(false);
    }
    let mut back = [0u8; 1];
    let read = acked(bus.read(raw, &mut back));
    let restore = acked(bus.write(raw, &[probe]));
    let hub = read? && back[0] == MASK;
    restore?;
    Ok(hub)
}

/// How many times the SHT3x status read is retried on a NACK: each try is a
/// few bit times, so this covers the sensor's sub-millisecond preparation
/// at 100-400 kHz.
const SHT3X_READ_TRIES: usize = 40;

fn is_chip<I: I2c>(bus: &mut I, raw: u8, id: ChipId) -> Result<bool, Error<I::Error>> {
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
