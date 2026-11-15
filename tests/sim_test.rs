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
    assert!(m.angular_velocity_ready().unwrap());
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
    assert_eq!((r.ir, r.green, r.red, r.blue), (500, 1000, 1000, 1000));
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
        // Autonomous timing divides by the oscillator calibration.
        d.set_range_timing(50, 100).unwrap();
    }
    // RANGE_CONFIG_A from an OSC_FREQUENCY of 3213, as ST's driver computes
    // it for the CD (tests/chips_test.rs, init_follows_st_sensor_init).
    let mut d = Distance::new(SimBus::new().with_chip(ChipKind::Vl53l4cd));
    d.init(&mut NoopDelay::new()).unwrap();
    let bus = d.release();
    let regs = &bus.model().chips[0].regs;
    assert_eq!((regs[&0x005E], regs[&0x005F]), (0x00, 0x40));

    // INTERMEASUREMENT from an oscillator calibration of 300: 1.055 * 18 *
    // 300 is 5697 (tests/chips_test.rs, autonomous_timing_writes_the_period).
    let mut d = Distance::new(SimBus::new().with_chip(ChipKind::Vl53l4cd));
    d.init(&mut NoopDelay::new()).unwrap();
    d.set_range_timing(10, 18).unwrap();
    let bus = d.release();
    let regs = &bus.model().chips[0].regs;
    let period = u32::from_be_bytes([regs[&0x006C], regs[&0x006D], regs[&0x006E], regs[&0x006F]]);
    assert_eq!(period, 5697);
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
fn boards_behind_a_hub_are_told_apart_by_port() {
    let hub_addr = a(0x70);
    let buttons = Kind::Buttons.default_address();
    let hub = SimHub::new(hub_addr)
        .unwrap()
        .with_port(0, SimBus::new().with_node(Kind::Buttons))
        .with_port(1, SimBus::new().with_node(Kind::Buttons));
    let bus = RefCell::new(SimBus::new().with_hub(hub));
    let hub = Hub::new(&bus);
    let mut left = Buttons::new(hub.port(0).unwrap());
    let mut right = Buttons::new(hub.port(1).unwrap());
    left.verify().unwrap();
    right.verify().unwrap();

    // A write through port 1 reaches only port 1's board.
    right.set_leds(false, false, true).unwrap();
    hub.deselect().unwrap();
    {
        let sim = bus.borrow();
        assert_eq!(
            sim.last_write_at(hub_addr, 1, buttons),
            Some(&[0, 0, 1][..])
        );
        assert_eq!(
            sim.last_write_at(hub_addr, 0, buttons),
            None,
            "port 0 untouched"
        );
    }

    // Inputs and presence per port.
    assert!(bus
        .borrow_mut()
        .set_reading_at(hub_addr, 0, buttons, &[1, 0, 0]));
    assert!(bus
        .borrow_mut()
        .set_reading_at(hub_addr, 1, buttons, &[0, 1, 0]));
    let (l, r) = (left.read().unwrap(), right.read().unwrap());
    assert!(l.a && !l.b, "{l:?}");
    assert!(r.b && !r.a, "{r:?}");
    assert!(bus.borrow_mut().set_present_at(hub_addr, 0, buttons, false));
    assert!(is_nack(&left.read().unwrap_err()));
    assert!(right.read().is_ok(), "only port 0's board left");
    assert!(bus.borrow_mut().set_present_at(hub_addr, 0, buttons, true));
    assert!(left.read().is_ok());

    // With the hub off the bus, nothing behind it answers either.
    bus.borrow_mut().set_present(hub_addr, false);
    let err = left.read().unwrap_err();
    assert!(is_nack(&err), "{err:?}");
}

#[test]
fn address_wide_methods_reach_behind_hub_ports() {
    let hub_addr = a(0x70);
    let buttons = Kind::Buttons.default_address();
    let hub = SimHub::new(hub_addr)
        .unwrap()
        .with_port(2, SimBus::new().with_node(Kind::Buttons));
    let bus = RefCell::new(SimBus::new().with_hub(hub));
    let hub = Hub::new(&bus);
    let mut b = Buttons::new(hub.port(2).unwrap());
    assert!(bus.borrow_mut().set_reading(buttons, &[0, 0, 1]));
    assert!(b.read().unwrap().c);
    b.set_leds(true, true, true).unwrap();
    assert_eq!(bus.borrow().last_write(buttons), Some(&[1, 1, 1][..]));
    assert!(bus.borrow_mut().set_present(buttons, false));
    assert!(is_nack(&b.read().unwrap_err()));
}

