//! Modulino Thermo: temperature and humidity, from an HS3003 at 0x44 or,
//! on some boards, an SHT31 at 0x45 (Arduino_Modulino `Modulino.h:614-627`).

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::chips::hs3003::{self, ThermoReading};
use crate::chips::{is_nack, sht31, verify};
use crate::spec::ChipKind;
use crate::Error;

/// Driver for the Modulino Thermo module.
///
/// ```rust,ignore
/// use modulino_driver::Thermo;
///
/// let mut thermo = Thermo::new(i2c);
/// let chip = thermo.verify()?; // ChipKind::Hs3003 or ChipKind::Sht31
/// let reading = thermo.read(&mut delay)?;
/// ```
#[derive(Debug)]
pub struct Thermo<I2C> {
    i2c: I2C,
    chip: Option<ChipKind>,
}

impl<I2C: I2c> Thermo<I2C> {
    /// A Thermo board of either variant. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self { i2c, chip: None }
    }

    /// The sensor found by [`Thermo::verify`], if it has run.
    pub fn chip(&self) -> Option<ChipKind> {
        self.chip
    }

    /// The sensor's address, once found.
    pub fn address(&self) -> Option<Address> {
        self.chip.map(|c| {
            let raw = match c {
                ChipKind::Sht31 => sht31::ADDRESS,
                _ => hs3003::ADDRESS,
            };
            Address::new(raw).expect("sensor addresses are valid")
        })
    }

    /// Find the sensor: an HS3003 that acknowledges at 0x44 (it has no
    /// readable identity), or else an SHT31 that proves its identity at
    /// 0x45. If neither does, the error is the SHT31's, unless the HS3003
    /// gave a more telling one than a NACK.
    pub fn verify(&mut self) -> Result<ChipKind, Error<I2C::Error>> {
        let hs = Address::new(hs3003::ADDRESS).expect("0x44 is an address");
        let first = match verify(&mut self.i2c, hs, ChipKind::Hs3003) {
            Ok(()) => return Ok(self.found(ChipKind::Hs3003)),
            Err(e) => e,
        };
        let sht = Address::new(sht31::ADDRESS).expect("0x45 is an address");
        match verify(&mut self.i2c, sht, ChipKind::Sht31) {
            Ok(()) => Ok(self.found(ChipKind::Sht31)),
            Err(e) if is_nack(&first) => Err(e),
            Err(_) => Err(first),
        }
    }

    /// Measure temperature and humidity, finding the sensor first if
    /// [`Thermo::verify`] has not run. Waits through `delay`: about 40 ms
    /// on the HS3003, 15 ms on the SHT31, with bounded retries after that
    /// ending in [`Error::Timeout`].
    pub fn read(&mut self, delay: &mut impl DelayNs) -> Result<ThermoReading, Error<I2C::Error>> {
        let chip = match self.chip {
            Some(c) => c,
            None => self.verify()?,
        };
        let addr = self.address().expect("chip found above");
        match chip {
            ChipKind::Sht31 => sht31::measure(&mut self.i2c, addr, delay),
            _ => hs3003::measure(&mut self.i2c, addr, delay),
        }
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    fn found(&mut self, chip: ChipKind) -> ChipKind {
        self.chip = Some(chip);
        chip
    }
}
