//! The public simulated bus: every driver runs on its defaults and reads
//! plausible values, outputs read back, and a missing board is a NACK.

use core::cell::RefCell;

use embedded_hal::i2c::ErrorKind;
use embedded_hal_mock::eh1::delay::NoopDelay;
use modulino_driver::sim::{SimBus, SimHub};
use modulino_driver::{
    Address, Buttons, Buzzer, ChipKind, Distance, Error, Hub, Joystick, Kind, Knob, LatchRelay,
    LedMatrix, Light, MatrixMode, Motors, Movement, OptoRelay, Pixels, Pressure, Thermo, Vibro,
};

fn a(raw: u8) -> Address {
    Address::new(raw).unwrap()
}

fn is_nack<E: embedded_hal::i2c::Error>(e: &Error<E>) -> bool {
    matches!(e, Error::I2c(e) if matches!(e.kind(), ErrorKind::NoAcknowledge(_)))
}

#[test]
fn every_node_kind_verifies_at_its_default_address() {
    let kinds = [
        Kind::Buttons,
        Kind::Buzzer,
        Kind::Vibro,
        Kind::Knob,
        Kind::Pixels,
        Kind::Joystick,
        Kind::OptoRelay,
        Kind::LatchRelay,
        Kind::LedMatrix,
        Kind::Motors,
    ];
    for kind in kinds {
        let bus = SimBus::new().with_node(kind);
        let result = match kind {
            Kind::Buttons => Buttons::new(bus).verify(),
            Kind::Buzzer => Buzzer::new(bus).verify(),
            Kind::Vibro => Vibro::new(bus).verify(),
            Kind::Knob => Knob::new(bus).verify(),
            Kind::Pixels => Pixels::new(bus).verify(),
            Kind::Joystick => Joystick::new(bus).verify(),
            Kind::OptoRelay => OptoRelay::new(bus).verify(),
            Kind::LatchRelay => LatchRelay::new(bus).verify(),
            Kind::LedMatrix => LedMatrix::new(bus).verify(),
            Kind::Motors => Motors::new(bus).verify(),
            _ => unreachable!(),
        };
        assert_eq!(result, Ok(()), "{kind:?}");
    }
}

#[test]
fn inputs_start_idle_and_follow_set_reading() {
    let mut buttons = Buttons::new(SimBus::new().with_node(Kind::Buttons));
    let s = buttons.read().unwrap();
    assert!(!s.a && !s.b && !s.c);
    let mut bus = buttons.release();
    assert!(bus.set_reading(Kind::Buttons.default_address(), &[0, 1, 0]));
    let mut buttons = Buttons::new(bus);
    assert!(buttons.read().unwrap().b);

    let mut joystick = Joystick::new(SimBus::new().with_node(Kind::Joystick));
    let s = joystick.read().unwrap();
    assert_eq!((s.x, s.y, s.pressed), (0, 0, false), "centred");

    let mut knob = Knob::new(SimBus::new().with_node(Kind::Knob));
    let s = knob.read().unwrap();
    assert_eq!((s.position, s.pressed), (0, false));
}

#[test]
fn outputs_read_back() {
    let addr = Kind::Buttons.default_address();
    let mut buttons = Buttons::new(SimBus::new().with_node(Kind::Buttons));
    buttons.set_leds(true, false, true).unwrap();
    buttons.set_leds(false, true, false).unwrap();
    assert_eq!(
        buttons.release().last_write(addr),
        Some(&[0, 1, 0][..]),
        "the latest"
    );

    let mut relay = OptoRelay::new(SimBus::new().with_node(Kind::OptoRelay));
    assert!(!relay.is_on().unwrap());
    relay.on().unwrap();
    assert!(relay.is_on().unwrap());
    relay.off().unwrap();
    assert!(!relay.is_on().unwrap());

    let mut latch = LatchRelay::new(SimBus::new().with_node(Kind::LatchRelay));
    let mut delay = NoopDelay::new();
    assert_eq!(latch.state().unwrap(), None, "unknown until commanded");
    latch.on(&mut delay).unwrap();
    assert_eq!(latch.state().unwrap(), Some(true));
    latch.off(&mut delay).unwrap();
    assert_eq!(latch.state().unwrap(), Some(false));

    let mut matrix = LedMatrix::new(SimBus::new().with_node(Kind::LedMatrix));
    matrix.verify().unwrap();
    matrix.set_mode(MatrixMode::Grayscale).unwrap();
    // A fresh driver reads the mode from the board.
    let mut again = LedMatrix::new(matrix.release());
    again.verify().unwrap();
    assert_eq!(again.mode().unwrap(), MatrixMode::Grayscale);

    let addr = Kind::Pixels.default_address();
    let mut pixels = Pixels::new(SimBus::new().with_node(Kind::Pixels));
    pixels.show().unwrap();
    assert_eq!(pixels.release().last_write(addr).map(<[u8]>::len), Some(32));
}

