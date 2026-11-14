mod support;

use modulino_driver::{Address, ButtonState, Buttons, Error, Kind};
use support::{FakeBus, FakeNode, Nack};

const BUTTONS_STRAP: u8 = 0x7C;
const PIXELS_STRAP: u8 = 0x6C;

fn buttons_board(pressed: [u8; 3]) -> FakeNode {
    FakeNode::new(BUTTONS_STRAP, &pressed, 3)
}

#[test]
fn reads_buttons_and_sets_leds() {
    let bus = FakeBus::new(vec![buttons_board([0, 1, 0])]);
    let mut buttons = Buttons::new(bus);

    buttons.verify().unwrap();
    let state = buttons.read().unwrap();
    assert_eq!(
        state,
        ButtonState {
            a: false,
            b: true,
            c: false
        }
    );
    buttons.set_leds(true, false, true).unwrap();

    let bus = buttons.release();
    let board = bus.node(0x3E);
    assert_eq!(board.frames, vec![vec![1, 0, 1]]);
    assert_eq!(board.dropped, 0);
}

#[test]
fn constructors_do_no_io() {
    let bus = FakeBus::new(vec![buttons_board([0, 0, 0])]);
    let buttons = Buttons::new(bus);
    assert_eq!(buttons.address(), Kind::Buttons.default_address());
    assert_eq!(buttons.release().transactions, 0);

    let bus = FakeBus::new(vec![buttons_board([0, 0, 0]).at(0x40)]);
    let buttons = Buttons::with_address(bus, Address::new(0x40).unwrap());
    assert_eq!(buttons.release().transactions, 0);
}

#[test]
fn missing_board_is_an_i2c_error() {
    let mut buttons = Buttons::new(FakeBus::default());
    assert_eq!(buttons.verify(), Err(Error::I2c(Nack(0x3E))));
    assert_eq!(buttons.read(), Err(Error::I2c(Nack(0x3E))));
}

#[test]
fn custom_address() {
    let bus = FakeBus::new(vec![buttons_board([1, 1, 1]).at(0x40)]);
    let mut buttons = Buttons::with_address(bus, Address::new(0x40).unwrap());
    assert!(buttons.read().unwrap().all_pressed());
}

#[test]
fn read_from_another_kind_is_wrong_identity() {
    // A Pixels board moved to the Buttons default address.
    let bus = FakeBus::new(vec![FakeNode::new(PIXELS_STRAP, &[0; 32], 32).at(0x3E)]);
    let mut buttons = Buttons::new(bus);

    let expected = Err(Error::WrongIdentity {
        expected: Kind::Buttons,
        got: PIXELS_STRAP,
    });
    assert_eq!(buttons.read(), expected);
    assert_eq!(buttons.verify(), expected.map(|_| ()));
}
