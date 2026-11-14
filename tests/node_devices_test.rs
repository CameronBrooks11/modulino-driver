//! Every node device from M2 against the simulated firmware.

mod support;

use modulino_driver::codec::latch_relay::SETTLE_MS;
use modulino_driver::{
    Address, Buzzer, Color, Error, Joystick, JoystickState, Kind, Knob, KnobState, LatchRelay,
    Note, OptoRelay, Pixels, PowerLevel, Vibro,
};
use support::{Event, FakeBus, FakeDelay, FakeNode, Log};

// Two HAL_Delay(50) calls take at most about 102 ms (main.c:315-323,
// stm32c0xx_hal.c:373-387).
const _: () = assert!(SETTLE_MS >= 102);

fn bus(strap: u8, payload: &[u8], write_len: usize) -> FakeBus {
    FakeBus::new(vec![FakeNode::new(strap, payload, write_len)])
}

#[test]
fn constructors_do_no_io() {
    let empty = FakeBus::default;
    assert_eq!(Buzzer::new(empty()).release().transactions, 0);
    assert_eq!(Vibro::new(empty()).release().transactions, 0);
    assert_eq!(Knob::new(empty()).release().transactions, 0);
    assert_eq!(Pixels::new(empty()).release().transactions, 0);
    assert_eq!(Joystick::new(empty()).release().transactions, 0);
    assert_eq!(OptoRelay::new(empty()).release().transactions, 0);
    assert_eq!(LatchRelay::new(empty()).release().transactions, 0);
}

#[test]
fn buzzer_plays_and_stops() {
    let mut buzzer = Buzzer::new(bus(0x3C, &[0; 3], 8));
    buzzer.verify().unwrap();
    buzzer.tone(440, 500).unwrap();
    buzzer.play_note(Note::A4, 250).unwrap();
    buzzer.play_note(Note::Rest, 0).unwrap();
    let bus = buzzer.release();
    assert_eq!(
        bus.node(0x1E).frames,
        vec![
            vec![0xB8, 0x01, 0, 0, 0xF4, 0x01, 0, 0],
            vec![0xB8, 0x01, 0, 0, 0xFA, 0x00, 0, 0],
            vec![0; 8],
        ]
    );
}

#[test]
fn buzzer_refuses_what_the_firmware_cannot_play() {
    let mut buzzer = Buzzer::new(bus(0x3C, &[0; 3], 8));
    assert_eq!(buzzer.tone(179, 100), Err(Error::OutOfRange));
    assert_eq!(buzzer.tone(u32::MAX, 100), Err(Error::OutOfRange));
    // 17987 Hz encodes as "CF" 0x00: sent raw, the firmware would rewrite
    // the address in flash (back to the strap default) and reset.
    assert_eq!(buzzer.tone(17987, 100), Err(Error::ReservedPrefix));
    let bus = buzzer.release();
    let board = bus.node(0x1E);
    assert!(board.frames.is_empty());
    assert_eq!(board.address_changes, 0);
}

#[test]
fn vibro_off_sends_the_full_frame() {
    let mut vibro = Vibro::new(bus(0x70, &[0; 3], 12));
    vibro.on(500, PowerLevel::Intense).unwrap();
    vibro.off().unwrap();
    let bus = vibro.release();
    let board = bus.node(0x38);
    assert_eq!(board.frames[0][8], PowerLevel::Intense as u8, "duty byte");
    // The motor stops only if the board processed the off frame.
    assert_eq!(board.frames.last(), Some(&vec![0u8; 12]));
    assert_eq!((board.frames.len(), board.dropped, board.long), (2, 0, 0));
}

#[test]
fn vibro_power_is_a_percentage() {
    let mut vibro = Vibro::new(bus(0x70, &[0; 3], 12));
    assert_eq!(vibro.on_with_power(100, 101), Err(Error::OutOfRange));
    assert_eq!(vibro.on_with_power(0, 50), Err(Error::OutOfRange));
    vibro.on_with_power(100, 100).unwrap();
}

#[test]
fn knob_offset_lives_on_the_host() {
    let mut board = FakeNode::new(0x74, &[0; 3], 4);
    board.queued.extend([vec![10, 0, 0], vec![15, 0, 1]]);
    let mut knob = Knob::new(FakeBus::new(vec![board]));
    assert_eq!(knob.address().get(), 0x3A);

    knob.set_position(0).unwrap();
    assert_eq!(
        knob.read().unwrap(),
        KnobState {
            position: 5,
            pressed: true
        }
    );
    let bus = knob.release();
    let board = bus.node(0x3A);
    assert_eq!(
        (board.frames.len(), board.dropped, board.long),
        (0, 0, 0),
        "no writes"
    );
}

/// A Knob whose reads give these counts, in order (button released).
fn knob_reading(counts: &[i16]) -> Knob<FakeBus> {
    let mut board = FakeNode::new(0x74, &[0; 3], 4);
    for c in counts {
        let [lo, hi] = c.to_le_bytes();
        board.queued.push_back(vec![lo, hi, 0]);
    }
    Knob::new(FakeBus::new(vec![board]))
}

#[test]
fn knob_position_wraps_at_the_i16_limits() {
    // Offset +1, then the board at i16::MAX: the position wraps to MIN.
    let mut knob = knob_reading(&[0, i16::MAX]);
    knob.set_position(1).unwrap();
    assert_eq!(knob.read().unwrap().position, i16::MIN);
}

#[test]
fn knob_set_position_wraps_the_offset() {
    // The board at i16::MIN asked to read i16::MAX: the offset is
    // MAX - MIN, which wraps to -1, and the position reads back as asked.
    let mut knob = knob_reading(&[i16::MIN, i16::MIN]);
    knob.set_position(i16::MAX).unwrap();
    assert_eq!(knob.read().unwrap().position, i16::MAX);
}

