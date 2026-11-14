//! Modulino Pixels frames.
//!
//! Write frame: eight 4-byte LED words, each `0xE0 | brightness`, blue,
//! green, red, with brightness 0-31, passed straight to the LEDs
//! (main.c:307-309, `show_leds`). Every word starts at 0xE0 or above, so no
//! frame can begin with a firmware command prefix. The read payload carries
//! nothing for this kind.

use super::OutOfRange;
use crate::spec::PIXELS;
use crate::Color;

/// Bytes in a frame.
pub const WRITE_LEN: usize = PIXELS.write_len.fixed();
/// LEDs on the board.
pub const LEDS: usize = WRITE_LEN / 4;
/// The highest LED brightness.
pub const MAX_BRIGHTNESS: u8 = 31;

/// One LED: a colour and a brightness from 0 to 31.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Led {
    /// Colour
    pub color: Color,
    /// Brightness, 0-31
    pub brightness: u8,
}

/// Map a brightness percentage to the LED's 0-31 scale as Arduino_Modulino
/// does (`map(brightness, 0, 100, 0, 0x1F)`, which rounds down).
pub fn brightness_from_percent(percent: u8) -> Result<u8, OutOfRange> {
    if percent > 100 {
        return Err(OutOfRange);
    }
    Ok((percent as u16 * MAX_BRIGHTNESS as u16 / 100) as u8)
}

/// Encode all eight LEDs.
pub fn encode(leds: &[Led; LEDS]) -> Result<[u8; WRITE_LEN], OutOfRange> {
    let mut f = [0u8; WRITE_LEN];
    for (word, led) in f.chunks_exact_mut(4).zip(leds) {
        if led.brightness > MAX_BRIGHTNESS {
            return Err(OutOfRange);
        }
        word.copy_from_slice(&[0xE0 | led.brightness, led.color.b, led.color.g, led.color.r]);
    }
    Ok(f)
}
