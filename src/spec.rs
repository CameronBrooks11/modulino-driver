//! Protocol constants for the node boards and chip modules.
//!
//! This is the only place in the crate that states them. It mirrors
//! `spec/nodes.toml`, which cites the firmware line behind each value, and a
//! test fails when the two disagree.

/// A node board type, identified by its strap byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
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
    /// The single length of a [`WriteLen::Fixed`] kind.
    ///
    /// # Panics
    ///
    /// On a [`WriteLen::Mode`] kind. In a constant, as the crate uses it,
    /// that fails the build instead.
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub struct NodeSpec {
    /// The kind these constants describe.
    pub kind: Kind,
    /// The kind's name in `spec/nodes.toml`.
    pub name: &'static str,
    /// Strap bytes the firmware reports for this kind; the first gives the
    /// default address.
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
    straps: &[0x74, 0x76],
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
pub const NODES: &[NodeSpec] = &[
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

/// A chip-based module's sensor, identified by evidence rather than address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum ChipKind {
    /// LSM6DSOX accelerometer and gyroscope (Modulino Movement)
    Lsm6dsox,
    /// VL53L4CD time-of-flight sensor (Modulino Distance)
    Vl53l4cd,
    /// VL53L4ED time-of-flight sensor (Modulino Distance variant)
    Vl53l4ed,
    /// HS3003 temperature and humidity sensor (Modulino Thermo)
    Hs3003,
    /// SHT31 temperature and humidity sensor (Modulino Thermo variant)
    Sht31,
    /// LTR-381RGB colour and light sensor (Modulino Light)
    Ltr381rgb,
    /// LPS22HB pressure sensor (Modulino Pressure)
    Lps22hb,
}

/// How a chip proves its identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum ChipId {
    /// Read one byte from an 8-bit register: `(byte & mask) == value`.
    Register8 {
        /// Register index
        register: u8,
        /// Expected value after masking
        value: u8,
        /// Bits compared
        mask: u8,
    },
    /// Read one byte from a 16-bit (big-endian) register.
    Register16 {
        /// Register index
        register: u16,
        /// Expected value after masking
        value: u8,
        /// Bits compared
        mask: u8,
    },
    /// Write a 16-bit command and read status MSB, LSB and CRC-8 (Sensirion
    /// polynomial 0x31, initial 0xFF); a matching CRC is the evidence.
    Sht3xStatus {
        /// Command
        command: u16,
    },
    /// No readable identity.
    None,
}

/// The constants of one chip kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub struct ChipSpec {
    /// The kind these constants describe.
    pub kind: ChipKind,
    /// The kind's name in `spec/chips.toml`.
    pub name: &'static str,
    /// Addresses the Modulino board can use.
    pub addresses: &'static [u8],
    /// How to identify it.
    pub id: ChipId,
}

/// Every chip kind, in `spec/chips.toml` order.
pub const CHIPS: &[ChipSpec] = &[
    ChipSpec {
        kind: ChipKind::Lsm6dsox,
        name: "lsm6dsox",
        addresses: &[0x6A, 0x6B],
        id: ChipId::Register8 {
            register: 0x0F,
            value: 0x6C,
            mask: 0xFF,
        },
    },
    ChipSpec {
        kind: ChipKind::Vl53l4cd,
        name: "vl53l4cd",
        addresses: &[0x29],
        id: ChipId::Register16 {
            register: 0x010F,
            value: 0xEB,
            mask: 0xFF,
        },
    },
    ChipSpec {
        kind: ChipKind::Vl53l4ed,
        name: "vl53l4ed",
        addresses: &[0x29],
        id: ChipId::Register16 {
            register: 0x010F,
            value: 0xEC,
            mask: 0xFF,
        },
    },
    ChipSpec {
        kind: ChipKind::Hs3003,
        name: "hs3003",
        addresses: &[0x44],
        id: ChipId::None,
    },
    ChipSpec {
        kind: ChipKind::Sht31,
        name: "sht31",
        addresses: &[0x45],
        id: ChipId::Sht3xStatus { command: 0xF32D },
    },
    ChipSpec {
        kind: ChipKind::Ltr381rgb,
        name: "ltr381rgb",
        addresses: &[0x53],
        id: ChipId::Register8 {
            register: 0x06,
            value: 0xC0,
            mask: 0xF0,
        },
    },
    ChipSpec {
        kind: ChipKind::Lps22hb,
        name: "lps22hb",
        addresses: &[0x5C],
        id: ChipId::Register8 {
            register: 0x0F,
            value: 0xB1,
            mask: 0xFF,
        },
    },
];

/// Sensirion's CRC-8: polynomial 0x31, initial 0xFF, no reflection.
pub const fn sensirion_crc8(data: &[u8]) -> u8 {
    let mut crc = 0xFFu8;
    let mut i = 0;
    while i < data.len() {
        crc ^= data[i];
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
            bit += 1;
        }
        i += 1;
    }
    crc
}

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
    fn moded_write_len_accepts_each_mode() {
        let w = LED_MATRIX.write_len;
        assert!(w.accepts(12) && w.accepts(48));
        assert!(!w.accepts(0) && !w.accepts(13) && !w.accepts(47));
        assert!(BUTTONS.write_len.accepts(3) && !BUTTONS.write_len.accepts(4));
    }

    #[test]
    fn sensirion_crc_matches_the_datasheet_example() {
        assert_eq!(sensirion_crc8(&[0xBE, 0xEF]), 0x92);
    }

    #[test]
    fn max_read_payload_is_the_largest() {
        let max = NODES.iter().map(|n| n.read_payload).max().unwrap();
        assert_eq!(MAX_READ_PAYLOAD, max);
    }

    #[test]
    fn spec_lookup_matches_table() {
        for n in NODES {
            assert_eq!(n.kind.spec(), n);
        }
    }
}
