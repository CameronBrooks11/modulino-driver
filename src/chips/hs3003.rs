//! HS3003 temperature and humidity sensor (Modulino Thermo).
//!
//! The protocol, from the Renesas HS300x datasheet as I know it and the
//! `hs3003` crate 0.3 used: a write to the sensor requests a measurement
//! (this sends one 0x00 byte, as that crate does, rather than a zero-length
//! write that some HALs refuse); after the conversion a 4-byte read returns
//! two status bits and 14-bit humidity, then 14-bit temperature, left
//! aligned. Status 00 is a new sample, 01 a sample already read.
//!
//! The sensor has no readable identity.

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::Error;

/// The HS3003's fixed address.
pub const ADDRESS: Address = match Address::new(0x44) {
    Some(a) => a,
    None => panic!("valid address"),
};
/// Wait after a request before the first fetch, in ms: the 14-bit
/// conversion takes about 34 ms (datasheet, as I know it).
pub const CONVERSION_MS: u32 = 40;
/// Wait between fetches that return an already-read sample, in ms.
pub const RETRY_MS: u32 = 5;
/// Fetches tried before [`Error::Timeout`].
pub const FETCHES: usize = 20;

/// One temperature and humidity reading.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ThermoReading {
    /// Temperature in degrees C
    pub temperature: f32,
    /// Relative humidity in percent
    pub humidity: f32,
}

/// Decode a 4-byte data fetch: `Some` for a new sample (status 00), `None`
/// for one already read (any other status).
pub fn decode(b: &[u8; 4]) -> Option<ThermoReading> {
    if b[0] >> 6 != 0 {
        return None;
    }
    // The status bits are 00 here, so the top byte is humidity alone.
    let humidity = u16::from_be_bytes([b[0], b[1]]);
    let temperature = u16::from_be_bytes([b[2], b[3]]) >> 2;
    Some(ThermoReading {
        temperature: temperature as f32 / 16383.0 * 165.0 - 40.0,
        humidity: humidity as f32 / 16383.0 * 100.0,
    })
}

/// Request a measurement at `addr`, wait for it, and fetch it, retrying
/// while the sensor still returns the previous sample.
pub(crate) fn measure<I: I2c>(
    bus: &mut I,
    addr: Address,
    delay: &mut impl DelayNs,
) -> Result<ThermoReading, Error<I::Error>> {
    bus.write(addr.get(), &[0x00]).map_err(Error::I2c)?;
    delay.delay_ms(CONVERSION_MS);
    let mut b = [0u8; 4];
    for fetch in 0..FETCHES {
        if fetch > 0 {
            delay.delay_ms(RETRY_MS);
        }
        bus.read(addr.get(), &mut b).map_err(Error::I2c)?;
        if let Some(reading) = decode(&b) {
            return Ok(reading);
        }
    }
    Err(Error::Timeout)
}
