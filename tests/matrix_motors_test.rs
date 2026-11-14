//! LED matrix and Motors against the simulated firmware.

mod support;

use modulino_driver::{DecayMode, Error, LedMatrix, MatrixMode, MotorMode, MotorStatus, Motors};
use support::{FakeBus, FakeNode};

const MATRIX: u8 = 0x39;
const MOTORS: u8 = 0x24;

fn matrix_on_bus(grayscale: bool) -> LedMatrix<FakeBus> {
    LedMatrix::new(FakeBus::new(vec![FakeNode::led_matrix(grayscale)]))
}

#[test]
fn constructors_do_no_io() {
    assert_eq!(LedMatrix::new(FakeBus::default()).release().transactions, 0);
    assert_eq!(Motors::new(FakeBus::default()).release().transactions, 0);
}

#[test]
fn matrix_mode_switch_changes_the_frame_length() {
    let mut matrix = matrix_on_bus(false);
    assert_eq!(matrix.verify().unwrap(), MatrixMode::Mono);

    let mut picture = [[false; 12]; 8];
    picture[0][0] = true;
    matrix.show_mono(&picture).unwrap();

    matrix.set_mode(MatrixMode::Grayscale).unwrap();
    assert_eq!(
        matrix.show_mono(&picture),
        Err(Error::FrameLength {
            expected: 48,
            got: 12
        }),
        "a 12-byte frame is refused in grayscale"
    );
    let mut levels = [[0u8; 12]; 8];
    levels[0][0] = 15;
    matrix.show_gray(&levels).unwrap();

    matrix.set_mode(MatrixMode::Mono).unwrap();
    assert_eq!(matrix.verify().unwrap(), MatrixMode::Mono);

    let bus = matrix.release();
    let board = bus.node(MATRIX);
    let mut gray = vec![0u8; 48];
    gray[0] = 0xF0;
    let mut mono = vec![0u8; 12];
    mono[0] = 0x01;
    assert_eq!(board.frames, vec![mono, gray], "pictures shown");
    assert_eq!(
        board.dropped, 0,
        "mode commands were padded to the current length"
    );
    assert_eq!(board.matrix_grayscale, Some(false));
}

#[test]
fn matrix_mode_commands_are_padded_to_the_current_length() {
    // The board drops a command of the wrong length, so it stays in its
    // mode; padding to the new mode's length would leave it there.
    let mut matrix = matrix_on_bus(false);
    matrix.set_mode(MatrixMode::Grayscale).unwrap();
    assert_eq!(matrix.release().node(MATRIX).matrix_grayscale, Some(true));

    let mut matrix = matrix_on_bus(true);
    matrix.set_mode(MatrixMode::Mono).unwrap();
    assert_eq!(matrix.release().node(MATRIX).matrix_grayscale, Some(false));
}

#[test]
fn matrix_reads_its_mode_on_first_use() {
    let mut matrix = matrix_on_bus(true);
    let levels = [[1u8; 12]; 8];
    matrix.show_gray(&levels).unwrap();
    let bus = matrix.release();
    assert_eq!(bus.node(MATRIX).frames, vec![vec![0x11; 48]]);
}

#[test]
fn matrix_refuses_pictures_that_spell_a_mode_command() {
    let mut matrix = matrix_on_bus(false);
    // Columns 0-2 spell "GS4": the board would switch to grayscale.
    let mut picture = [[false; 12]; 8];
    for (col, byte) in b"GS4".iter().enumerate() {
        for (row, line) in picture.iter_mut().enumerate() {
            line[col] = byte >> row & 1 == 1;
        }
    }
    assert_eq!(matrix.show_mono(&picture), Err(Error::ReservedPrefix));
    let bus = matrix.release();
    let board = bus.node(MATRIX);
    assert!(board.frames.is_empty());
    assert_eq!(board.matrix_grayscale, Some(false));
}

#[test]
fn matrix_refuses_levels_above_15() {
    let mut matrix = matrix_on_bus(true);
    let mut levels = [[0u8; 12]; 8];
    levels[2][3] = 16;
    assert_eq!(matrix.show_gray(&levels), Err(Error::OutOfRange));
}

#[test]
fn matrix_unknown_mode_reply_is_an_error() {
    let bus = FakeBus::new(vec![FakeNode::new(0x72, b"XYZ", 12)]);
    let mut matrix = LedMatrix::new(bus);
    assert_eq!(matrix.verify(), Err(Error::DataError));
}

#[test]
fn motors_send_one_frame_per_command() {
    let bus = FakeBus::new(vec![FakeNode::new(0x48, &[0; 5], 8)]);
    let mut motors = Motors::new(bus);
    motors.set_mode(MotorMode::Dc).unwrap();
    motors.set_dc_frequency(20000).unwrap();
    motors.set_speeds(1000, -1).unwrap();
    motors.stop().unwrap();
    motors.set_mode(MotorMode::Stepper).unwrap();
    motors.set_half_step(true).unwrap();
    motors.step(-200, 50, 20).unwrap();
    motors.hold().unwrap();
    motors.release_coils().unwrap();
    motors.set_decay(DecayMode::Fast).unwrap();
    motors.set_half_full_scale(true).unwrap();
    assert_eq!(motors.set_dc_frequency(199), Err(Error::OutOfRange));
    assert_eq!(motors.set_speeds(i16::MIN, 0), Err(Error::OutOfRange));

    let bus = motors.release();
    let frames = &bus.node(MOTORS).frames;
    let f = |letter: u8, args: &[u8]| {
        let mut v = vec![letter];
        v.extend_from_slice(args);
        v.resize(8, 0);
        v
    };
    assert_eq!(
        *frames,
        vec![
            f(b'M', &[0]),
            f(b'F', &20000u16.to_le_bytes()),
            f(b'S', &[0xE8, 0x03, 0xFF, 0xFF]),
            f(b'S', &[0, 0, 0, 0]),
            f(b'M', &[1]),
            f(b'H', &[1]),
            f(b'G', &[0x38, 0xFF, 0xFF, 0xFF, 50, 0, 20]),
            f(b'G', &[0, 0, 0, 0, 1, 0, 0]),
            f(b'G', &[0, 0, 0, 0, 1, 0, 1]),
            f(b'T', &[3]),
            f(b'X', &[1]),
        ]
    );
}

#[test]
fn motors_status_decodes_currents_and_flags() {
    let bus = FakeBus::new(vec![FakeNode::new(
        0x48,
        &[0x34, 0x12, 0x10, 0x00, 0x7F],
        8,
    )]);
    let mut motors = Motors::new(bus);
    assert_eq!(
        motors.status().unwrap(),
        MotorStatus {
            current_a_raw: 0x1234,
            current_b_raw: 0x0010,
            busy: true,
            mode: MotorMode::Stepper,
            half_step: true,
            half_full_scale: true,
            decay: DecayMode::Fast,
            release_on_complete: true,
        }
    );
}
