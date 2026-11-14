//! Modulino Motors: a dual H-bridge for two DC motors or one stepper.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::motors::{self as codec, DecayMode, Mode, MotorStatus};
use crate::codec::OutOfRange;
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Driver for the Modulino Motors module.
///
/// The firmware ignores DC commands in stepper mode and stepper commands
/// in DC mode, without telling the host; [`Motors::status`] reports the
/// mode.
///
/// ```rust,ignore
/// use modulino_driver::{MotorMode, Motors};
///
/// let mut motors = Motors::new(i2c);
/// motors.verify()?;
/// motors.set_mode(MotorMode::Dc)?;
/// motors.set_speeds(16384, -16384)?;
/// let status = motors.status()?;
/// ```
#[derive(Debug)]
pub struct Motors<I2C> {
    node: Node<I2C>,
}

impl<I2C: I2c> Motors<I2C> {
    /// Motors at the default address. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::Motors.default_address())
    }

    /// Motors at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::Motors),
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Motors module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Switch between two DC motors and one stepper.
    pub fn set_mode(&mut self, mode: Mode) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_mode(mode))
    }

    /// Set DC speeds for A and B, each -32767 to 32767 (DC mode).
    pub fn set_speeds(&mut self, a: i16, b: i16) -> Result<(), Error<I2C::Error>> {
        let frame = codec::encode_dc_speed(a, b).map_err(OutOfRange::into_error)?;
        self.node.write(&frame)
    }

    /// Stop both DC motors (DC mode).
    pub fn stop(&mut self) -> Result<(), Error<I2C::Error>> {
        self.set_speeds(0, 0)
    }

    /// Set the DC PWM frequency, 200 to 60 000 Hz (DC mode).
    pub fn set_dc_frequency(&mut self, hz: u16) -> Result<(), Error<I2C::Error>> {
        let frame = codec::encode_dc_frequency(hz).map_err(OutOfRange::into_error)?;
        self.node.write(&frame)
    }

    /// Move the stepper `steps` (negative is backwards), one step every
    /// `period_ticks` × 0.1 ms, then keep the coils energised for
    /// `release_delay_ms` (0 holds them) (stepper mode).
    pub fn step(
        &mut self,
        steps: i32,
        period_ticks: u16,
        release_delay_ms: u8,
    ) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_stepper(
            steps,
            period_ticks,
            release_delay_ms,
        ))
    }

    /// Energise the stepper coils at the current phase (stepper mode).
    pub fn hold(&mut self) -> Result<(), Error<I2C::Error>> {
        self.step(0, 1, 0)
    }

    /// Release the stepper coils after 1 ms (stepper mode).
    pub fn release_coils(&mut self) -> Result<(), Error<I2C::Error>> {
        self.step(0, 1, 1)
    }

    /// Set the bridge decay mode.
    pub fn set_decay(&mut self, mode: DecayMode) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_decay(mode))
    }

    /// Use half steps (`true`) or full steps for the stepper.
    pub fn set_half_step(&mut self, half_step: bool) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_step_mode(half_step))
    }

    /// Halve the full-scale current (`true`) or use the full range.
    pub fn set_half_full_scale(&mut self, enabled: bool) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_half_full_scale(enabled))
    }

    /// Read currents and flags.
    pub fn status(&mut self) -> Result<MotorStatus, Error<I2C::Error>> {
        Ok(codec::decode_status(&self.node.read()?))
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
