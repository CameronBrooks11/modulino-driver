//! Modulino Buzzer: a piezo speaker that plays tones.

use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::codec::buzzer as codec;
use crate::codec::OutOfRange;
use crate::node::Node;
use crate::spec::Kind;
use crate::Error;

/// Musical note frequencies in Hz.
///
/// Notes are named with the note letter, optional sharp (S), and octave number.
/// For example, `A4` is 440 Hz (standard tuning), `CS5` is C# in octave 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u16)]
#[allow(missing_docs)]
pub enum Note {
    // Octave 3
    FS3 = 185,
    G3 = 196,
    GS3 = 208,
    A3 = 220,
    AS3 = 233,
    B3 = 247,

    // Octave 4
    C4 = 262,
    CS4 = 277,
    D4 = 294,
    DS4 = 311,
    E4 = 330,
    F4 = 349,
    FS4 = 370,
    G4 = 392,
    GS4 = 415,
    A4 = 440,
    AS4 = 466,
    B4 = 494,

    // Octave 5
    C5 = 523,
    CS5 = 554,
    D5 = 587,
    DS5 = 622,
    E5 = 659,
    F5 = 698,
    FS5 = 740,
    G5 = 784,
    GS5 = 831,
    A5 = 880,
    AS5 = 932,
    B5 = 988,

    // Octave 6
    C6 = 1047,
    CS6 = 1109,
    D6 = 1175,
    DS6 = 1245,
    E6 = 1319,
    F6 = 1397,
    FS6 = 1480,
    G6 = 1568,
    GS6 = 1661,
    A6 = 1760,
    AS6 = 1865,
    B6 = 1976,

    // Octave 7
    C7 = 2093,
    CS7 = 2217,
    D7 = 2349,
    DS7 = 2489,
    E7 = 2637,
    F7 = 2794,
    FS7 = 2960,
    G7 = 3136,
    GS7 = 3322,
    A7 = 3520,
    AS7 = 3729,
    B7 = 3951,

    // Octave 8
    C8 = 4186,
    CS8 = 4435,
    D8 = 4699,
    DS8 = 4978,

    /// Silence (rest)
    Rest = 0,
}

impl Note {
    /// Get the frequency in Hz.
    pub const fn frequency(&self) -> u16 {
        *self as u16
    }
}

impl From<Note> for u16 {
    fn from(note: Note) -> Self {
        note.frequency()
    }
}

/// Driver for the Modulino Buzzer module.
///
/// ```rust,ignore
/// use modulino_driver::{Buzzer, Note};
///
/// let mut buzzer = Buzzer::new(i2c);
/// buzzer.verify()?;
/// buzzer.tone(440, 500)?;
/// buzzer.play_note(Note::C5, 1000)?;
/// buzzer.no_tone()?;
/// ```
#[derive(Debug)]
pub struct Buzzer<I2C> {
    node: Node<I2C>,
}

impl<I2C: I2c> Buzzer<I2C> {
    /// Buzzer at the default address. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Kind::Buzzer.default_address())
    }

    /// Buzzer at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            node: Node::new(i2c, addr, Kind::Buzzer),
        }
    }

    /// The module's address.
    pub fn address(&self) -> Address {
        self.node.address()
    }

    /// Check that a Buzzer module answers at this address.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.verify()
    }

    /// Play `frequency_hz` for `duration_ms`; 0 plays until the next call.
    ///
    /// Below 180 Hz is `OutOfRange`. A frequency whose frame would start
    /// with a firmware command (17987 Hz encodes as `CF`) is
    /// `ReservedPrefix`.
    pub fn tone(&mut self, frequency_hz: u32, duration_ms: u32) -> Result<(), Error<I2C::Error>> {
        self.node
            .write(&codec::encode_tone(frequency_hz, duration_ms).map_err(OutOfRange::into_error)?)
    }

    /// Play a note; [`Note::Rest`] stops the tone.
    pub fn play_note(&mut self, note: Note, duration_ms: u32) -> Result<(), Error<I2C::Error>> {
        match note {
            Note::Rest => self.no_tone(),
            n => self.tone(n.frequency() as u32, duration_ms),
        }
    }

    /// Stop the tone.
    pub fn no_tone(&mut self) -> Result<(), Error<I2C::Error>> {
        self.node.write(&codec::encode_stop())
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.node.release()
    }
}
