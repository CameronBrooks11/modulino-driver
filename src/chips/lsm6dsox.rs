//! LSM6DSOX accelerometer and gyroscope (Modulino Movement).
//!
//! Registers (ST LSM6DSOX datasheet): `WHO_AM_I` 0x0F = 0x6C, `CTRL1_XL`
//! 0x10, `CTRL2_G` 0x11, `CTRL3_C` 0x12 (`BDU` bit 6, `IF_INC` bit 2,
//! `SW_RESET` bit 0), `STATUS_REG` 0x1E (`XLDA` bit 0, `GDA` bit 1),
//! `OUTX_L_G` 0x22 and `OUTX_L_A` 0x28, each three little-endian i16.
//! The module's SA0 strap selects 0x6A or 0x6B.

use embedded_hal::i2c::I2c;

use super::Regs;
use crate::addr::Address;
use crate::spec::ChipKind;
use crate::Error;

const CTRL1_XL: u8 = 0x10;
const CTRL2_G: u8 = 0x11;
const CTRL3_C: u8 = 0x12;
const STATUS_REG: u8 = 0x1E;
const OUTX_L_G: u8 = 0x22;
const OUTX_L_A: u8 = 0x28;

const SW_RESET: u8 = 0x01;
/// `BDU` and `IF_INC`: outputs update as a unit, reads auto-increment.
const CTRL3_C_RUN: u8 = 0x44;
/// 104 Hz, ±2 g.
const CTRL1_XL_RUN: u8 = 0x40;
/// 104 Hz, ±250 dps.
const CTRL2_G_RUN: u8 = 0x40;
/// mg per LSB at ±2 g.
const ACCEL_MG_PER_LSB: f32 = 0.061;
/// mdps per LSB at ±250 dps.
const GYRO_MDPS_PER_LSB: f32 = 8.75;

const ADDRESSES: [u8; 2] = [0x6A, 0x6B];

/// Three axes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Axes {
    /// X
    pub x: f32,
    /// Y
    pub y: f32,
    /// Z
    pub z: f32,
}

/// LSM6DSOX driver; [`Movement`](crate::Movement) is this type.
///
/// ```rust,ignore
/// use modulino_driver::Movement;
///
/// let mut movement = Movement::new(i2c);
/// movement.init()?; // finds 0x6A or 0x6B, resets, configures
/// let g = movement.acceleration()?;
/// let dps = movement.angular_velocity()?;
/// ```
#[derive(Debug)]
pub struct Lsm6dsox<I2C> {
    regs: Regs<I2C>,
    /// Try the other strap address in `verify` if this one fails.
    fallback: bool,
}

impl<I2C: I2c> Lsm6dsox<I2C> {
    /// The chip at 0x6A, or 0x6B if [`Lsm6dsox::verify`] finds it there.
    /// Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self {
            regs: Regs {
                i2c,
                addr: Address::new(ADDRESSES[0]).expect("0x6A is an address"),
            },
            fallback: true,
        }
    }

    /// The chip at `addr` only. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            regs: Regs { i2c, addr },
            fallback: false,
        }
    }

    /// The address in use: 0x6A until `verify` finds the chip at 0x6B.
    pub fn address(&self) -> Address {
        self.regs.addr
    }

    /// Check `WHO_AM_I`. Built with [`Lsm6dsox::new`], this tries 0x6A and
    /// then 0x6B and keeps the address that answers.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        if !self.fallback {
            return super::verify(&mut self.regs.i2c, self.regs.addr, ChipKind::Lsm6dsox);
        }
        let mut last = Error::WrongChip {
            expected: ChipKind::Lsm6dsox,
        };
        for raw in ADDRESSES {
            let addr = Address::new(raw).expect("strap addresses are valid");
            match super::verify(&mut self.regs.i2c, addr, ChipKind::Lsm6dsox) {
                Ok(()) => {
                    self.regs.addr = addr;
                    self.fallback = false;
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Verify, reset, and configure both sensors at 104 Hz, ±2 g and
    /// ±250 dps with block data update.
    pub fn init(&mut self) -> Result<(), Error<I2C::Error>> {
        self.verify()?;
        self.regs.write(CTRL3_C, SW_RESET)?;
        self.regs.wait_clear(CTRL3_C, SW_RESET)?;
        self.regs.write(CTRL3_C, CTRL3_C_RUN)?;
        self.regs.write(CTRL1_XL, CTRL1_XL_RUN)?;
        self.regs.write(CTRL2_G, CTRL2_G_RUN)
    }

    /// Whether new acceleration or rotation data is ready.
    pub fn data_ready(&mut self) -> Result<bool, Error<I2C::Error>> {
        Ok(self.regs.read(STATUS_REG)? & 0x03 != 0)
    }

    /// Acceleration in g.
    pub fn acceleration(&mut self) -> Result<Axes, Error<I2C::Error>> {
        self.axes(OUTX_L_A, ACCEL_MG_PER_LSB / 1000.0)
    }

    /// Angular velocity in degrees per second.
    pub fn angular_velocity(&mut self) -> Result<Axes, Error<I2C::Error>> {
        self.axes(OUTX_L_G, GYRO_MDPS_PER_LSB / 1000.0)
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.regs.i2c
    }

    fn axes(&mut self, reg: u8, scale: f32) -> Result<Axes, Error<I2C::Error>> {
        let mut b = [0u8; 6];
        self.regs.read_into(reg, &mut b)?;
        let axis = |i: usize| i16::from_le_bytes([b[i], b[i + 1]]) as f32 * scale;
        Ok(Axes {
            x: axis(0),
            y: axis(2),
            z: axis(4),
        })
    }
}
