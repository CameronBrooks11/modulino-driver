//! Modulino Buttons frames.
//!
//! Read payload: one byte per button A, B, C, non-zero while pressed.
//! Write frame: one byte per LED A, B, C, non-zero for on.

use crate::spec::BUTTONS;

/// Bytes read after the strap byte.
pub const READ_PAYLOAD: usize = BUTTONS.read_payload;
/// Bytes in an LED frame.
pub const WRITE_LEN: usize = BUTTONS.write_len.fixed();

/// Which buttons are pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ButtonState {
    /// Button A is pressed
    pub a: bool,
    /// Button B is pressed
    pub b: bool,
    /// Button C is pressed
    pub c: bool,
}

impl ButtonState {
    /// Whether any button is pressed.
    pub fn any_pressed(&self) -> bool {
        self.a || self.b || self.c
    }

    /// Whether every button is pressed.
    pub fn all_pressed(&self) -> bool {
        self.a && self.b && self.c
    }
}

/// Decode a read payload.
pub fn decode(payload: &[u8; READ_PAYLOAD]) -> ButtonState {
    ButtonState {
        a: payload[0] != 0,
        b: payload[1] != 0,
        c: payload[2] != 0,
    }
}

/// Encode an LED frame.
pub fn encode_leds(a: bool, b: bool, c: bool) -> [u8; WRITE_LEN] {
    [a as u8, b as u8, c as u8]
}
