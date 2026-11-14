//! Modulino Knob frames.
//!
//! Read payload: the encoder count (i16 LE), then a byte that is non-zero
//! while the shaft is pressed (main.c:483-488). The firmware also accepts a
//! 4-byte frame that sets the count; this crate never sends it and keeps any
//! offset on the host instead (design D9).

use crate::spec::KNOB;

/// Bytes read after the strap byte.
pub const READ_PAYLOAD: usize = KNOB.read_payload;

/// The knob as the firmware reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct KnobRaw {
    /// Encoder count; wraps at the i16 limits
    pub count: i16,
    /// The shaft is pressed
    pub pressed: bool,
}

/// Decode a read payload.
pub fn decode(payload: &[u8; READ_PAYLOAD]) -> KnobRaw {
    KnobRaw {
        count: i16::from_le_bytes([payload[0], payload[1]]),
        pressed: payload[2] != 0,
    }
}
