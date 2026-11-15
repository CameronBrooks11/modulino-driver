//! Modulino Motors frames.
//!
//! The Motors board runs the node firmware built with
//! `MODULINO_MOTORS_BUILD`. Every command is an 8-byte frame: a command
//! letter, its arguments (little-endian) and zero padding (main.c:557-558,
//! motors.c:613-657). The firmware ignores DC commands in stepper mode and
//! stepper commands in DC mode (motors.c:442, 491, 513). No frame can start
//! with a firmware command prefix, since every letter here differs from `C`
//! and `D`.
//!
//! Read payload: current sense A and B (u16 LE, ADC counts above an idle
//! baseline) and a flags byte (motors.c:800-828).

use super::OutOfRange;
use crate::spec::MOTORS;

/// Bytes in a command frame.
pub const WRITE_LEN: usize = MOTORS.write_len.fixed();
/// Bytes read after the strap byte.
pub const READ_PAYLOAD: usize = MOTORS.read_payload;
/// Full-scale DC speed; the firmware clamps to ±this (motors.h:55, motors.c:445-446).
pub const MAX_DC_SPEED: i16 = 32767;
/// Lowest DC PWM frequency the firmware accepts without clamping (motors.c:496).
pub const MIN_DC_FREQUENCY: u16 = 200;
/// Highest DC PWM frequency the firmware accepts without clamping (motors.c:497).
pub const MAX_DC_FREQUENCY: u16 = 60000;

/// Driver mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Mode {
    /// Two independent DC motors
    Dc,
    /// One bipolar stepper
    Stepper,
}

/// Bridge current decay (motors.c:675-679).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum DecayMode {
    /// Slow decay: smoother and quieter
    #[default]
    Slow = 0,
    /// 30 % fast, 70 % slow
    Mixed30Fast70Slow = 1,
    /// 60 % fast, 40 % slow
    Mixed60Fast40Slow = 2,
    /// Fast decay: quicker response, more noise
    Fast = 3,
}

impl DecayMode {
    const fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0 => DecayMode::Slow,
            1 => DecayMode::Mixed30Fast70Slow,
            2 => DecayMode::Mixed60Fast40Slow,
            _ => DecayMode::Fast,
        }
    }
}

/// The board's status report.
///
/// `busy` is cleared only by the end of a move or a new stepper command;
/// switching mode mid-move leaves it set (`Motor_SetMode` never clears it).
/// `release_on_complete` reports that the last stepper command had a
/// release delay above 0 (motors.c:790-792).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub struct MotorStatus {
    /// Motor A current, ADC counts above the idle baseline
    pub current_a_raw: u16,
    /// Motor B current, ADC counts above the idle baseline
    pub current_b_raw: u16,
    /// A stepper move is in progress (flag 0x01)
    pub busy: bool,
    /// Driver mode (flag 0x02)
    pub mode: Mode,
    /// Stepper half-step mode (flag 0x04)
    pub half_step: bool,
    /// Half full-scale current range (flag 0x08)
    pub half_full_scale: bool,
    /// Decay mode (flags 0x30)
    pub decay: DecayMode,
    /// The coils are released when a move completes (flag 0x40)
    pub release_on_complete: bool,
}

fn frame(letter: u8, args: &[u8]) -> [u8; WRITE_LEN] {
    let mut f = [0u8; WRITE_LEN];
    f[0] = letter;
    f[1..1 + args.len()].copy_from_slice(args);
    f
}

/// Encode a mode switch (`M`).
pub fn encode_mode(mode: Mode) -> [u8; WRITE_LEN] {
    frame(b'M', &[(mode == Mode::Stepper) as u8])
}

/// Encode DC speeds for A and B (`S`), each -32767 to 32767.
pub fn encode_dc_speed(a: i16, b: i16) -> Result<[u8; WRITE_LEN], OutOfRange> {
    if a < -MAX_DC_SPEED || b < -MAX_DC_SPEED {
        return Err(OutOfRange);
    }
    let (a, b) = (a.to_le_bytes(), b.to_le_bytes());
    Ok(frame(b'S', &[a[0], a[1], b[0], b[1]]))
}

/// Encode the DC PWM frequency (`F`), 200 to 60 000 Hz.
pub fn encode_dc_frequency(hz: u16) -> Result<[u8; WRITE_LEN], OutOfRange> {
    if !(MIN_DC_FREQUENCY..=MAX_DC_FREQUENCY).contains(&hz) {
        return Err(OutOfRange);
    }
    Ok(frame(b'F', &hz.to_le_bytes()))
}

/// Encode a stepper move (`G`): `steps` (negative is backwards), the step
/// period in 0.1 ms timer ticks (motors.c:316), and how long to keep the
/// coils energised after the move, in ms (0 holds them).
///
/// With `steps` 0 nothing moves; the coils are held, or released after
/// `release_delay_ms` (motors.c:525-540). The firmware treats a period of 0
/// the same way, so a move with `period_ticks` 0 is refused.
pub fn encode_stepper(
    steps: i32,
    period_ticks: u16,
    release_delay_ms: u8,
) -> Result<[u8; WRITE_LEN], OutOfRange> {
    if steps != 0 && period_ticks == 0 {
        return Err(OutOfRange);
    }
    let (s, p) = (steps.to_le_bytes(), period_ticks.to_le_bytes());
    Ok(frame(
        b'G',
        &[s[0], s[1], s[2], s[3], p[0], p[1], release_delay_ms],
    ))
}

/// Encode the decay mode (`T`).
pub fn encode_decay(mode: DecayMode) -> [u8; WRITE_LEN] {
    frame(b'T', &[mode as u8])
}

/// Encode the stepper step mode (`H`): half steps or full steps.
pub fn encode_step_mode(half_step: bool) -> [u8; WRITE_LEN] {
    frame(b'H', &[half_step as u8])
}

/// Encode the current range (`X`): half full-scale or full range.
pub fn encode_half_full_scale(enabled: bool) -> [u8; WRITE_LEN] {
    frame(b'X', &[enabled as u8])
}

/// Decode the status payload.
pub fn decode_status(payload: &[u8; READ_PAYLOAD]) -> MotorStatus {
    let flags = payload[4];
    MotorStatus {
        current_a_raw: u16::from_le_bytes([payload[0], payload[1]]),
        current_b_raw: u16::from_le_bytes([payload[2], payload[3]]),
        busy: flags & 0x01 != 0,
        mode: if flags & 0x02 != 0 {
            Mode::Stepper
        } else {
            Mode::Dc
        },
        half_step: flags & 0x04 != 0,
        half_full_scale: flags & 0x08 != 0,
        decay: DecayMode::from_bits(flags >> 4),
        release_on_complete: flags & 0x40 != 0,
    }
}

/// Convert a raw current reading to mA, with the constants
/// arduino-modulino-mpy uses (motors.py:38-42, 106-109): a 12-bit ADC at
/// 3.3 V, a 4.7 kΩ ISEN resistor, and the MAX22211's KISEN of 7500, or 3750
/// at half full-scale.
pub fn current_ma(raw: u16, half_full_scale: bool) -> f32 {
    let kisen = if half_full_scale { 3750.0 } else { 7500.0 };
    raw as f32 * 3300.0 * kisen / (4095.0 * 4700.0)
}
