//! The fake bus must behave like the firmware it stands in for, or the tests
//! built on it prove nothing. These drive it with raw I2C, below `Node`.

mod support;

use embedded_hal::i2c::I2c;
use support::{FakeBus, FakeNode, BOOTLOADER};

const BUZZER_STRAP: u8 = 0x3C;

fn buzzer_bus() -> FakeBus {
    FakeBus::new(vec![FakeNode::new(BUZZER_STRAP, &[1, 2, 3], 8)])
}

#[test]
fn read_returns_strap_then_payload() {
    let mut bus = buzzer_bus();
    let mut buf = [0u8; 4];
    bus.read(0x1E, &mut buf).unwrap();
    assert_eq!(buf, [BUZZER_STRAP, 1, 2, 3]);
}

#[test]
fn short_write_is_dropped_silently() {
    let mut bus = buzzer_bus();
    bus.write(0x1E, &[0; 7]).unwrap();
    let board = bus.node(0x1E);
    assert_eq!((board.frames.len(), board.dropped), (0, 1));
}

#[test]
fn long_write_is_cut_to_the_board_length() {
    let mut bus = buzzer_bus();
    bus.write(0x1E, &[9, 8, 7, 6, 5, 4, 3, 2, 1, 0]).unwrap();
    assert_eq!(bus.node(0x1E).frames, vec![vec![9, 8, 7, 6, 5, 4, 3, 2]]);
    assert_eq!(bus.node(0x1E).long, 1, "counted as a long write");
}

#[test]
fn cf_moves_the_board() {
    let mut bus = buzzer_bus();
    bus.write(0x1E, &[b'C', b'F', 0x40 << 1, 0, 0, 0, 0, 0])
        .unwrap();
    assert_eq!(bus.node(0x40).address_changes, 1);
}

#[test]
fn cf_zero_or_ff_restores_the_strap_default() {
    for arg in [0x00u8, 0xFF] {
        let mut bus = FakeBus::new(vec![FakeNode::new(BUZZER_STRAP, &[0; 3], 8).at(0x40)]);
        bus.write(0x40, &[b'C', b'F', arg, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(bus.node(0x1E).address_changes, 1, "CF {arg:#04x}");
    }
}

#[test]
fn die_sends_the_board_to_the_bootloader() {
    let mut bus = buzzer_bus();
    bus.write(0x1E, &[b'D', b'I', b'E', 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(bus.node(BOOTLOADER).strap, BUZZER_STRAP);
}

#[test]
fn general_call_reaches_every_node() {
    let mut bus = FakeBus::new(vec![
        FakeNode::new(BUZZER_STRAP, &[0; 3], 8),
        FakeNode::new(0x3C, &[0; 3], 8).at(0x40),
    ]);
    bus.write(0x00, &[1, 0, 0, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(bus.node(0x1E).frames.len(), 1);
    assert_eq!(bus.node(0x40).frames.len(), 1);
}

#[test]
fn absent_address_nacks() {
    let mut bus = buzzer_bus();
    assert!(bus.read(0x20, &mut [0u8; 1]).is_err());
}
