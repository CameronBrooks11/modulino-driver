//! Hub ports as buses: every device works behind one, unchanged.

mod support;

use core::cell::RefCell;

use embedded_hal::i2c::I2c;
use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction};
use modulino_driver::{Address, Buttons, Error, Hub, Knob, LatchRelay, LedMatrix, MatrixMode};
use support::{FakeBus, FakeDelay, FakeHub, FakeNode, Nack};

const HUB: u8 = 0x70;
const BUTTONS: u8 = 0x3E;

fn buttons_board(pressed: [u8; 3]) -> FakeNode {
    FakeNode::new(0x7C, &pressed, 3)
}

#[test]
fn port_selects_operates_and_deselects() {
    let expectations = [
        Transaction::write(HUB, vec![0x20]),
        Transaction::transaction_start(BUTTONS),
        Transaction::write(BUTTONS, vec![1, 0, 1]),
        Transaction::transaction_end(BUTTONS),
        Transaction::write(HUB, vec![0x00]),
    ];
    let bus = RefCell::new(I2cMock::new(&expectations));
    let hub = Hub::new(&bus);
    let mut buttons = Buttons::new(hub.port(5).unwrap());
    buttons.set_leds(true, false, true).unwrap();
    bus.into_inner().done();
}

#[test]
fn identical_boards_on_two_ports_stay_apart() {
    let hub = FakeHub::new(HUB)
        .with_nodes(0, vec![buttons_board([1, 0, 0])])
        .with_nodes(1, vec![buttons_board([0, 0, 1])]);
    let bus = RefCell::new(FakeBus::with_hubs(vec![hub]));
    let hub = Hub::new(&bus);
    let mut left = Buttons::new(hub.port(0).unwrap());
    let mut right = Buttons::new(hub.port(1).unwrap());

    assert!(left.read().unwrap().a);
    assert!(right.read().unwrap().c);
    right.set_leds(false, true, false).unwrap();

    let bus = bus.into_inner();
    let hub = bus.hub(HUB);
    assert!(hub.ports[0].nodes[0].frames.is_empty());
    assert_eq!(hub.ports[1].nodes[0].frames, vec![vec![0, 1, 0]]);
    assert_eq!(hub.control, 0, "deselected after every transaction");
}

#[test]
fn every_device_kind_works_behind_a_port() {
    let hub = FakeHub::new(HUB)
        .with_nodes(2, vec![FakeNode::new(0x74, &[5, 0, 1], 4)])
        .with_nodes(3, vec![FakeNode::led_matrix(false)])
        .with_nodes(4, vec![FakeNode::new(0x04, &[0, 1, 0], 3)]);
    let bus = RefCell::new(FakeBus::with_hubs(vec![hub]));
    let hub = Hub::new(&bus);

    let mut knob = Knob::new(hub.port(2).unwrap());
    assert_eq!(knob.read().unwrap().position, 5);

    let mut matrix = LedMatrix::new(hub.port(3).unwrap());
    matrix.set_mode(MatrixMode::Grayscale).unwrap();
    matrix.show_gray(&[[1; 12]; 8]).unwrap();

    let mut relay = LatchRelay::new(hub.port(4).unwrap());
    relay.on(&mut FakeDelay::default()).unwrap();
    assert_eq!(relay.state().unwrap(), Some(true));

    let bus = bus.into_inner();
    assert_eq!(bus.node(0x39).frames, vec![vec![0x11; 48]]);
    assert_eq!(bus.node(0x02).frames, vec![vec![1, 0, 0]]);
    assert_eq!(bus.hub(HUB).control, 0);
}

#[test]
fn port_deselects_after_a_failed_operation() {
    let bus = RefCell::new(FakeBus::with_hubs(vec![FakeHub::new(HUB)]));
    let hub = Hub::new(&bus);
    // Nothing on port 6 answers.
    let mut buttons = Buttons::new(hub.port(6).unwrap());
    assert_eq!(buttons.read(), Err(Error::I2c(Nack(BUTTONS))));

    let bus = bus.into_inner();
    let hub = bus.hub(HUB);
    assert_eq!(hub.control, 0, "control register after the failure");
    assert_eq!(hub.writes, vec![0x40, 0x00]);
}

#[test]
fn missing_hub_fails_before_the_device_is_addressed() {
    let bus = RefCell::new(FakeBus::new(vec![buttons_board([0; 3])]));
    let hub = Hub::new(&bus);
    let mut buttons = Buttons::new(hub.port(0).unwrap());
    assert_eq!(buttons.read(), Err(Error::I2c(Nack(HUB))));
    assert_eq!(bus.into_inner().transactions, 1, "only the select");
}

#[test]
fn nested_hub_reaches_the_inner_port() {
    let inner = FakeHub::new(0x71).with_nodes(5, vec![buttons_board([0, 1, 0])]);
    let outer = FakeHub::new(HUB).with_hub(2, inner);
    let bus = RefCell::new(FakeBus::with_hubs(vec![outer]));

    let outer = Hub::new(&bus);
    let outer_port = RefCell::new(outer.port(2).unwrap());
    let inner = Hub::with_address(&outer_port, Address::new(0x71).unwrap()).unwrap();
    let mut buttons = Buttons::new(inner.port(5).unwrap());

    assert!(buttons.read().unwrap().b);
    buttons.set_leds(true, true, true).unwrap();

    let bus = bus.into_inner();
    assert_eq!(bus.node(BUTTONS).frames, vec![vec![1, 1, 1]]);
    assert_eq!(bus.hub(HUB).control, 0);
    assert_eq!(bus.hub(0x71).control, 0);
}

#[test]
fn hub_addresses_and_ports_are_checked() {
    let bus = RefCell::new(FakeBus::default());
    assert!(Hub::with_address(&bus, Address::new(0x6F).unwrap()).is_none());
    assert!(Hub::with_address(&bus, Address::new(0x77).unwrap()).is_some());
    let hub = Hub::new(&bus);
    assert_eq!(hub.address().get(), HUB);
    assert_eq!(hub.port(7).unwrap().port(), 7);
    assert!(hub.port(8).is_none());
    assert_eq!(bus.borrow().transactions, 0, "no I/O");
}

#[test]
fn port_works_as_a_plain_bus() {
    let hub = FakeHub::new(0x73).with_nodes(1, vec![buttons_board([1, 1, 1])]);
    let bus = RefCell::new(FakeBus::with_hubs(vec![hub]));
    let hub = Hub::with_address(&bus, Address::new(0x73).unwrap()).unwrap();
    let mut port = hub.port(1).unwrap();
    let mut buf = [0u8; 4];
    port.read(BUTTONS, &mut buf).unwrap();
    assert_eq!(buf, [0x7C, 1, 1, 1]);
}
