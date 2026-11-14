//! SHT31 temperature and humidity sensor (the Modulino Thermo variant at
//! 0x45).
//!
//! The protocol, from the Sensirion SHT3x datasheet as I know it: the
//! single-shot command 0x2400 (high repeatability, no clock stretching)
//! starts a measurement of at most 15 ms, during which a read is NACKed;
//! the result is temperature MSB, LSB, CRC, then humidity MSB, LSB, CRC,
//! with Sensirion's CRC-8. Temperature is `-45 + 175 * raw / 65535` degrees
//! C, humidity `100 * raw / 65535` percent.

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

use super::acked;
use super::hs3003::ThermoReading;
use crate::addr::Address;
use crate::spec::sensirion_crc8;
use crate::Error;

/// The SHT31's address on the Modulino Thermo.
pub const ADDRESS: u8 = 0x45;
/// Single shot, high repeatability, clock stretching off.
const MEASURE: [u8; 2] = [0x24, 0x00];
/// Maximum high-repeatability measurement time, in ms.
pub const MEASURE_MS: u32 = 15;
/// Wait between reads NACKed because the measurement is not done, in ms.
pub const RETRY_MS: u32 = 1;
/// Reads tried before [`Error::Timeout`].
pub const READS: usize = 20;

/// Decode a 6-byte result: `None` if either CRC fails.
pub fn decode(b: &[u8; 6]) -> Option<ThermoReading> {
    if sensirion_crc8(&b[0..2]) != b[2] || sensirion_crc8(&b[3..5]) != b[5] {
        return None;
    }
    let t = u16::from_be_bytes([b[0], b[1]]) as f32;
    let rh = u16::from_be_bytes([b[3], b[4]]) as f32;
    Some(ThermoReading {
        temperature: -45.0 + 175.0 * t / 65535.0,
        humidity: 100.0 * rh / 65535.0,
    })
}

/// Start a single-shot measurement at `addr` and read it, retrying while
/// the sensor NACKs. A result whose CRC fails is [`Error::DataError`].
pub(crate) fn measure<I: I2c>(
    bus: &mut I,
    addr: Address,
    delay: &mut impl DelayNs,
) -> Result<ThermoReading, Error<I::Error>> {
    bus.write(addr.get(), &MEASURE).map_err(Error::I2c)?;
    delay.delay_ms(MEASURE_MS);
    let mut b = [0u8; 6];
    for read in 0..READS {
        if read > 0 {
            delay.delay_ms(RETRY_MS);
        }
        if acked(bus.read(addr.get(), &mut b))? {
            return decode(&b).ok_or(Error::DataError);
        }
    }
    Err(Error::Timeout)
}