#[test]
fn chips_behind_a_hub_port() {
    let hub = SimHub::new(a(0x70))
        .unwrap()
        .with_port(3, SimBus::new().with_chip(ChipKind::Lps22hb));
    let bus = RefCell::new(SimBus::new().with_hub(hub));
    let hub = Hub::new(&bus);
    let mut p = Pressure::new(hub.port(3).unwrap());
    p.init().unwrap();
    assert!((p.pressure().unwrap() - 1013.25).abs() < 0.01);
}

#[test]
fn a_hub_address_outside_0x70_0x77_is_refused() {
    assert!(SimHub::new(a(0x40)).is_none());
    assert!(SimHub::new(a(0x77)).is_some());
}

#[test]
fn an_absent_chip_is_a_nack() {
    let addr = a(0x5C);
    let mut bus = SimBus::new().with_chip(ChipKind::Lps22hb);
    assert!(bus.set_present(addr, false));
    let mut p = Pressure::new(bus);
    let err = p.init().unwrap_err();
    assert!(is_nack(&err), "{err:?}");
    let mut bus = p.release();
    assert!(bus.set_present(addr, true));
    let mut p = Pressure::new(bus);
    p.init().unwrap();
}

#[test]
fn a_chip_at_another_address() {
    let mut p = Pressure::with_address(
        SimBus::new().with_chip_at(ChipKind::Lps22hb, a(0x5D)),
        a(0x5D),
    );
    p.init().unwrap();
    let err = Pressure::new(p.release()).init().unwrap_err();
    assert!(is_nack(&err), "nothing at the default 0x5C: {err:?}");
}

#[test]
fn the_sht31_nacks_idle_reads() {
    use embedded_hal::i2c::I2c;
    let mut bus = SimBus::new().with_chip(ChipKind::Sht31);
    let mut buf = [0u8; 3];
    let err = bus.read(0x45, &mut buf).unwrap_err();
    assert_eq!(err.address(), 0x45);
    assert!(matches!(
        embedded_hal::i2c::Error::kind(&err),
        ErrorKind::NoAcknowledge(_)
    ));
}

#[test]
fn the_matrix_starts_in_mono() {
    let mut m = LedMatrix::new(SimBus::new().with_node(Kind::LedMatrix));
    m.verify().unwrap();
    assert_eq!(m.mode().unwrap(), MatrixMode::Mono);
}

#[test]
fn a_short_reading_does_not_break_a_relay() {
    let opto = Kind::OptoRelay.default_address();
    let mut bus = SimBus::new().with_node(Kind::OptoRelay);
    assert!(bus.set_reading(opto, &[]));
    let mut relay = OptoRelay::new(bus);
    relay.on().unwrap();
    assert!(relay.is_on().unwrap());

    let latch = Kind::LatchRelay.default_address();
    let mut bus = SimBus::new().with_node(Kind::LatchRelay);
    assert!(bus.set_reading(latch, &[0]));
    let mut relay = LatchRelay::new(bus);
    relay.on(&mut NoopDelay::new()).unwrap();
    assert_eq!(relay.state().unwrap(), Some(true));
}

