//! LPS22HB barometric pressure sensor (Modulino Pressure).
//!
//! Registers (ST LPS22HB datasheet, as I know it): `WHO_AM_I` 0x0F = 0xB1, `CTRL_REG1`
//! 0x10 (`ODR` bits 6-4, `BDU` bit 1), `CTRL_REG2` 0x11 (`IF_ADD_INC`
//! bit 4, set at reset; `SWRESET` bit 2), `STATUS` 0x27 (`P_DA` bit 0,
//! `T_DA` bit 1), `PRESS_OUT_XL` 0x28 (24-bit two's complement, 4096 LSB
//! per hPa) and `TEMP_OUT_L` 0x2B (16-bit two's complement, 100 LSB per
//! degree C).

use embedded_hal::i2c::I2c;

use super::Regs;
use crate::addr::Address;
use crate::spec::ChipKind;
use crate::Error;

const CTRL_REG1: u8 = 0x10;
const CTRL_REG2: u8 = 0x11;
const STATUS: u8 = 0x27;
const PRESS_OUT_XL: u8 = 0x28;
const TEMP_OUT_L: u8 = 0x2B;

const SWRESET: u8 = 0x04;
/// `IF_ADD_INC`, kept set in the reset write.
const IF_ADD_INC: u8 = 0x10;
/// 10 Hz output data rate, block data update.
const CTRL_REG1_RUN: u8 = 0x22;

/// LPS22HB driver; [`Pressure`](crate::Pressure) is this type.
///
/// ```no_run
/// # use embedded_hal_mock::eh1::{delay::NoopDelay, i2c::Mock};
/// # fn main() -> Result<(), modulino_driver::Error<embedded_hal::i2c::ErrorKind>> {
/// # let i2c = Mock::new(&[]);
/// # let mut delay = NoopDelay::new();
/// use modulino_driver::Pressure;
///
/// let mut pressure = Pressure::new(i2c);
/// pressure.init()?;
/// let hpa = pressure.pressure()?;
/// let celsius = pressure.temperature()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Lps22hb<I2C> {
    regs: Regs<I2C>,
}

impl<I2C: I2c> Lps22hb<I2C> {
    /// The chip at the Modulino's 0x5C. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Address::new(0x5C).expect("0x5C is an address"))
    }

    /// The chip at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            regs: Regs { i2c, addr },
        }
    }

    /// The chip's address.
    pub fn address(&self) -> Address {
        self.regs.addr
    }

    /// Check `WHO_AM_I`.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        super::verify(&mut self.regs.i2c, self.regs.addr, ChipKind::Lps22hb)
    }

    /// Verify, reset, and run at 10 Hz with block data update.
    pub fn init(&mut self) -> Result<(), Error<I2C::Error>> {
        self.verify()?;
        self.regs.write(CTRL_REG2, IF_ADD_INC | SWRESET)?;
        self.regs.wait_clear(CTRL_REG2, SWRESET)?;
        self.regs.write(CTRL_REG1, CTRL_REG1_RUN)
    }

    /// Whether a new pressure sample is ready (`P_DA`).
    pub fn pressure_ready(&mut self) -> Result<bool, Error<I2C::Error>> {
        Ok(self.regs.read(STATUS)? & 0x01 != 0)
    }

    /// Whether a new temperature sample is ready (`T_DA`).
    pub fn temperature_ready(&mut self) -> Result<bool, Error<I2C::Error>> {
        Ok(self.regs.read(STATUS)? & 0x02 != 0)
    }

    /// Pressure in hPa.
    pub fn pressure(&mut self) -> Result<f32, Error<I2C::Error>> {
        let mut b = [0u8; 3];
        self.regs.read_into(PRESS_OUT_XL, &mut b)?;
        // Sign-extend the 24-bit value through the top byte.
        let raw = i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8;
        Ok(raw as f32 / 4096.0)
    }

    /// Temperature in degrees C.
    pub fn temperature(&mut self) -> Result<f32, Error<I2C::Error>> {
        let mut b = [0u8; 2];
        self.regs.read_into(TEMP_OUT_L, &mut b)?;
        Ok(i16::from_le_bytes(b) as f32 / 100.0)
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.regs.i2c
    }
}
