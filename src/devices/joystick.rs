//! Modulino Joystick: a two-axis stick with a push button.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::joystick::{self as codec, JoystickRaw, JoystickState, DEFAULT_DEADZONE};
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Driver for the Modulino Joystick module.
///
/// ```no_run
/// # use embedded_hal_mock::eh1::{delay::NoopDelay, i2c::Mock};
/// # fn main() -> Result<(), modulino_driver::Error<embedded_hal::i2c::ErrorKind>> {
/// # let i2c = Mock::new(&[]);
/// # let mut delay = NoopDelay::new();
/// use modulino_driver::Joystick;
///
/// let mut joystick = Joystick::new(i2c);
/// joystick.verify()?;
/// let state = joystick.read()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Joystick<I2C> {
    node: Node<I2C>,
    deadzone: u8,
}

impl<I2C: I2c> Joystick<I2C> {
    /// Joystick at the default address. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::Joystick.default_address())
    }

    /// Joystick at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::Joystick),
            deadzone: DEFAULT_DEADZONE,
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Joystick module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// The dead zone around the centre, in raw counts.
    pub fn deadzone(&self) -> u8 {
        self.deadzone
    }

    /// Set the dead zone; 0 turns it off.
    pub fn set_deadzone(&mut self, deadzone: u8) {
        self.deadzone = deadzone;
    }

    /// Read the stick centred on zero, with the dead zone applied.
    pub fn read(&mut self) -> Result<JoystickState, Error<I2C::Error>> {
        Ok(codec::to_state(self.read_raw()?, self.deadzone))
    }

    /// Read the raw ADC values.
    pub fn read_raw(&mut self) -> Result<JoystickRaw, Error<I2C::Error>> {
        Ok(codec::decode(&self.node.read()?))
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
