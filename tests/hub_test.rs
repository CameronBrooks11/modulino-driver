//! Hub ports as buses: every device works behind one, unchanged.

mod support;

use core::cell::RefCell;

use embedded_hal::i2c::I2c;
use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction};
use modulino_driver::{Address, Buttons, Error, Hub, Knob, LatchRelay, LedMatrix, MatrixMode};
use support::{Event, FakeBus, FakeDelay, FakeHub, FakeNode, Log, Nack};

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
    let log = Log::default();
    let bus = RefCell::new(FakeBus {
        log: Some(log.clone()),
        ..FakeBus::new(vec![buttons_board([0; 3])])
    });
    let hub = Hub::new(&bus);
    let mut buttons = Buttons::new(hub.port(0).unwrap());
    assert_eq!(buttons.read(), Err(Error::I2c(Nack(HUB))));
    assert_eq!(
        *log.lock().unwrap(),
        [Event::Bus(HUB), Event::Bus(HUB)],
        "select, best-effort deselect; the board is never addressed"
    );
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

/// A bus that fails the next write of `[0]` to `hub`, as a hub that drops
/// off the bus mid-transaction would.
struct FailDeselect {
    bus: FakeBus,
    hub: u8,
    armed: bool,
}

impl embedded_hal::i2c::ErrorType for FailDeselect {
    type Error = Nack;
}

impl I2c for FailDeselect {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Nack> {
        let deselect = matches!(operations, [embedded_hal::i2c::Operation::Write(&[0])]);
        if self.armed && address == self.hub && deselect {
            self.armed = false;
            return Err(Nack(0xEE));
        }
        self.bus.transaction(address, operations)
    }
}

fn failing_deselect(port_nodes: Vec<FakeNode>) -> RefCell<FailDeselect> {
    let hub = FakeHub::new(HUB).with_nodes(1, port_nodes);
    RefCell::new(FailDeselect {
        bus: FakeBus::with_hubs(vec![hub]),
        hub: HUB,
        armed: true,
    })
}

#[test]
fn operation_error_comes_before_deselect_error() {
    // Nothing on port 1, and the deselect fails too.
    let bus = failing_deselect(vec![]);
    let hub = Hub::new(&bus);
    let mut buttons = Buttons::new(hub.port(1).unwrap());
    assert_eq!(buttons.read(), Err(Error::I2c(Nack(BUTTONS))));
}

#[test]
fn failed_deselect_is_reported_and_recoverable() {
    let bus = failing_deselect(vec![buttons_board([0; 3])]);
    let hub = Hub::new(&bus);
    let mut buttons = Buttons::new(hub.port(1).unwrap());
    // The write took effect, but the deselect error is reported.
    assert_eq!(
        buttons.set_leds(true, false, false),
        Err(Error::I2c(Nack(0xEE)))
    );
    assert_eq!(bus.borrow().bus.node(BUTTONS).frames, vec![vec![1, 0, 0]]);
    assert_eq!(
        bus.borrow().bus.hub(HUB).control,
        0x02,
        "port left connected"
    );

    hub.deselect().unwrap();
    assert_eq!(bus.borrow().bus.hub(HUB).control, 0);
}

#[test]
fn hub_address_behind_its_own_port_is_a_collision() {
    // A board readdressed to 0x70 behind port 0 shares the joined segment
    // with the hub itself, so both answer a read.
    let hub = FakeHub::new(HUB).with_nodes(0, vec![FakeNode::new(0x7C, &[0; 3], 3).at(HUB)]);
    let bus = RefCell::new(FakeBus::with_hubs(vec![hub]));
    let mut port = Hub::new(&bus).port(0).unwrap();
    let mut byte = [0u8; 1];
    port.read(HUB, &mut byte).unwrap();
    assert_eq!(byte, [0x01 & 0x7C], "wired-AND of the hub and the board");
}
