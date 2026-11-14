//! Modulino Buttons: three buttons, A, B and C, each with an LED.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::buttons::{self as codec, ButtonState};
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Driver for the Modulino Buttons module.
///
/// ```rust,ignore
/// use modulino_driver::Buttons;
///
/// let mut buttons = Buttons::new(i2c);
/// buttons.verify()?;
/// let state = buttons.read()?;
/// buttons.set_leds(state.a, state.b, state.c)?;
/// let i2c = buttons.release();
/// ```
#[derive(Debug)]
pub struct Buttons<I2C> {
    node: Node<I2C>,
}

impl<I2C: I2c> Buttons<I2C> {
    /// Buttons at the default address. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::Buttons.default_address())
    }

    /// Buttons at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::Buttons),
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Buttons module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Read which buttons are pressed.
    pub fn read(&mut self) -> Result<ButtonState, Error<I2C::Error>> {
        let payload = self.node.read::<{ codec::READ_PAYLOAD }>()?;
        Ok(codec::decode(&payload))
    }

    /// Set the three LEDs.
    pub fn set_leds(&mut self, a: bool, b: bool, c: bool) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_leds(a, b, c))
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
