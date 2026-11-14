//! Modulino Vibro frames.
//!
//! Write frame: PWM frequency in Hz, duration in ms, duty in percent, each
//! u32 LE (main.c:284-297). The motor stops once the duration has passed
//! (main.c:182-185), so a duration of 0 stops it within a millisecond and is
//! refused by [`encode_on`]; the all-zero frame is the stop. Its zero
//! frequency divides by zero in the firmware, which on the Cortex-M0+ yields
//! 0. The stop time `tick + duration` also wraps for durations near
//! `u32::MAX`, stopping the motor almost at once. The read payload carries
//! nothing for this kind.

use super::OutOfRange;
use crate::spec::VIBRO;

/// Bytes in a frame.
pub const WRITE_LEN: usize = VIBRO.write_len.fixed();
/// PWM frequency Arduino's libraries drive the motor at.
pub const FREQUENCY: u32 = 1000;
/// The highest meaningful duty, in percent.
pub const MAX_POWER: u8 = 100;

/// Vibration strength, as Arduino_Modulino names it (Modulino.h:28-36).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum PowerLevel {
    /// Motor stopped
    Stop = 0,
    /// Gentle vibration
    Gentle = 25,
    /// Moderate vibration
    Moderate = 30,
    /// Medium vibration
    #[default]
    Medium = 35,
    /// Intense vibration
    Intense = 40,
    /// Powerful vibration
    Powerful = 45,
    /// Maximum vibration
    Maximum = 50,
}

/// Encode a vibration of `duration_ms` (at least 1) at `power` percent duty.
pub fn encode_on(duration_ms: u32, power: u8) -> Result<[u8; WRITE_LEN], OutOfRange> {
    if duration_ms == 0 || power > MAX_POWER {
        return Err(OutOfRange);
    }
    Ok(frame(FREQUENCY, duration_ms, power as u32))
}

/// Encode a stop: the full-length frame of zeros.
pub fn encode_off() -> [u8; WRITE_LEN] {
    frame(0, 0, 0)
}

fn frame(frequency: u32, duration_ms: u32, duty: u32) -> [u8; WRITE_LEN] {
    let mut f = [0u8; WRITE_LEN];
    f[..4].copy_from_slice(&frequency.to_le_bytes());
    f[4..8].copy_from_slice(&duration_ms.to_le_bytes());
    f[8..].copy_from_slice(&duty.to_le_bytes());
    f
}
