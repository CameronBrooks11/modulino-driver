//! Modulino LED Matrix: 12 by 8 LEDs, monochrome or 16-level grayscale.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::led_matrix::{self as codec, Mode, COLS, GRAY_LEN, MONO_LEN, ROWS};
use crate::codec::OutOfRange;
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Driver for the Modulino LED Matrix module.
///
/// The board's write length depends on its mode, so the driver tracks the
/// mode. [`LedMatrix::verify`] and [`LedMatrix::set_mode`] read it from the
/// board; the `show_*` calls trust the tracked mode. If the board may have
/// been reset (it boots monochrome) or switched by another host, call
/// `verify()` to read it again. Switching mode leaves the old picture
/// reinterpreted on screen until the next one is shown (matrix.c:174-181).
///
/// ```rust,ignore
/// use modulino_driver::{LedMatrix, MatrixMode};
///
/// let mut matrix = LedMatrix::new(i2c);
/// matrix.verify()?;
/// let mut picture = [[false; 12]; 8];
/// picture[0][0] = true;
/// matrix.set_mode(MatrixMode::Mono)?;
/// matrix.show_mono(&picture)?;
/// ```
#[derive(Debug)]
pub struct LedMatrix<I2C> {
    node: Node<I2C>,
    mode: Option<Mode>,
}

impl<I2C: I2c> LedMatrix<I2C> {
    /// LED matrix at the default address. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::LedMatrix.default_address())
    }

    /// LED matrix at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::LedMatrix),
            mode: None,
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that an LED matrix answers at this address, and read its mode.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.read_mode().map(|_| ())
    }

    /// The board's mode: the tracked one, or read from the board if not
    /// known yet.
    pub fn mode(&mut self) -> Result<Mode, Error<I2C::Error>> {
        match self.mode {
            Some(mode) => Ok(mode),
            None => self.read_mode(),
        }
    }

    /// Switch the board to `target`. The board's current mode is read
    /// first, so the command can be padded to that mode's length, the only
    /// length the firmware accepts.
    pub fn set_mode(&mut self, target: Mode) -> Result<(), Error<I2C::Error>> {
        let sent = match self.read_mode()? {
            Mode::Mono => self
                .node
                .write(&codec::encode_mode_command::<MONO_LEN>(target)),
            Mode::Grayscale => self
                .node
                .write(&codec::encode_mode_command::<GRAY_LEN>(target)),
        };
        // After a failed write the board may or may not have switched.
        self.mode = sent.as_ref().ok().map(|_| target);
        sent
    }

    /// Show a monochrome picture, `pixels[row][col]`.
    ///
    /// `FrameLength` if the board is in grayscale mode. `ReservedPrefix` if
    /// the frame would start `GS4` or `MON`, which the board takes as a
    /// mode switch, or `CF`/`DIE`.
    pub fn show_mono(&mut self, pixels: &[[bool; COLS]; ROWS]) -> Result<(), Error<I2C::Error>> {
        self.expect_mode(Mode::Mono)?;
        self.send(&codec::encode_mono(pixels))
    }

    /// Show a grayscale picture, `levels[row][col]` from 0 to 15.
    ///
    /// `FrameLength` if the board is in monochrome mode, `OutOfRange` for a
    /// level above 15, and `ReservedPrefix` as for [`LedMatrix::show_mono`].
    pub fn show_gray(&mut self, levels: &[[u8; COLS]; ROWS]) -> Result<(), Error<I2C::Error>> {
        let frame = codec::encode_gray(levels).map_err(OutOfRange::into_error)?;
        self.expect_mode(Mode::Grayscale)?;
        self.send(&frame)
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }

    fn read_mode(&mut self) -> Result<Mode, Error<I2C::Error>> {
        let payload = self.node.read::<{ codec::READ_PAYLOAD }>()?;
        let mode = codec::decode_mode(&payload).ok_or(Error::DataError)?;
        self.mode = Some(mode);
        Ok(mode)
    }

    fn expect_mode(&mut self, frame_mode: Mode) -> Result<(), Error<I2C::Error>> {
        let mode = self.mode()?;
        if mode == frame_mode {
            Ok(())
        } else {
            Err(Error::FrameLength {
                expected: mode.write_len(),
                got: frame_mode.write_len(),
            })
        }
    }

    fn send<const N: usize>(&mut self, frame: &[u8; N]) -> Result<(), Error<I2C::Error>> {
        if codec::is_mode_command(frame) {
            return Err(Error::ReservedPrefix);
        }
        let sent = self.node.write(frame);
        if sent.is_err() {
            self.mode = None;
        }
        sent
    }
}
