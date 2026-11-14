//! Modulino Vibro: a vibration motor.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::vibro::{self as codec, PowerLevel};
use crate::codec::OutOfRange;
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Driver for the Modulino Vibro module.
///
/// ```rust,ignore
/// use modulino_driver::{PowerLevel, Vibro};
///
/// let mut vibro = Vibro::new(i2c);
/// vibro.verify()?;
/// vibro.on(500, PowerLevel::Medium)?;
/// vibro.off()?;
/// ```
#[derive(Debug)]
pub struct Vibro<I2C> {
    node: Node<I2C>,
}

impl<I2C: I2c> Vibro<I2C> {
    /// Vibro at the default address. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::Vibro.default_address())
    }

    /// Vibro at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::Vibro),
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Vibro module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Vibrate for `duration_ms` at a named power level.
    pub fn on(&mut self, duration_ms: u32, power: PowerLevel) -> Result<(), Error<I2C::Error>> {
        self.on_with_power(duration_ms, power as u8)
    }

    /// Vibrate for `duration_ms` at `power` percent duty, 0-100.
    pub fn on_with_power(&mut self, duration_ms: u32, power: u8) -> Result<(), Error<I2C::Error>> {
        self.node
            .write(&codec::encode_on(duration_ms, power).map_err(OutOfRange::into_error)?)
    }

    /// Stop vibrating.
    pub fn off(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_off())
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
