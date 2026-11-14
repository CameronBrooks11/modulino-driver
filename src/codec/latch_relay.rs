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
/// How long to leave the board alone after a command, in ms.
///
/// The firmware handles the command in its main loop with two
/// `HAL_Delay(50)` calls, each at most 51 ms with the HAL's added tick, and
/// re-reads the command byte from the I2C buffer five times along the way
/// (main.c:315-323). Any I2C transfer to the board meanwhile runs in the
/// I2C interrupt and overwrites that byte: a read stores the strap byte
/// (main.c:476, 571-580), which is non-zero. During a reset, a read in the
/// first 50 ms pulses the set coil; a later one leaves the reset coil
/// driven and the status pins reporting "set". A write during the pulse is
/// half-applied and its own command lost. 120 ms covers the ~102 ms
/// handler with margin; the bench confirms it.
pub const SETTLE_MS: u32 = 120;

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
