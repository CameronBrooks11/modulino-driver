//! Modulino Knob: a rotary encoder with a push button.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::knob as codec;
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// The knob's position and button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct KnobState {
    /// Position, including any offset from [`Knob::set_position`]; wraps at
    /// the i16 limits
    pub position: i16,
    /// The shaft is pressed
    pub pressed: bool,
}

/// Driver for the Modulino Knob module.
///
/// The position offset lives on the host: [`Knob::set_position`] never
/// writes to the board (design D9).
///
/// ```rust,ignore
/// use modulino_driver::Knob;
///
/// let mut knob = Knob::new(i2c);
/// knob.verify()?;
/// knob.set_position(0)?;
/// let state = knob.read()?;
/// ```
#[derive(Debug)]
pub struct Knob<I2C> {
    node: Node<I2C>,
    offset: i16,
}

impl<I2C: I2c> Knob<I2C> {
    /// Knob at its primary default address (0x3A). Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::Knob.default_address())
    }

    /// Knob at `addr`, for example the second default, 0x3B. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::Knob),
            offset: 0,
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Knob module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Read the position and button.
    pub fn read(&mut self) -> Result<KnobState, Error<I2C::Error>> {
        let raw = codec::decode(&self.node.read()?);
        Ok(KnobState {
            position: raw.count.wrapping_add(self.offset),
            pressed: raw.pressed,
        })
    }

    /// Make the current position read as `position` from now on.
    pub fn set_position(&mut self, position: i16) -> Result<(), Error<I2C::Error>> {
        let raw = codec::decode(&self.node.read()?);
        self.offset = position.wrapping_sub(raw.count);
        Ok(())
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