#[test]
fn knob_at_its_second_default_address() {
    let bus = FakeBus::new(vec![FakeNode::new(0x76, &[0xFE, 0xFF, 0], 4)]);
    let mut knob = Knob::with_address(bus, Address::new(0x3B).unwrap());
    assert_eq!(knob.read().unwrap().position, -2);
}

#[test]
fn pixels_show_sends_the_whole_frame() {
    let mut pixels = Pixels::new(bus(0x6C, &[0; 32], 32));
    pixels.set(0, Color::RED, 100).unwrap();
    pixels.set(7, Color::new(1, 2, 3), 50).unwrap();
    assert_eq!(pixels.set(8, Color::RED, 100), Err(Error::OutOfRange));
    assert_eq!(pixels.set(0, Color::RED, 101), Err(Error::OutOfRange));
    pixels.show().unwrap();
    pixels.clear(0).unwrap();
    pixels.show().unwrap();
    pixels.set_all(Color::BLUE, 100).unwrap();
    pixels.show().unwrap();
    pixels.clear_all();
    pixels.show().unwrap();

    let bus = pixels.release();
    let frames = &bus.node(0x36).frames;
    let off = [0xE0u8, 0, 0, 0];
    let mut lit = off.repeat(8);
    lit[..4].copy_from_slice(&[0xFF, 0, 0, 0xFF]);
    lit[28..].copy_from_slice(&[0xEF, 3, 2, 1]);
    assert_eq!(frames[0], lit);
    lit[..4].copy_from_slice(&off);
    assert_eq!(frames[1], lit, "clear(0)");
    assert_eq!(frames[2], [0xFFu8, 0xFF, 0, 0].repeat(8), "set_all");
    assert_eq!(frames[3], off.repeat(8), "clear_all");
}

#[test]
fn joystick_reads_centred_axes() {
    let mut joystick = Joystick::new(bus(0x58, &[0, 255, 1], 3));
    assert_eq!(
        joystick.read().unwrap(),
        JoystickState {
            x: 127,
            y: -127,
            pressed: true
        }
    );
    assert_eq!(joystick.read_raw().unwrap().x, 0);
}

#[test]
fn joystick_dead_zone_defaults_to_26_and_can_be_turned_off() {
    // 12 and 8 counts off centre: inside the default dead zone. (The sign
    // follows Arduino_Modulino's `128 - raw`, so raw 0 is +127.)
    let mut joystick = Joystick::new(bus(0x58, &[140, 120, 0], 3));
    assert_eq!(joystick.deadzone(), 26);
    let centred = JoystickState {
        x: 0,
        y: 0,
        pressed: false,
    };
    assert_eq!(joystick.read().unwrap(), centred);

    joystick.set_deadzone(0);
    assert_eq!(joystick.deadzone(), 0);
    assert_eq!(
        joystick.read().unwrap(),
        JoystickState {
            x: -12,
            y: 8,
            pressed: false
        }
    );
}

#[test]
fn opto_relay_switches_and_reads_the_pin() {
    let mut relay = OptoRelay::new(bus(0x28, &[1, 0, 0], 3));
    relay.on().unwrap();
    relay.off().unwrap();
    assert!(relay.is_on().unwrap());
    let bus = relay.release();
    assert_eq!(bus.node(0x14).frames, vec![vec![1, 0, 0], vec![0, 0, 0]]);
}

#[test]
fn latch_relay_answers_at_its_factory_address() {
    let mut board = FakeNode::new(0x04, &[0, 0, 0], 3);
    // What the firmware reports at power-up (read by verify), after a
    // set, then after a reset.
    board
        .queued
        .extend([vec![0, 0, 0], vec![0, 1, 0], vec![1, 0, 0]]);
    let mut relay = LatchRelay::new(FakeBus::new(vec![board]));
    assert_eq!(relay.address().get(), 0x02);
    assert_eq!(relay.address(), Kind::LatchRelay.default_address());
    relay.verify().unwrap();
    let mut delay = FakeDelay::default();
    relay.on(&mut delay).unwrap();
    assert_eq!(relay.state().unwrap(), Some(true));
    relay.off(&mut delay).unwrap();
    assert_eq!(relay.state().unwrap(), Some(false));
    let bus = relay.release();
    assert_eq!(bus.node(0x02).frames, vec![vec![1, 0, 0], vec![0, 0, 0]]);
}

#[test]
fn latch_relay_waits_out_the_coil_pulse() {
    let log = Log::default();
    let mut bus = bus(0x04, &[0, 0, 0], 3);
    bus.log = Some(log.clone());
    let mut relay = LatchRelay::new(bus);
    let mut delay = FakeDelay {
        log: Some(log.clone()),
        ..Default::default()
    };
    relay.on(&mut delay).unwrap();
    relay.off(&mut delay).unwrap();
    relay.state().unwrap();
    let wait = Event::Delay(SETTLE_MS as u64 * 1_000_000);
    assert_eq!(
        *log.borrow(),
        [
            Event::Bus(0x02),
            wait,
            Event::Bus(0x02),
            wait,
            Event::Bus(0x02)
        ],
        "each command is followed by the full wait before the next transfer"
    );
}

#[test]
fn latch_relay_waits_even_when_the_write_fails() {
    let log = Log::default();
    let bus = FakeBus {
        log: Some(log.clone()),
        ..Default::default()
    };
    let mut relay = LatchRelay::new(bus);
    let mut delay = FakeDelay {
        log: Some(log.clone()),
        ..Default::default()
    };
    assert!(relay.on(&mut delay).is_err());
    assert_eq!(
        *log.borrow(),
        [Event::Bus(0x02), Event::Delay(SETTLE_MS as u64 * 1_000_000)]
    );
}
