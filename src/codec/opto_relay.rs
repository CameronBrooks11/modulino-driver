//! Modulino Opto Relay frames.
//!
//! Write frame: a byte that closes the relay when non-zero, then two
//! ignored bytes (main.c:312-314). Read payload: the relay pin, then two
//! zero bytes (main.c:496-500).

use crate::spec::OPTO_RELAY;

/// Bytes read after the strap byte.
pub const READ_PAYLOAD: usize = OPTO_RELAY.read_payload;
/// Bytes in a frame.
pub const WRITE_LEN: usize = OPTO_RELAY.write_len.fixed();

/// Encode the relay state.
pub fn encode(on: bool) -> [u8; WRITE_LEN] {
    [on as u8, 0, 0]
}

/// Decode a read payload: whether the relay is closed.
pub fn decode(payload: &[u8; READ_PAYLOAD]) -> bool {
    payload[0] != 0
}