#[test]
fn thermo_reads_room_conditions_on_either_chip() {
    for chip in [ChipKind::Hs3003, ChipKind::Sht31] {
        let mut thermo = Thermo::new(SimBus::new().with_chip(chip));
        let r = thermo.read(&mut NoopDelay::new()).unwrap();
        assert_eq!(thermo.chip(), Some(chip));
        assert!((r.temperature - 25.0).abs() < 0.1, "{chip:?} {r:?}");
        assert!((r.humidity - 50.0).abs() < 0.1, "{chip:?} {r:?}");
    }
}

#[test]
fn movement_lies_flat() {
    let mut m = Movement::new(SimBus::new().with_chip(ChipKind::Lsm6dsox));
    m.init().unwrap();
    assert!(m.acceleration_ready().unwrap());
    let g = m.acceleration().unwrap();
    assert!(
        g.x == 0.0 && g.y == 0.0 && (g.z - 1.0).abs() < 0.01,
        "{g:?}"
    );
    let w = m.angular_velocity().unwrap();
    assert_eq!((w.x, w.y, w.z), (0.0, 0.0, 0.0));
}

#[test]
fn pressure_reads_sea_level() {
    let mut p = Pressure::new(SimBus::new().with_chip(ChipKind::Lps22hb));
    p.init().unwrap();
    assert!((p.pressure().unwrap() - 1013.25).abs() < 0.01);
    assert!((p.temperature().unwrap() - 25.0).abs() < 0.01);
}

#[test]
fn light_reads_a_lit_room() {
    let mut l = Light::new(SimBus::new().with_chip(ChipKind::Ltr381rgb));
    l.init().unwrap();
    assert!(l.ready().unwrap());
    let r = l.read().unwrap();
    assert!((r.lux - 174.84).abs() < 0.1, "{r:?}");
}

#[test]
fn distance_ranges_a_target_on_either_variant() {
    for chip in [ChipKind::Vl53l4cd, ChipKind::Vl53l4ed] {
        let mut d = Distance::new(SimBus::new().with_chip(chip));
        let mut delay = NoopDelay::new();
        assert_eq!(
            d.verify().unwrap().kind(),
            chip,
            "the variant answers as itself"
        );
        d.init(&mut delay).unwrap();
        d.start_ranging(&mut delay).unwrap();
        assert!(d.data_ready().unwrap(), "{chip:?}");
        let r = d.read().unwrap();
        assert_eq!((r.distance_mm, r.status), (200, 0), "{chip:?}");
        d.stop_ranging().unwrap();
    }
}

#[test]
fn an_absent_board_is_a_nack_and_comes_back_with_its_state() {
    let err = Buttons::new(SimBus::new()).verify().unwrap_err();
    assert!(is_nack(&err), "{err:?}");

    let addr = Kind::OptoRelay.default_address();
    let mut relay = OptoRelay::new(SimBus::new().with_node(Kind::OptoRelay));
    relay.on().unwrap();
    let mut bus = relay.release();
    assert!(bus.set_present(addr, false));
    let mut relay = OptoRelay::new(bus);
    let err = relay.is_on().unwrap_err();
    assert!(is_nack(&err), "{err:?}");
    let mut bus = relay.release();
    assert!(bus.set_present(addr, true));
    let mut relay = OptoRelay::new(bus);
    assert!(relay.is_on().unwrap(), "kept its state while off the bus");

    let mut bus = SimBus::new();
    assert!(!bus.set_present(a(0x40), false), "nothing there");
}

#[test]
fn boards_behind_a_hub() {
    let hub = SimHub::new(a(0x70))
        .with_port(0, SimBus::new().with_node(Kind::Buttons))
        .with_port(1, SimBus::new().with_node(Kind::Buttons));
    let bus = RefCell::new(SimBus::new().with_hub(hub));
    let hub = Hub::new(&bus);
    let mut left = Buttons::new(hub.port(0).unwrap());
    let mut right = Buttons::new(hub.port(1).unwrap());
    left.verify().unwrap();
    right.verify().unwrap();
    left.set_leds(true, false, false).unwrap();
    // Only one of the two answers each port.
    assert!(right.read().is_ok());
    hub.deselect().unwrap();

    // With the hub off the bus, nothing behind it answers either.
    bus.borrow_mut().set_present(a(0x70), false);
    let err = left.read().unwrap_err();
    assert!(is_nack(&err), "{err:?}");
}

#[test]
fn a_node_at_a_changed_address() {
    let mut buttons =
        Buttons::with_address(SimBus::new().with_node_at(Kind::Buttons, a(0x40)), a(0x40));
    buttons.verify().unwrap();
}

#[test]
fn sim_bus_is_send() {
    fn is_send<T: Send + 'static>() {}
    is_send::<SimBus>();
}
