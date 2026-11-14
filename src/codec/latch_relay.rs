//! Modulino Latch Relay frames.
//!
//! Write frame: a byte that sets the relay when non-zero and resets it when
//! zero, then two ignored bytes. The firmware pulses the coil for 100 ms and
//! then drives PA2/PA3 to record the state: PA2 high after a reset, PA3 high
//! after a set (main.c:315-323). Read payload: PA2, PA3, 0 (main.c:501-505).
//! Both low means no command since power-up, so the state is unknown; the
//! relay itself keeps its last position.

use crate::spec::LATCH_RELAY;

/// Bytes read after the strap byte.
pub const READ_PAYLOAD: usize = LATCH_RELAY.read_payload;
/// Bytes in a frame.
pub const WRITE_LEN: usize = LATCH_RELAY.write_len.fixed();

/// Encode a set (`true`) or reset (`false`).
pub fn encode(on: bool) -> [u8; WRITE_LEN] {
    [on as u8, 0, 0]
}

/// Decode a read payload: `Some(true)` after a set, `Some(false)` after a
/// reset, `None` if neither has happened since power-up.
pub fn decode(payload: &[u8; READ_PAYLOAD]) -> Option<bool> {
    match (payload[0] != 0, payload[1] != 0) {
        (true, _) => Some(false),
        (false, true) => Some(true),
        (false, false) => None,
    }
}
