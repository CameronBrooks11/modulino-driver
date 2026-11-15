//! Modulino Thermo: temperature and humidity, from an HS3003 at 0x44 or,
//! on some boards, an SHT31 at 0x45 (Arduino_Modulino `Modulino.h:614-627`).

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::chips::hs3003::{self, ThermoReading};
use crate::chips::{is_nack, sht31, verify};
use crate::spec::ChipKind;
use crate::Error;

/// The two sensors a Thermo board carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sensor {
    Hs3003,
    Sht31,
}

impl Sensor {
    fn kind(self) -> ChipKind {
        match self {
            Sensor::Hs3003 => ChipKind::Hs3003,
            Sensor::Sht31 => ChipKind::Sht31,
        }
    }

    fn address(self) -> Address {
        match self {
            Sensor::Hs3003 => hs3003::ADDRESS,
            Sensor::Sht31 => sht31::ADDRESS,
        }
    }
}

/// Driver for the Modulino Thermo module.
///
/// The HS3003 has no readable identity, so, as in Arduino's library, any
/// device that acknowledges at 0x44 is taken for one. [`scan`](fn@crate::scan)
/// is stricter and reports 0x44 as unknown.
///
/// ```no_run
/// # use embedded_hal_mock::eh1::{delay::NoopDelay, i2c::Mock};
/// # fn main() -> Result<(), modulino_driver::Error<embedded_hal::i2c::ErrorKind>> {
/// # let i2c = Mock::new(&[]);
/// # let mut delay = NoopDelay::new();
/// use modulino_driver::Thermo;
///
/// let mut thermo = Thermo::new(i2c);
/// let chip = thermo.verify()?; // ChipKind::Hs3003 or ChipKind::Sht31
/// let reading = thermo.read(&mut delay)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Thermo<I2C> {
    i2c: I2C,
    sensor: Option<Sensor>,
}

impl<I2C: I2c> Thermo<I2C> {
    /// A Thermo board of either variant. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self { i2c, sensor: None }
    }

    /// The sensor the last successful [`Thermo::verify`] found. A failed
    /// verify leaves it unchanged; call verify again after swapping boards.
    pub fn chip(&self) -> Option<ChipKind> {
        self.sensor.map(Sensor::kind)
    }

    /// The sensor's address, once found.
    pub fn address(&self) -> Option<Address> {
        self.sensor.map(Sensor::address)
    }

    /// Find the sensor: an HS3003 that acknowledges at 0x44, or else an
    /// SHT31 that proves its identity at 0x45. If neither does, the error is
    /// the SHT31's, unless the HS3003 gave a more telling one than a NACK.
    pub fn verify(&mut self) -> Result<ChipKind, Error<I2C::Error>> {
        let hs = Sensor::Hs3003.address();
        let first = match verify(&mut self.i2c, hs, ChipKind::Hs3003) {
            Ok(()) => return Ok(self.found(Sensor::Hs3003)),
            Err(e) => e,
        };
        let sht = Sensor::Sht31.address();
        match verify(&mut self.i2c, sht, ChipKind::Sht31) {
            Ok(()) => Ok(self.found(Sensor::Sht31)),
            Err(e) if is_nack(&first) => Err(e),
            Err(_) => Err(first),
        }
    }

    /// Measure temperature and humidity, finding the sensor first if
    /// [`Thermo::verify`] has not run. Waits through `delay`: 40 ms on the
    /// HS3003, 15 ms on the SHT31, with bounded retries after that ending in
    /// [`Error::Timeout`].
    pub fn read(&mut self, delay: &mut impl DelayNs) -> Result<ThermoReading, Error<I2C::Error>> {
        let sensor = match self.sensor {
            Some(s) => s,
            None => {
                self.verify()?;
                self.sensor.expect("verify succeeded")
            }
        };
        match sensor {
            Sensor::Hs3003 => hs3003::measure(&mut self.i2c, sensor.address(), delay),
            Sensor::Sht31 => sht31::measure(&mut self.i2c, sensor.address(), delay),
        }
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    fn found(&mut self, sensor: Sensor) -> ChipKind {
        self.sensor = Some(sensor);
        sensor.kind()
    }
}
