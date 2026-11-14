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

/// Semitones from A4 for every note. The match is exhaustive, so a new
/// `Note` fails to compile here until it is given its place.
fn semitones_from_a4(note: Note) -> Option<i32> {
    Some(match note {
        Note::FS3 => -15,
        Note::G3 => -14,
        Note::GS3 => -13,
        Note::A3 => -12,
        Note::AS3 => -11,
        Note::B3 => -10,
        Note::C4 => -9,
        Note::CS4 => -8,
        Note::D4 => -7,
        Note::DS4 => -6,
        Note::E4 => -5,
        Note::F4 => -4,
        Note::FS4 => -3,
        Note::G4 => -2,
        Note::GS4 => -1,
        Note::A4 => 0,
        Note::AS4 => 1,
        Note::B4 => 2,
        Note::C5 => 3,
        Note::CS5 => 4,
        Note::D5 => 5,
        Note::DS5 => 6,
        Note::E5 => 7,
        Note::F5 => 8,
        Note::FS5 => 9,
        Note::G5 => 10,
        Note::GS5 => 11,
        Note::A5 => 12,
        Note::AS5 => 13,
        Note::B5 => 14,
        Note::C6 => 15,
        Note::CS6 => 16,
        Note::D6 => 17,
        Note::DS6 => 18,
        Note::E6 => 19,
        Note::F6 => 20,
        Note::FS6 => 21,
        Note::G6 => 22,
        Note::GS6 => 23,
        Note::A6 => 24,
        Note::AS6 => 25,
        Note::B6 => 26,
        Note::C7 => 27,
        Note::CS7 => 28,
        Note::D7 => 29,
        Note::DS7 => 30,
        Note::E7 => 31,
        Note::F7 => 32,
        Note::FS7 => 33,
        Note::G7 => 34,
        Note::GS7 => 35,
        Note::A7 => 36,
        Note::AS7 => 37,
        Note::B7 => 38,
        Note::C8 => 39,
        Note::CS8 => 40,
        Note::D8 => 41,
        Note::DS8 => 42,
        Note::Rest => return None,
    })
}

#[test]
fn notes_are_equal_tempered_semitones_from_fs3() {
    // Every note is 440 Hz * 2^(n/12), rounded to the nearest hertz.
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
    for note in notes {
        let n = semitones_from_a4(note).expect("a pitched note");
        let hz = 440.0 * 2f64.powf(f64::from(n) / 12.0);
        let got = f64::from(note as u16);
        assert!(
            (got - hz).abs() <= 0.5,
            "{note:?} = {got}, expected {hz:.2}"
        );
    }
    // The list is every pitched note: consecutive semitones, FS3 to DS8.
    let offsets: Vec<i32> = notes
        .iter()
        .map(|&n| semitones_from_a4(n).unwrap())
        .collect();
    assert_eq!(offsets, (-15..=42).collect::<Vec<i32>>());
    assert_eq!(semitones_from_a4(Note::Rest), None);
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
