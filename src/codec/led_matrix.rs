//! Modulino LED Matrix frames.
//!
//! The matrix is 12 columns by 8 rows, driven by the node firmware built with
//! `MODULINO_LEDMATRIX_BUILD`. It has two modes, and the write length follows
//! the mode (main.c:555-556):
//!
//! - Monochrome, 12 bytes: byte `col` holds that column, bit `row` set for an
//!   LED that is on (matrix.c:234-262).
//! - Grayscale, 48 bytes: LEDs numbered row by row (`row * 12 + col`), two
//!   per byte, the even LED in the high nibble, brightness 0-15
//!   (matrix.c:211-233).
//!
//! A frame starting `GS4` or `MON` switches the mode instead of being shown
//! (main.c:324-338), and the read payload reports the mode as those same
//! three bytes (main.c:506-516). So a picture whose first bytes spell either
//! command cannot be displayed; [`is_mode_command`] detects it.

use super::OutOfRange;
use crate::spec::{WriteLen, LED_MATRIX};

/// Columns.
pub const COLS: usize = 12;
/// Rows.
pub const ROWS: usize = 8;
/// Bytes read after the strap byte: the mode.
pub const READ_PAYLOAD: usize = LED_MATRIX.read_payload;
/// Bytes in a monochrome frame.
pub const MONO_LEN: usize = match LED_MATRIX.write_len {
    WriteLen::Mode { mono, .. } => mono,
    WriteLen::Fixed(_) => panic!("LED matrix has one write length per mode"),
};
/// Bytes in a grayscale frame.
pub const GRAY_LEN: usize = match LED_MATRIX.write_len {
    WriteLen::Mode { grayscale, .. } => grayscale,
    WriteLen::Fixed(_) => panic!("LED matrix has one write length per mode"),
};
/// The highest grayscale level.
pub const MAX_LEVEL: u8 = 15;

const GRAYSCALE_COMMAND: &[u8; 3] = b"GS4";
const MONO_COMMAND: &[u8; 3] = b"MON";

/// Display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Mode {
    /// One bit per LED
    Mono,
    /// Sixteen levels per LED
    Grayscale,
}

impl Mode {
    /// The frame length the board accepts in this mode.
    pub const fn write_len(self) -> usize {
        match self {
            Mode::Mono => MONO_LEN,
            Mode::Grayscale => GRAY_LEN,
        }
    }
}

/// Decode the read payload: the board's current mode, or `None` for a reply
/// the firmware does not send.
pub fn decode_mode(payload: &[u8; READ_PAYLOAD]) -> Option<Mode> {
    match payload {
        p if p == MONO_COMMAND => Some(Mode::Mono),
        p if p == GRAYSCALE_COMMAND => Some(Mode::Grayscale),
        _ => None,
    }
}

/// A mode command, sized for the mode the board is in when it is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ModeCommand {
    /// For a board in monochrome mode
    Mono([u8; MONO_LEN]),
    /// For a board in grayscale mode
    Grayscale([u8; GRAY_LEN]),
}

/// Encode the command that switches a board in mode `current` to `target`:
/// the command, padded with zeros to `current`'s length, the only length
/// the board accepts.
pub fn encode_mode_command(current: Mode, target: Mode) -> ModeCommand {
    let command = match target {
        Mode::Mono => MONO_COMMAND,
        Mode::Grayscale => GRAYSCALE_COMMAND,
    };
    match current {
        Mode::Mono => {
            let mut f = [0u8; MONO_LEN];
            f[..3].copy_from_slice(command);
            ModeCommand::Mono(f)
        }
        Mode::Grayscale => {
            let mut f = [0u8; GRAY_LEN];
            f[..3].copy_from_slice(command);
            ModeCommand::Grayscale(f)
        }
    }
}

/// Encode a monochrome picture, `pixels[row][col]`.
pub fn encode_mono(pixels: &[[bool; COLS]; ROWS]) -> [u8; MONO_LEN] {
    let mut f = [0u8; MONO_LEN];
    for (row, line) in pixels.iter().enumerate() {
        for (col, &on) in line.iter().enumerate() {
            f[col] |= (on as u8) << row;
        }
    }
    f
}

/// Encode a grayscale picture, `levels[row][col]` from 0 to 15.
pub fn encode_gray(levels: &[[u8; COLS]; ROWS]) -> Result<[u8; GRAY_LEN], OutOfRange> {
    let mut f = [0u8; GRAY_LEN];
    for (i, &level) in levels.iter().flatten().enumerate() {
        if level > MAX_LEVEL {
            return Err(OutOfRange);
        }
        f[i / 2] |= if i % 2 == 0 { level << 4 } else { level };
    }
    Ok(f)
}

/// Whether the firmware would take `frame` as a mode switch rather than a
/// picture.
pub fn is_mode_command(frame: &[u8]) -> bool {
    frame.starts_with(GRAYSCALE_COMMAND) || frame.starts_with(MONO_COMMAND)
}