#[test]
fn memory_stays_flat_over_a_long_run() {
    let addr = Kind::Buttons.default_address();
    let mut buttons = Buttons::new(SimBus::new().with_node(Kind::Buttons));
    for i in 0..1000 {
        buttons.set_leds(i % 2 == 0, false, false).unwrap();
    }
    let bus = buttons.release();
    assert_eq!(bus.model().nodes[0].frames.len(), 1, "only the last frame");
    assert_eq!(bus.last_write(addr), Some(&[0, 0, 0][..]));

    let mut p = Pressure::new(SimBus::new().with_chip(ChipKind::Lps22hb));
    p.init().unwrap();
    for _ in 0..1000 {
        p.pressure().unwrap();
    }
    assert!(p.release().model().chips[0].writes.is_empty());

    let hub = SimHub::new(a(0x70)).unwrap();
    let bus = RefCell::new(SimBus::new().with_hub(hub));
    let h = Hub::new(&bus);
    for _ in 0..1000 {
        h.deselect().unwrap();
    }
    assert!(bus.borrow().model().hubs[0].writes.is_empty());
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

#[test]
fn nested_hubs_are_reached_by_their_own_address() {
    let (outer, inner) = (a(0x70), a(0x71));
    let buttons = Kind::Buttons.default_address();
    let pressure = a(0x5C);
    let inner_hub = SimHub::new(inner)
        .unwrap()
        .with_port(0, SimBus::new().with_node(Kind::Buttons))
        .with_port(1, SimBus::new().with_chip(ChipKind::Lps22hb));
    let hub = SimHub::new(outer)
        .unwrap()
        .with_port(2, SimBus::new().with_hub(inner_hub));
    let mut sim = SimBus::new().with_hub(hub);

    // The board behind the inner hub, by the inner hub's address.
    assert!(sim.set_reading_at(inner, 0, buttons, &[1, 1, 0]));
    assert!(
        !sim.set_reading_at(outer, 2, buttons, &[0, 0, 0]),
        "not directly on 0x70's port 2"
    );

    // Presence per port, for a chip and for the inner hub itself.
    assert!(sim.set_present_at(inner, 1, pressure, false));
    assert!(sim.model().hubs[0].ports[2].hubs[0].ports[1].chips[0].absent);
    assert!(sim.set_present_at(inner, 1, pressure, true));
    assert!(
        !sim.set_present_at(outer, 3, inner, false),
        "nothing at 0x71 on port 3"
    );
    assert!(sim.set_present_at(outer, 2, inner, false));
    assert!(sim.model().hubs[0].ports[2].hubs[0].absent);

    let bus = RefCell::new(sim);
    let outer_hub = Hub::new(&bus);
    let outer_port = RefCell::new(outer_hub.port(2).unwrap());
    let inner_hub = Hub::with_address(&outer_port, inner).unwrap();
    let mut b = Buttons::new(inner_hub.port(0).unwrap());
    assert!(
        is_nack(&b.read().unwrap_err()),
        "the inner hub is off the bus"
    );
    assert!(bus.borrow_mut().set_present_at(outer, 2, inner, true));
    let outer_port = RefCell::new(outer_hub.port(2).unwrap());
    let inner_hub = Hub::with_address(&outer_port, inner).unwrap();
    let mut b = Buttons::new(inner_hub.port(0).unwrap());
    let s = b.read().unwrap();
    assert!(s.a && s.b && !s.c, "{s:?}");
    b.set_leds(false, true, false).unwrap();
    assert_eq!(
        bus.borrow().last_write_at(inner, 0, buttons),
        Some(&[0, 1, 0][..])
    );
}

#[test]
fn an_address_change_keeps_no_history() {
    use modulino_driver::{service, NewAddress, Node};
    let mut node = Node::new(
        SimBus::new().with_node(Kind::Buttons),
        Kind::Buttons.default_address(),
        Kind::Buttons,
    );
    service::set_address(
        &mut node,
        NewAddress::new(0x40).unwrap(),
        &mut NoopDelay::new(),
    )
    .unwrap();
    assert_eq!(node.address(), a(0x40));
    assert!(node.release().model().nodes[0].cf_lengths.is_empty());
}

#[test]
fn movement_pressure_and_light_ready_flags_can_be_held_clear() {
    let mut bus = SimBus::new().with_chip(ChipKind::Lsm6dsox);
    assert!(bus.set_ready(a(0x6A), false));
    let mut m = Movement::new(bus);
    m.init().unwrap();
    assert!(!m.acceleration_ready().unwrap());
    assert!(!m.angular_velocity_ready().unwrap());
    let mut bus = m.release();
    assert!(bus.set_ready(a(0x6A), true));
    let mut m = Movement::new(bus);
    m.init().unwrap();
    assert!(m.acceleration_ready().unwrap());
    assert!(m.angular_velocity_ready().unwrap());

    let mut bus = SimBus::new().with_chip(ChipKind::Lps22hb);
    assert!(bus.set_ready(a(0x5C), false));
    let mut p = Pressure::new(bus);
    p.init().unwrap();
    assert!(!p.pressure_ready().unwrap());
    assert!(!p.temperature_ready().unwrap());
    let mut bus = p.release();
    assert!(bus.set_ready(a(0x5C), true));
    let mut p = Pressure::new(bus);
    assert!(p.pressure_ready().unwrap());
    assert!(p.temperature_ready().unwrap());

    let mut bus = SimBus::new().with_chip(ChipKind::Ltr381rgb);
    assert!(bus.set_ready(a(0x53), false));
    let mut l = Light::new(bus);
    l.init().unwrap();
    assert!(!l.ready().unwrap());
    let mut bus = l.release();
    assert!(bus.set_ready(a(0x53), true));
    let mut l = Light::new(bus);
    assert!(l.ready().unwrap());
}

#[test]
fn a_held_distance_flag_times_out_its_waits() {
    for chip in [ChipKind::Vl53l4cd, ChipKind::Vl53l4ed] {
        let mut delay = NoopDelay::new();
        let mut bus = SimBus::new().with_chip(chip);
        assert!(bus.set_ready(a(0x29), false));
        let mut d = Distance::new(bus);
        // Before init the GPIO is active high (the sim's power-up value);
        // init sets it active low. Held reads "no data" under either.
        assert!(!d.data_ready().unwrap(), "{chip:?} active high");
        assert!(
            matches!(d.init(&mut delay), Err(Error::Timeout)),
            "{chip:?}"
        );

        let mut bus = d.release();
        assert!(bus.set_ready(a(0x29), true));
        let mut d = Distance::new(bus);
        d.init(&mut delay).unwrap();
        let mut bus = d.release();
        bus.set_ready(a(0x29), false);
        let mut d = Distance::new(bus);
        assert!(!d.data_ready().unwrap(), "{chip:?}");
        // Only the CD waits for, and discards, a first sample on start.
        let started = d.start_ranging(&mut delay);
        if chip == ChipKind::Vl53l4cd {
            assert!(matches!(started, Err(Error::Timeout)), "{started:?}");
        } else {
            assert!(started.is_ok(), "{started:?}");
        }

        let mut bus = d.release();
        bus.set_ready(a(0x29), true);
        let mut d = Distance::new(bus);
        assert!(d.data_ready().unwrap(), "{chip:?}");
    }
}

#[test]
fn set_ready_needs_a_chip_with_a_ready_flag() {
    let mut bus = SimBus::new()
        .with_node(Kind::Buttons)
        .with_chip(ChipKind::Hs3003);
    assert!(!bus.set_ready(a(0x3E), false), "a node");
    assert!(!bus.set_ready(a(0x44), false), "the HS3003 has no flag");
    assert!(!bus.set_ready(a(0x53), false), "nothing there");
}

#[test]
fn ready_flags_are_held_per_hub_port() {
    let hub_addr = a(0x70);
    let hub = SimHub::new(hub_addr)
        .unwrap()
        .with_port(1, SimBus::new().with_chip(ChipKind::Ltr381rgb))
        .with_port(2, SimBus::new().with_chip(ChipKind::Ltr381rgb));
    let mut bus = SimBus::new().with_hub(hub);
    assert!(bus.set_ready_at(hub_addr, 1, a(0x53), false));
    assert!(!bus.set_ready_at(hub_addr, 3, a(0x53), false), "empty port");
    let bus = RefCell::new(bus);
    let hub = Hub::new(&bus);
    assert!(!Light::new(hub.port(1).unwrap()).ready().unwrap());
    assert!(Light::new(hub.port(2).unwrap()).ready().unwrap());
    // The address-wide method reaches the first one behind a port.
    assert!(bus.borrow_mut().set_ready(a(0x53), true));
    assert!(Light::new(hub.port(1).unwrap()).ready().unwrap());
}
