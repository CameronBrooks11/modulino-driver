//! Small public helpers: button predicates, colour conversions, hub port
//! address, frame length errors, note frequencies, error display.

mod support;

use core::cell::RefCell;

use modulino_driver::{Address, ButtonState, ChipKind, Color, Error, Hub, Kind, Node, Note};
use support::FakeBus;

#[test]
fn button_predicates_on_mixed_states() {
    let state = |a, b, c| ButtonState { a, b, c };
    assert!(!state(false, false, false).any_pressed());
    assert!(state(false, true, false).any_pressed());
    assert!(!state(true, true, false).all_pressed());
    assert!(!state(false, false, false).all_pressed());
    assert!(state(true, true, true).all_pressed());
}

#[test]
fn colour_from_a_24_bit_value() {
    let c = Color::from_rgb24(0x123456);
    assert_eq!((c.r, c.g, c.b), (0x12, 0x34, 0x56));
    assert_eq!(Color::from(0x123456u32), c);
    // Bits above 24 are ignored.
    assert_eq!(Color::from_rgb24(0xFF12_3456), c);
}

#[test]
fn a_port_knows_its_hub_address() {
    let bus = RefCell::new(FakeBus::default());
    let hub = Hub::with_address(&bus, Address::new(0x73).unwrap()).unwrap();
    assert_eq!(
        hub.port(5).unwrap().hub_address(),
        Address::new(0x73).unwrap()
    );
}

#[test]
fn a_mode_kind_reports_its_mono_length_for_a_bad_frame() {
    let mut node = Node::new(
        FakeBus::default(),
        Address::new(0x39).unwrap(),
        Kind::LedMatrix,
    );
    assert_eq!(
        node.write(&[0u8; 13]),
        Err(Error::FrameLength {
            expected: 12,
            got: 13
        })
    );
    assert_eq!(node.release().transactions, 0, "nothing sent");
}

#[test]
fn notes_are_equal_tempered_semitones_from_fs3() {
    // FS3 is 15 semitones below A4 = 440 Hz; each note is the next
    // semitone, rounded to the nearest hertz.
    let notes = [
        Note::FS3,
        Note::G3,
        Note::GS3,
        Note::A3,
        Note::AS3,
        Note::B3,
        Note::C4,
        Note::CS4,
        Note::D4,
        Note::DS4,
        Note::E4,
        Note::F4,
        Note::FS4,
        Note::G4,
        Note::GS4,
        Note::A4,
        Note::AS4,
        Note::B4,
        Note::C5,
        Note::CS5,
        Note::D5,
        Note::DS5,
        Note::E5,
        Note::F5,
        Note::FS5,
        Note::G5,
        Note::GS5,
        Note::A5,
        Note::AS5,
        Note::B5,
        Note::C6,
        Note::CS6,
        Note::D6,
        Note::DS6,
        Note::E6,
        Note::F6,
        Note::FS6,
        Note::G6,
        Note::GS6,
        Note::A6,
        Note::AS6,
        Note::B6,
        Note::C7,
        Note::CS7,
        Note::D7,
        Note::DS7,
        Note::E7,
        Note::F7,
        Note::FS7,
        Note::G7,
        Note::GS7,
        Note::A7,
        Note::AS7,
        Note::B7,
        Note::C8,
        Note::CS8,
        Note::D8,
        Note::DS8,
    ];
    for (i, note) in notes.iter().enumerate() {
        let semitones = i as f64 - 15.0;
        let hz = 440.0 * 2f64.powf(semitones / 12.0);
        let got = *note as u16 as f64;
        assert!(
            (got - hz).abs() <= 0.5,
            "{note:?} = {got}, expected {hz:.2}"
        );
    }
    assert_eq!(Note::Rest as u16, 0);
}

#[test]
fn error_display_shows_the_values() {
    let shown = |e: Error<u8>| format!("{e}");
    let s = shown(Error::FrameLength {
        expected: 12,
        got: 13,
    });
    assert!(s.contains("12") && s.contains("13"), "{s}");
    let s = shown(Error::WrongIdentity {
        expected: Kind::Knob,
        got: 0x7C,
    });
    assert!(s.contains("Knob") && s.contains("0x7c"), "{s}");
    let s = shown(Error::WrongChip {
        expected: ChipKind::Sht31,
        got: Some(0x74),
    });
    assert!(s.contains("Sht31") && s.contains("0x74"), "{s}");
    let s = shown(Error::WrongChip {
        expected: ChipKind::Hs3003,
        got: None,
    });
    assert!(s.contains("Hs3003"), "{s}");
    let s = shown(Error::I2c(7));
    assert!(s.contains('7'), "{s}");
}
