//! Modulino Opto Relay: a solid-state relay.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::opto_relay as codec;
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Driver for the Modulino Opto Relay module.
///
/// ```no_run
/// # use embedded_hal_mock::eh1::{delay::NoopDelay, i2c::Mock};
/// # fn main() -> Result<(), modulino_driver::Error<embedded_hal::i2c::ErrorKind>> {
/// # let i2c = Mock::new(&[]);
/// # let mut delay = NoopDelay::new();
/// use modulino_driver::OptoRelay;
///
/// let mut relay = OptoRelay::new(i2c);
/// relay.verify()?;
/// relay.on()?;
/// assert!(relay.is_on()?);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct OptoRelay<I2C> {
    node: Node<I2C>,
}

impl<I2C: I2c> OptoRelay<I2C> {
    /// Opto relay at the default address. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::OptoRelay.default_address())
    }

    /// Opto relay at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::OptoRelay),
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that an Opto Relay module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Close (`true`) or open (`false`) the relay.
    pub fn set(&mut self, on: bool) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode(on))
    }

    /// Close the relay.
    pub fn on(&mut self) -> Result<(), Error<I2C::Error>> {
        self.set(true)
    }

    /// Open the relay.
    pub fn off(&mut self) -> Result<(), Error<I2C::Error>> {
        self.set(false)
    }

    /// Read whether the relay is closed.
    pub fn is_on(&mut self) -> Result<bool, Error<I2C::Error>> {
        Ok(codec::decode(&self.node.read()?))
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
