//! Modulino Buzzer frames.
//!
//! Write frame: frequency in Hz (u32 LE), then duration in ms (u32 LE)
//! (main.c:262-283). A duration of 0 plays until the next frame; frequency
//! and duration both 0 stop the tone. The firmware sets its 16-bit timer
//! period to `0xFFFF * 180 / frequency`, so a frequency below 180 Hz
//! overflows it. The read payload carries nothing for this kind.

use super::OutOfRange;
use crate::spec::BUZZER;

/// Bytes in a tone frame.
pub const WRITE_LEN: usize = BUZZER.write_len.fixed();
/// The lowest frequency the firmware's timer can produce.
pub const MIN_FREQUENCY: u32 = 180;

/// Encode a tone. `duration_ms` 0 plays until the next frame.
pub fn encode_tone(frequency_hz: u32, duration_ms: u32) -> Result<[u8; WRITE_LEN], OutOfRange> {
    if frequency_hz < MIN_FREQUENCY {
        return Err(OutOfRange);
    }
    Ok(frame(frequency_hz, duration_ms))
}

/// Encode a stop.
pub fn encode_stop() -> [u8; WRITE_LEN] {
    frame(0, 0)
}

fn frame(frequency_hz: u32, duration_ms: u32) -> [u8; WRITE_LEN] {
    let mut f = [0u8; WRITE_LEN];
    f[..4].copy_from_slice(&frequency_hz.to_le_bytes());
    f[4..].copy_from_slice(&duration_ms.to_le_bytes());
    f
}
