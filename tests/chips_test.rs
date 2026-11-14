//! Chip drivers: exact register sequences (embedded-hal-mock), and address
//! fallback, identity and timeouts (FakeChip).

mod support;

use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction as T};
use modulino_driver::{Address, Axes, ChipKind, Error, Movement, Pressure};
use support::{FakeBus, FakeChip, Nack};

fn chips(chips: Vec<FakeChip>) -> FakeBus {
    FakeBus {
        chips,
        ..Default::default()
    }
}

#[test]
fn movement_init_and_read_sequence() {
    let a = 0x6A;
    let expectations = [
        T::write_read(a, vec![0x0F], vec![0x6C]),
        T::write(a, vec![0x12, 0x01]),
        T::write_read(a, vec![0x12], vec![0x01]),
        T::write_read(a, vec![0x12], vec![0x00]),
        T::write(a, vec![0x12, 0x44]),
        T::write(a, vec![0x10, 0x40]),
        T::write(a, vec![0x11, 0x40]),
        T::write_read(a, vec![0x1E], vec![0x03]),
        T::write_read(a, vec![0x28], vec![0x00, 0x00, 0x00, 0x00, 0x09, 0x40]),
        T::write_read(a, vec![0x22], vec![0xE8, 0x03, 0x18, 0xFC, 0x00, 0x00]),
    ];
    let mut movement = Movement::new(I2cMock::new(&expectations));
    movement.init().unwrap();
    assert!(movement.data_ready().unwrap());

    let g = movement.acceleration().unwrap();
    assert_eq!((g.x, g.y), (0.0, 0.0));
    assert!((g.z - 16393.0 * 0.061 / 1000.0).abs() < 1e-6, "{g:?}");

    let dps = movement.angular_velocity().unwrap();
    assert_eq!(
        dps,
        Axes {
            x: 1000.0 * 8.75 / 1000.0,
            y: -1000.0 * 8.75 / 1000.0,
            z: 0.0
        }
    );
    movement.release().done();
}

#[test]
fn movement_falls_back_to_0x6b() {
    let bus = chips(vec![FakeChip::new(0x6B, 1).reg(0x0F, 0x6C)]);
    let mut movement = Movement::new(bus);
    assert_eq!(movement.address().get(), 0x6A);
    movement.verify().unwrap();
    assert_eq!(movement.address().get(), 0x6B);
}

#[test]
fn movement_with_a_fixed_address_does_not_fall_back() {
    let bus = chips(vec![FakeChip::new(0x6B, 1).reg(0x0F, 0x6C)]);
    let mut movement = Movement::with_address(bus, Address::new(0x6A).unwrap());
    assert_eq!(movement.verify(), Err(Error::I2c(Nack(0x6A))));
}

#[test]
fn wrong_chip_and_missing_chip_are_told_apart() {
    let mut pressure = Pressure::new(chips(vec![FakeChip::new(0x5C, 1).reg(0x0F, 0xB3)]));
    assert_eq!(
        pressure.verify(),
        Err(Error::WrongChip {
            expected: ChipKind::Lps22hb
        })
    );
    let mut pressure = Pressure::new(FakeBus::default());
    assert_eq!(pressure.verify(), Err(Error::I2c(Nack(0x5C))));
}

#[test]
fn movement_reset_that_never_clears_times_out() {
    // FakeChip keeps whatever is written, so SW_RESET stays set.
    let bus = chips(vec![FakeChip::new(0x6A, 1).reg(0x0F, 0x6C)]);
    let mut movement = Movement::new(bus);
    assert_eq!(movement.init(), Err(Error::Timeout));
}

#[test]
fn pressure_init_and_read_sequence() {
    let a = 0x5C;
    let expectations = [
        T::write_read(a, vec![0x0F], vec![0xB1]),
        T::write(a, vec![0x11, 0x04]),
        T::write_read(a, vec![0x11], vec![0x14]),
        T::write_read(a, vec![0x11], vec![0x10]),
        T::write(a, vec![0x10, 0x22]),
        T::write_read(a, vec![0x27], vec![0x01]),
        T::write_read(a, vec![0x28], vec![0x00, 0x00, 0x3F]),
        T::write_read(a, vec![0x28], vec![0xFF, 0xFF, 0xFF]),
        T::write_read(a, vec![0x2B], vec![0xC4, 0x09]),
        T::write_read(a, vec![0x2B], vec![0x18, 0xFC]),
    ];
    let mut pressure = Pressure::new(I2cMock::new(&expectations));
    pressure.init().unwrap();
    assert!(pressure.data_ready().unwrap());
    assert_eq!(pressure.pressure().unwrap(), 0x3F0000 as f32 / 4096.0);
    assert_eq!(pressure.pressure().unwrap(), -1.0 / 4096.0, "sign-extended");
    assert_eq!(pressure.temperature().unwrap(), 25.0);
    assert_eq!(pressure.temperature().unwrap(), -10.0);
    pressure.release().done();
}

#[test]
fn pressure_reset_that_never_clears_times_out() {
    let bus = chips(vec![FakeChip::new(0x5C, 1).reg(0x0F, 0xB1)]);
    let mut pressure = Pressure::new(bus);
    assert_eq!(pressure.init(), Err(Error::Timeout));
}

#[test]
fn constructors_do_no_io() {
    assert_eq!(Movement::new(FakeBus::default()).release().transactions, 0);
    assert_eq!(Pressure::new(FakeBus::default()).release().transactions, 0);
}
