//! LTR-381RGB colour and ambient light sensor (Modulino Light).
//!
//! Registers, as modulino-rs 0.3 used them (`light.rs`) and the Lite-On
//! datasheet gives them as I know it: `MAIN_CTRL` 0x00 (0x06 enables the
//! colour sensor in RGB mode), `MEAS_RATE` 0x04 (resolution in bits 6-4,
//! rate in bits 2-0), `GAIN` 0x05, `PART_ID` 0x06 (part number 0xC in the
//! high nibble), `MAIN_STATUS` 0x07, then the 20-bit channels as three
//! little-endian bytes each: IR 0x0A, green 0x0D, red 0x10, blue 0x13.
//!
//! The four channels are read in one 12-byte burst from 0x0A, so they come
//! from the same measurement; the bench confirms the chip auto-increments
//! across them. The lux formula and the defaults (18x gain, 16-bit, 25 ms)
//! are those of modulino-rs 0.3, which took them from Arduino's library.

use embedded_hal::i2c::I2c;

use super::Regs;
use crate::addr::Address;
use crate::spec::ChipKind;
use crate::Error;

const MAIN_CTRL: u8 = 0x00;
const MEAS_RATE: u8 = 0x04;
const GAIN: u8 = 0x05;
const MAIN_STATUS: u8 = 0x07;
const DATA_IR: u8 = 0x0A;

/// Colour sensor on, RGB mode.
const MAIN_CTRL_RGB: u8 = 0x06;
/// Channel values are 20 bits.
const CHANNEL_MASK: u32 = 0x000F_FFFF;

/// Analogue gain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Gain {
    /// 1x
    X1 = 0x00,
    /// 3x
    X3 = 0x01,
    /// 6x
    X6 = 0x02,
    /// 9x
    X9 = 0x03,
    /// 18x, the default
    X18 = 0x04,
}

impl Gain {
    /// The gain as a multiplier.
    pub fn factor(self) -> f32 {
        match self {
            Gain::X1 => 1.0,
            Gain::X3 => 3.0,
            Gain::X6 => 6.0,
            Gain::X9 => 9.0,
            Gain::X18 => 18.0,
        }
    }
}

/// ADC resolution, which sets the integration time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Resolution {
    /// 20 bits, 400 ms
    Bits20 = 0x00,
    /// 19 bits, 200 ms
    Bits19 = 0x01,
    /// 18 bits, 100 ms
    Bits18 = 0x02,
    /// 17 bits, 50 ms
    Bits17 = 0x03,
    /// 16 bits, 25 ms, the default
    Bits16 = 0x04,
}

impl Resolution {
    /// Integration time divided by 100 ms.
    pub fn time_factor(self) -> f32 {
        match self {
            Resolution::Bits20 => 4.0,
            Resolution::Bits19 => 2.0,
            Resolution::Bits18 => 1.0,
            Resolution::Bits17 => 0.5,
            Resolution::Bits16 => 0.25,
        }
    }
}

/// Time between measurements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Rate {
    /// 25 ms, the default
    Ms25 = 0x00,
    /// 50 ms
    Ms50 = 0x01,
    /// 100 ms
    Ms100 = 0x02,
    /// 200 ms
    Ms200 = 0x03,
    /// 400 ms
    Ms400 = 0x04,
    /// 500 ms
    Ms500 = 0x05,
    /// 1 s
    Ms1000 = 0x06,
    /// 2 s
    Ms2000 = 0x07,
}

/// One reading of all four channels.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LightReading {
    /// Red channel, raw counts
    pub red: u32,
    /// Green channel, raw counts (also the ambient light channel)
    pub green: u32,
    /// Blue channel, raw counts
    pub blue: u32,
    /// Infrared channel, raw counts
    pub ir: u32,
    /// Ambient light in lux, from green and IR
    pub lux: f32,
}

/// Ambient light in lux from the green and IR channels:
/// `0.8 * green / (gain * integration / 100 ms)`, scaled by
/// `1 - 0.033 * IR / green` with the ratio capped at 30 (so the factor
/// stays at or above 0.01).
pub fn lux(green: u32, ir: u32, gain: Gain, resolution: Resolution) -> f32 {
    if green == 0 {
        return 0.0;
    }
    let lux = 0.8 * green as f32 / (gain.factor() * resolution.time_factor());
    let ratio = (ir as f32 / green as f32).min(30.0);
    lux * (1.0 - 0.033 * ratio)
}

/// LTR-381RGB driver; [`Light`](crate::Light) is this type.
///
/// ```rust,ignore
/// use modulino_driver::Light;
///
/// let mut light = Light::new(i2c);
/// light.init()?;
/// let reading = light.read()?;
/// ```
#[derive(Debug)]
pub struct Ltr381rgb<I2C> {
    regs: Regs<I2C>,
    gain: Gain,
    resolution: Resolution,
}

impl<I2C: I2c> Ltr381rgb<I2C> {
    /// The chip at the Modulino's 0x53. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Address::new(0x53).expect("0x53 is an address"))
    }

    /// The chip at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            regs: Regs { i2c, addr },
            gain: Gain::X18,
            resolution: Resolution::Bits16,
        }
    }

    /// The chip's address.
    pub fn address(&self) -> Address {
        self.regs.addr
    }

    /// Check `PART_ID`.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        super::verify(&mut self.regs.i2c, self.regs.addr, ChipKind::Ltr381rgb)
    }

    /// Verify, clear the status, and start measuring at 18x gain, 16 bits,
    /// every 25 ms.
    pub fn init(&mut self) -> Result<(), Error<I2C::Error>> {
        self.verify()?;
        self.regs.read(MAIN_STATUS)?;
        self.set_gain(Gain::X18)?;
        self.set_timing(Resolution::Bits16, Rate::Ms25)?;
        self.regs.write(MAIN_CTRL, MAIN_CTRL_RGB)
    }

    /// Set the gain. The lux calculation follows it.
    pub fn set_gain(&mut self, gain: Gain) -> Result<(), Error<I2C::Error>> {
        self.regs.write(GAIN, gain as u8)?;
        self.gain = gain;
        Ok(())
    }

    /// Set the resolution and the measurement rate. The lux calculation
    /// follows the resolution.
    pub fn set_timing(
        &mut self,
        resolution: Resolution,
        rate: Rate,
    ) -> Result<(), Error<I2C::Error>> {
        self.regs
            .write(MEAS_RATE, (resolution as u8) << 4 | rate as u8)?;
        self.resolution = resolution;
        Ok(())
    }

    /// Read all four channels in one burst, and the lux they give.
    pub fn read(&mut self) -> Result<LightReading, Error<I2C::Error>> {
        let mut b = [0u8; 12];
        self.regs.read_into(DATA_IR, &mut b)?;
        let channel = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], 0]) & CHANNEL_MASK;
        let (ir, green, red, blue) = (channel(0), channel(3), channel(6), channel(9));
        Ok(LightReading {
            red,
            green,
            blue,
            ir,
            lux: lux(green, ir, self.gain, self.resolution),
        })
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.regs.i2c
    }
}
