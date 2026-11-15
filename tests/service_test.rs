//! Address change and general-call reset against the simulated firmware,
//! which reboots after a `CF` and answers only once the host has waited.

mod support;

use modulino_driver::spec::{NodeSpec, WriteLen, NODES};
use modulino_driver::{service, Address, Error, Kind, NewAddress, Node};
use support::{FakeBus, FakeChip, FakeDelay, FakeNode, Log};

const MS: u64 = 1_000_000;

// Waits are checked against AddressChanger's 500 ms (AddressChanger.ino:151),
// not against `service::RESET_MS`, so a change to the constant shows.

fn a(raw: u8) -> Address {
    Address::new(raw).unwrap()
}

fn new(raw: u8) -> NewAddress {
    NewAddress::new(raw).unwrap()
}

/// A bus and a delay that share a log, so the boards reboot after a `CF`
/// until the delay runs.
fn bus_and_delay(nodes: Vec<FakeNode>) -> (FakeBus, FakeDelay) {
    let log = Log::default();
    let bus = FakeBus {
        nodes,
        log: Some(log.clone()),
        ..Default::default()
    };
    let delay = FakeDelay {
        log: Some(log),
        ..Default::default()
    };
    (bus, delay)
}

/// A board of `spec`'s kind at its first strap default, plus the boards of
/// a mode kind (the LED matrix) in each mode.
fn boards() -> Vec<(Kind, FakeNode)> {
    let mut out = Vec::new();
    for spec in NODES.iter() {
        match spec.write_len {
            WriteLen::Fixed(len) => out.push((spec.kind, board(spec, len))),
            WriteLen::Mode { .. } => {
                out.push((spec.kind, FakeNode::led_matrix(false)));
                out.push((spec.kind, FakeNode::led_matrix(true)));
            }
        }
    }
    out
}

fn board(spec: &NodeSpec, len: usize) -> FakeNode {
    FakeNode::new(spec.straps[0], &vec![0; spec.read_payload], len)
}

#[test]
fn every_kind_moves_to_a_new_address() {
    for (kind, board) in boards() {
        let from = board.addr;
        let label = format!("{kind:?} write_len {}", board.write_len);
        let (bus, mut delay) = bus_and_delay(vec![board]);
        let mut node = Node::new(bus, a(from), kind);
        service::set_address(&mut node, new(0x40), &mut delay).expect(&label);
        assert_eq!(node.address(), a(0x40), "{label}");
        assert!(delay.total_ns >= 500 * MS, "{label}");
        let bus = node.release();
        let board = bus.node(0x40);
        assert_eq!(
            (board.address_changes, board.dropped, board.long),
            (1, 0, 0),
            "{label}: one CF frame of exactly the board's length"
        );
    }
}

#[test]
fn reset_returns_every_kind_to_its_strap_default() {
    for (kind, board) in boards() {
        let default = board.addr;
        let label = format!("{kind:?} write_len {}", board.write_len);
        let (bus, mut delay) = bus_and_delay(vec![board.at(0x40)]);
        let mut node = Node::new(bus, a(0x40), kind);
        service::reset_address(&mut node, &mut delay).expect(&label);
        assert_eq!(node.address(), a(default), "{label}");
        let bus = node.release();
        let board = bus.node(default);
        assert_eq!(
            (board.address_changes, board.dropped, board.long),
            (1, 0, 0),
            "{label}"
        );
    }
}

#[test]
fn reset_uses_the_boards_own_strap() {
    // A Knob strapped 0x76 defaults to 0x3B, not the kind's first 0x3A.
    let knob = FakeNode::new(0x76, &[0; 3], 4).at(0x40);
    let (bus, mut delay) = bus_and_delay(vec![knob]);
    let mut node = Node::new(bus, a(0x40), Kind::Knob);
    service::reset_address(&mut node, &mut delay).unwrap();
    assert_eq!(node.address(), a(0x3B));
}

#[test]
fn an_address_in_use_is_refused_before_anything_is_sent() {
    let buttons = FakeNode::new(0x7C, &[0; 3], 3);
    let buzzer = FakeNode::new(0x3C, &[0; 3], 8).at(0x40);
    let (bus, mut delay) = bus_and_delay(vec![buttons, buzzer]);
    let mut node = Node::new(bus, a(0x3E), Kind::Buttons);
    assert_eq!(
        service::set_address(&mut node, new(0x40), &mut delay),
        Err(Error::InvalidAddress)
    );
    assert_eq!(node.address(), a(0x3E));
    assert_eq!(delay.total_ns, 0, "no reset to wait for");
    let bus = node.release();
    let buttons = bus.node(0x3E);
    assert_eq!(
        (
            buttons.address_changes,
            buttons.frames.len(),
            buttons.dropped
        ),
        (0, 0, 0)
    );
}

