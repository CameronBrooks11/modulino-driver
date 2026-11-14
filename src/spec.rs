//! Protocol constants for the node boards.
//!
//! This is the only place in the crate that states them. It mirrors
//! `spec/nodes.toml`, which cites the firmware line behind each value, and a
//! test fails when the two disagree.

/// A node board type, identified by its strap byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Kind {
    /// Modulino Buttons
    Buttons,
    /// Modulino Buzzer
    Buzzer,
    /// Modulino Vibro
    Vibro,
    /// Modulino Knob
    Knob,
    /// Modulino Pixels
    Pixels,
    /// Modulino Joystick
    Joystick,
    /// Modulino Opto Relay
    OptoRelay,
    /// Modulino Latch Relay
    LatchRelay,
    /// Modulino LED Matrix
    LedMatrix,
    /// Modulino Motors
    Motors,
}

/// The write lengths a node accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteLen {
    /// One length.
    Fixed(usize),
    /// One length per display mode (LED matrix).
    Mode {
        /// Monochrome frame length
        mono: usize,
        /// Grayscale frame length
        grayscale: usize,
    },
}

impl WriteLen {
    /// The single length of a [`WriteLen::Fixed`] kind. Used in constants, so
    /// calling it on a moded kind fails the build.
    pub const fn fixed(self) -> usize {
        match self {
            WriteLen::Fixed(n) => n,
            WriteLen::Mode { .. } => panic!("kind has one write length per mode"),
        }
    }

    /// Whether a frame of `len` bytes is one this node accepts.
    pub const fn accepts(self, len: usize) -> bool {
        match self {
            WriteLen::Fixed(n) => len == n,
            WriteLen::Mode { mono, grayscale } => len == mono || len == grayscale,
        }
    }
}

/// The protocol constants of one node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSpec {
    /// The kind these constants describe.
    pub kind: Kind,
    /// The kind's name in `spec/nodes.toml`.
    pub name: &'static str,
    /// Strap bytes the firmware reports for this kind, primary first.
    pub straps: &'static [u8],
    /// Bytes a read returns after the strap byte.
    pub read_payload: usize,
    /// Accepted write lengths.
    pub write_len: WriteLen,
}

/// Modulino Buttons.
pub const BUTTONS: NodeSpec = NodeSpec {
    kind: Kind::Buttons,
    name: "buttons",
    straps: &[0x7C],
    read_payload: 3,
    write_len: WriteLen::Fixed(3),
};

/// Modulino Buzzer.
pub const BUZZER: NodeSpec = NodeSpec {
    kind: Kind::Buzzer,
    name: "buzzer",
    straps: &[0x3C],
    read_payload: 3,
    write_len: WriteLen::Fixed(8),
};

/// Modulino Vibro.
pub const VIBRO: NodeSpec = NodeSpec {
    kind: Kind::Vibro,
    name: "vibro",
    straps: &[0x70],
    read_payload: 3,
    write_len: WriteLen::Fixed(12),
};

/// Modulino Knob.
pub const KNOB: NodeSpec = NodeSpec {
    kind: Kind::Knob,
    name: "knob",
    straps: &[0x76, 0x74],
    read_payload: 3,
    write_len: WriteLen::Fixed(4),
};

/// Modulino Pixels.
pub const PIXELS: NodeSpec = NodeSpec {
    kind: Kind::Pixels,
    name: "pixels",
    straps: &[0x6C],
    read_payload: 32,
    write_len: WriteLen::Fixed(32),
};

/// Modulino Joystick.
pub const JOYSTICK: NodeSpec = NodeSpec {
    kind: Kind::Joystick,
    name: "joystick",
    straps: &[0x58],
    read_payload: 3,
    write_len: WriteLen::Fixed(3),
};

/// Modulino Opto Relay.
pub const OPTO_RELAY: NodeSpec = NodeSpec {
    kind: Kind::OptoRelay,
    name: "opto_relay",
    straps: &[0x28],
    read_payload: 3,
    write_len: WriteLen::Fixed(3),
};

/// Modulino Latch Relay.
pub const LATCH_RELAY: NodeSpec = NodeSpec {
    kind: Kind::LatchRelay,
    name: "latch_relay",
    straps: &[0x04],
    read_payload: 3,
    write_len: WriteLen::Fixed(3),
};

/// Modulino LED Matrix.
pub const LED_MATRIX: NodeSpec = NodeSpec {
    kind: Kind::LedMatrix,
    name: "led_matrix",
    straps: &[0x72],
    read_payload: 3,
    write_len: WriteLen::Mode {
        mono: 12,
        grayscale: 48,
    },
};

/// Modulino Motors.
pub const MOTORS: NodeSpec = NodeSpec {
    kind: Kind::Motors,
    name: "motors",
    straps: &[0x48],
    read_payload: 5,
    write_len: WriteLen::Fixed(8),
};

/// Every node kind, in `spec/nodes.toml` order.
pub const NODES: [NodeSpec; 10] = [
    BUTTONS,
    BUZZER,
    VIBRO,
    KNOB,
    PIXELS,
    JOYSTICK,
    OPTO_RELAY,
    LATCH_RELAY,
    LED_MATRIX,
    MOTORS,
];

/// The longest read payload of any node kind.
pub const MAX_READ_PAYLOAD: usize = 32;

impl Kind {
    /// This kind's protocol constants.
    pub const fn spec(self) -> &'static NodeSpec {
        match self {
            Kind::Buttons => &BUTTONS,
            Kind::Buzzer => &BUZZER,
            Kind::Vibro => &VIBRO,
            Kind::Knob => &KNOB,
            Kind::Pixels => &PIXELS,
            Kind::Joystick => &JOYSTICK,
            Kind::OptoRelay => &OPTO_RELAY,
            Kind::LatchRelay => &LATCH_RELAY,
            Kind::LedMatrix => &LED_MATRIX,
            Kind::Motors => &MOTORS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_read_payload_is_the_largest() {
        let max = NODES.iter().map(|n| n.read_payload).max().unwrap();
        assert_eq!(MAX_READ_PAYLOAD, max);
    }

    #[test]
    fn spec_lookup_matches_table() {
        for n in NODES {
            assert_eq!(n.kind.spec(), &n);
        }
    }
}
