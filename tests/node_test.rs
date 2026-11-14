mod support;

use modulino_driver::{Address, Error, Kind, Node};
use support::{FakeBus, FakeNode};

const BUZZER_STRAP: u8 = 0x3C;
const BUZZER_ADDR: u8 = 0x1E;

fn buzzer() -> Node<FakeBus> {
    let bus = FakeBus::new(vec![FakeNode::new(BUZZER_STRAP, &[0; 3], 8)]);
    Node::new(bus, Address::new(BUZZER_ADDR).unwrap(), Kind::Buzzer)
}

#[test]
fn exact_length_write_reaches_the_board() {
    let mut node = buzzer();
    node.write(&[0x40, 0x01, 0, 0, 0xE8, 0x03, 0, 0]).unwrap();
    let bus = node.release();
    assert_eq!(bus.node(BUZZER_ADDR).frames.len(), 1);
}

#[test]
fn short_write_is_refused_before_the_bus() {
    let mut node = buzzer();
    // The 0.3 Vibro off() bug in miniature: a frame shorter than the
    // board's length, which the firmware would drop silently.
    assert_eq!(
        node.write(&[0u8; 4]),
        Err(Error::FrameLength {
            expected: 8,
            got: 4
        })
    );
    let bus = node.release();
    let board = bus.node(BUZZER_ADDR);
    assert_eq!((board.frames.len(), board.dropped), (0, 0));
}

#[test]
fn address_change_prefix_is_refused() {
    let mut node = buzzer();
    // 17987 Hz encodes as 43 46 00 00: "CF" followed by address 0x00.
    let frame = [b'C', b'F', 0x00, 0x00, 0xE8, 0x03, 0x00, 0x00];
    assert_eq!(node.write(&frame), Err(Error::ReservedPrefix));
    let bus = node.release();
    let board = bus.node(BUZZER_ADDR);
    assert_eq!(board.address_changes, 0);
    assert!(board.frames.is_empty());
}

#[test]
fn bootloader_prefix_is_refused() {
    let mut node = buzzer();
    let frame = [b'D', b'I', b'E', 0, 0, 0, 0, 0];
    assert_eq!(node.write(&frame), Err(Error::ReservedPrefix));
    let bus = node.release();
    assert_eq!(bus.node(BUZZER_ADDR).strap, BUZZER_STRAP);
}

#[test]
fn read_longer_than_payload_is_refused() {
    let mut node = buzzer();
    assert_eq!(
        node.read::<4>(),
        Err(Error::FrameLength {
            expected: 3,
            got: 4
        })
    );
}

#[test]
fn read_checks_identity() {
    let bus = FakeBus::new(vec![FakeNode::new(BUZZER_STRAP, &[0; 3], 8)]);
    let mut node = Node::new(bus, Address::new(BUZZER_ADDR).unwrap(), Kind::Vibro);
    assert_eq!(
        node.read::<3>(),
        Err(Error::WrongIdentity {
            expected: Kind::Vibro,
            got: BUZZER_STRAP
        })
    );
}

#[test]
fn identify_returns_the_strap() {
    let mut node = buzzer();
    assert_eq!(node.identify().unwrap().kind(), Some(Kind::Buzzer));
}
