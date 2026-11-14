//! Modulino Pixels: eight RGB LEDs.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::pixels::{self as codec, Led, LEDS};
use crate::codec::OutOfRange;
use crate::node::Node;
use crate::spec::Kind;
use crate::{Color, Error};

/// Driver for the Modulino Pixels module.
///
/// The board takes all eight LEDs in one frame, so the driver keeps the
/// frame on the host: the `set` calls change it and [`Pixels::show`] sends
/// it.
///
/// ```rust,ignore
/// use modulino_driver::{Color, Pixels};
///
/// let mut pixels = Pixels::new(i2c);
/// pixels.verify()?;
/// pixels.set(0, Color::RED, 50)?;
/// pixels.show()?;
/// ```
#[derive(Debug)]
pub struct Pixels<I2C> {
    node: Node<I2C>,
    leds: [Led; LEDS],
}

impl<I2C: I2c> Pixels<I2C> {
    /// Pixels at the default address, all LEDs off. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::Pixels.default_address())
    }

    /// Pixels at `addr`, all LEDs off. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::Pixels),
            leds: [Led::default(); LEDS],
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Pixels module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Set LED `index` (0-7) to `color` at `brightness` percent (0-100).
    /// Takes effect on [`Pixels::show`].
    pub fn set(
        &mut self,
        index: usize,
        color: Color,
        brightness: u8,
    ) -> Result<(), Error<I2C::Error>> {
        let brightness =
            codec::brightness_from_percent(brightness).map_err(OutOfRange::into_error)?;
        let led = self.leds.get_mut(index).ok_or(Error::OutOfRange)?;
        *led = Led { color, brightness };
        Ok(())
    }

    /// Set every LED to `color` at `brightness` percent.
    pub fn set_all(&mut self, color: Color, brightness: u8) -> Result<(), Error<I2C::Error>> {
        let brightness =
            codec::brightness_from_percent(brightness).map_err(OutOfRange::into_error)?;
        self.leds = [Led { color, brightness }; LEDS];
        Ok(())
    }

    /// Turn LED `index` off. Takes effect on [`Pixels::show`].
    pub fn clear(&mut self, index: usize) -> Result<(), Error<I2C::Error>> {
        let led = self.leds.get_mut(index).ok_or(Error::OutOfRange)?;
        *led = Led::default();
        Ok(())
    }

    /// Turn every LED off. Takes effect on [`Pixels::show`].
    pub fn clear_all(&mut self) {
        self.leds = [Led::default(); LEDS];
    }

    /// Send the frame to the board.
    pub fn show(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node
            .write(&codec::encode(&self.leds).map_err(OutOfRange::into_error)?)
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
