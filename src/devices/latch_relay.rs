//! Modulino Latch Relay: a bistable relay that keeps its position without
//! power.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::latch_relay as codec;
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Driver for the Modulino Latch Relay module.
///
/// Its factory address is 0x02, below the range many scanners probe.
///
/// ```rust,ignore
/// use modulino_driver::LatchRelay;
///
/// let mut relay = LatchRelay::new(i2c);
/// relay.verify()?;
/// relay.on()?;
/// assert_eq!(relay.state()?, Some(true));
/// ```
#[derive(Debug)]
pub struct LatchRelay<I2C> {
    node: Node<I2C>,
}

impl<I2C: I2c> LatchRelay<I2C> {
    /// Latch relay at the default address, 0x02. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::LatchRelay.default_address())
    }

    /// Latch relay at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::LatchRelay),
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Latch Relay module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Set (`true`) or reset (`false`) the relay. The firmware pulses the
    /// coil for about 100 ms before it reads the bus again.
    pub fn set(&mut self, on: bool) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode(on))
    }

    /// Set the relay.
    pub fn on(&mut self) -> Result<(), Error<I2C::Error>> {
        self.set(true)
    }

    /// Reset the relay.
    pub fn off(&mut self) -> Result<(), Error<I2C::Error>> {
        self.set(false)
    }

    /// The last commanded position: `None` if the board has had no command
    /// since power-up.
    pub fn state(&mut self) -> Result<Option<bool>, Error<I2C::Error>> {
        Ok(codec::decode(&self.node.read()?))
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
