//! Modulino Joystick frames.
//!
//! Read payload: the X and Y ADC readings (u8, 128 at rest) and a byte that
//! is non-zero while the stick is pressed (main.c:491-495). The board takes
//! no commands.

use crate::spec::JOYSTICK;

/// Bytes read after the strap byte.
pub const READ_PAYLOAD: usize = JOYSTICK.read_payload;
/// Arduino_Modulino's default dead zone around the centre
/// (Modulino.h, `_dz_threshold`).
pub const DEFAULT_DEADZONE: u8 = 26;

/// The joystick as the firmware reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct JoystickRaw {
    /// X reading, 0-255
    pub x: u8,
    /// Y reading, 0-255
    pub y: u8,
    /// The stick is pressed
    pub pressed: bool,
}

/// The joystick centred on zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct JoystickState {
    /// X, -128 to 127, 0 at rest
    pub x: i8,
    /// Y, -128 to 127, 0 at rest
    pub y: i8,
    /// The stick is pressed
    pub pressed: bool,
}

/// Decode a read payload.
pub fn decode(payload: &[u8; READ_PAYLOAD]) -> JoystickRaw {
    JoystickRaw {
        x: payload[0],
        y: payload[1],
        pressed: payload[2] != 0,
    }
}

/// Centre a raw reading as Arduino_Modulino does: `128 - raw`, so the sign
/// matches its `getX`/`getY`. Raw 0 gives 127 rather than overflowing.
pub fn center(raw: u8) -> i8 {
    (128 - raw as i16).clamp(i8::MIN as i16, i8::MAX as i16) as i8
}

/// Centre both axes, reporting (0, 0) while both raw readings are strictly
/// within `deadzone` of 128, as Arduino_Modulino's `map_value` does.
pub fn to_state(raw: JoystickRaw, deadzone: u8) -> JoystickState {
    let near = |v: u8| (v as i16 - 128).abs() < deadzone as i16;
    let (x, y) = if near(raw.x) && near(raw.y) {
        (0, 0)
    } else {
        (center(raw.x), center(raw.y))
    };
    JoystickState {
        x,
        y,
        pressed: raw.pressed,
    }
}