#[test]
fn a_chip_at_the_target_counts_as_in_use() {
    let buttons = FakeNode::new(0x7C, &[0; 3], 3);
    let (mut bus, mut delay) = bus_and_delay(vec![buttons]);
    bus.chips.push(FakeChip::new(0x6A, 1));
    let mut node = Node::new(bus, a(0x3E), Kind::Buttons);
    assert_eq!(
        service::set_address(&mut node, new(0x6A), &mut delay),
        Err(Error::InvalidAddress)
    );
}

#[test]
fn a_reset_onto_an_occupied_default_is_refused() {
    // Buttons moved to 0x40, and another Buttons board at the default 0x3E.
    let moved = FakeNode::new(0x7C, &[0; 3], 3).at(0x40);
    let other = FakeNode::new(0x7C, &[0; 3], 3);
    let (bus, mut delay) = bus_and_delay(vec![moved, other]);
    let mut node = Node::new(bus, a(0x40), Kind::Buttons);
    assert_eq!(
        service::reset_address(&mut node, &mut delay),
        Err(Error::InvalidAddress)
    );
    assert_eq!(node.release().node(0x40).address_changes, 0);
}

#[test]
fn moving_to_the_current_address_sends_nothing() {
    let (bus, mut delay) = bus_and_delay(vec![FakeNode::new(0x7C, &[0; 3], 3)]);
    let mut node = Node::new(bus, a(0x3E), Kind::Buttons);
    service::set_address(&mut node, new(0x3E), &mut delay).unwrap();
    service::reset_address(&mut node, &mut delay).unwrap();
    assert_eq!(delay.total_ns, 0);
    let bus = node.release();
    assert_eq!(bus.node(0x3E).address_changes, 0);
    // A strap read for set_address, two for reset_address, nothing else.
    assert_eq!(bus.transactions, 3);
}

#[test]
fn the_wrong_kind_is_refused_before_anything_is_sent() {
    let (bus, mut delay) = bus_and_delay(vec![FakeNode::new(0x3C, &[0; 3], 8)]);
    let mut node = Node::new(bus, a(0x1E), Kind::Buttons);
    assert_eq!(
        service::set_address(&mut node, new(0x40), &mut delay),
        Err(Error::WrongIdentity {
            expected: Kind::Buttons,
            got: 0x3C
        })
    );
    assert_eq!(node.release().node(0x1E).address_changes, 0);
}

#[test]
fn a_board_that_does_not_come_back_keeps_the_old_address() {
    // A delay outside the bus's log: the fake board never finishes its
    // reboot, so the confirmation read is NACKed.
    let (bus, _) = bus_and_delay(vec![FakeNode::new(0x7C, &[0; 3], 3)]);
    let mut node = Node::new(bus, a(0x3E), Kind::Buttons);
    let mut unlogged = FakeDelay::default();
    assert!(matches!(
        service::set_address(&mut node, new(0x40), &mut unlogged),
        Err(Error::I2c(_))
    ));
    assert_eq!(node.address(), a(0x3E));
}

#[test]
fn general_call_returns_every_kind_to_its_default() {
    // One board of every kind and mode, each moved off its default.
    let mut nodes = Vec::new();
    let mut defaults = Vec::new();
    for (i, (_, board)) in boards().into_iter().enumerate() {
        defaults.push(board.addr);
        nodes.push(board.at(0x50 + i as u8));
    }
    let (mut bus, mut delay) = bus_and_delay(nodes);
    assert_eq!(service::general_call_reset(&mut bus, &mut delay), Ok(true));
    assert!(delay.total_ns >= 500 * MS);
    for (board, default) in bus.nodes.iter().zip(defaults) {
        assert_eq!(board.addr, default, "strap {:#04x}", board.strap);
        assert_eq!(
            (board.address_changes, board.long),
            (1, 0),
            "strap {:#04x}: its own length first, then rebooting",
            board.strap
        );
    }
}

#[test]
fn general_call_on_an_empty_bus_reports_no_answer() {
    let (mut bus, mut delay) = bus_and_delay(Vec::new());
    assert_eq!(service::general_call_reset(&mut bus, &mut delay), Ok(false));
}
