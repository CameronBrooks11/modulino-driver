//! Chip drivers: exact register sequences (embedded-hal-mock), and address
//! fallback, identity and timeouts (FakeChip).

mod support;

use embedded_hal_mock::eh1::i2c::{Mock as I2cMock, Transaction as T};
use modulino_driver::chips::{Lps22hb, Lsm6dsox};
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
        T::write(a, vec![0x11, 0x00]),
        T::write(a, vec![0x12, 0x01]),
        T::write_read(a, vec![0x12], vec![0x01]),
        T::write_read(a, vec![0x12], vec![0x00]),
        T::write(a, vec![0x12, 0x44]),
        T::write(a, vec![0x10, 0x40]),
        T::write(a, vec![0x11, 0x40]),
        T::write_read(a, vec![0x1E], vec![0x01]),
        T::write_read(a, vec![0x1E], vec![0x01]),
        T::write_read(a, vec![0x28], vec![0x00, 0x00, 0x00, 0x00, 0x09, 0x40]),
        T::write_read(a, vec![0x22], vec![0xE8, 0x03, 0x18, 0xFC, 0x00, 0x00]),
    ];
    let mut movement = Movement::new(I2cMock::new(&expectations));
    movement.init().unwrap();
    assert!(movement.acceleration_ready().unwrap());
    assert!(!movement.angular_velocity_ready().unwrap());

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
            expected: ChipKind::Lps22hb,
            got: Some(0xB3)
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
        T::write(a, vec![0x11, 0x14]),
        T::write_read(a, vec![0x11], vec![0x14]),
        T::write_read(a, vec![0x11], vec![0x10]),
        T::write(a, vec![0x10, 0x22]),
        T::write_read(a, vec![0x27], vec![0x02]),
        T::write_read(a, vec![0x27], vec![0x02]),
        T::write_read(a, vec![0x28], vec![0x00, 0x00, 0x3F]),
        T::write_read(a, vec![0x28], vec![0xFF, 0xFF, 0xFF]),
        T::write_read(a, vec![0x2B], vec![0xC4, 0x09]),
        T::write_read(a, vec![0x2B], vec![0x18, 0xFC]),
    ];
    let mut pressure = Pressure::new(I2cMock::new(&expectations));
    pressure.init().unwrap();
    assert!(!pressure.pressure_ready().unwrap());
    assert!(pressure.temperature_ready().unwrap());
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

#[test]
fn movement_fallback_reports_the_wrong_chip_over_a_nack() {
    // Something with WHO_AM_I 0x00 at 0x6A, nothing at 0x6B.
    let bus = chips(vec![FakeChip::new(0x6A, 1)]);
    let mut movement = Movement::new(bus);
    assert_eq!(
        movement.verify(),
        Err(Error::WrongChip {
            expected: ChipKind::Lsm6dsox,
            got: Some(0x00)
        })
    );
    // Nothing anywhere: the NACK.
    let mut movement = Movement::new(FakeBus::default());
    assert_eq!(movement.verify(), Err(Error::I2c(Nack(0x6B))));
}

#[test]
fn chip_types_are_reachable_under_chips() {
    let _: Lsm6dsox<FakeBus> = Movement::new(FakeBus::default());
    let _: Lps22hb<FakeBus> = Pressure::new(FakeBus::default());
}

mod light {
    use super::*;
    use modulino_driver::chips::ltr381rgb::{lux, Gain, Rate, Resolution};
    use modulino_driver::{Light, LightReading};

    #[test]
    fn init_and_burst_read_sequence() {
        let a = 0x53;
        let expectations = [
            T::write_read(a, vec![0x06], vec![0xC2]),
            T::write_read(a, vec![0x07], vec![0x00]),
            T::write(a, vec![0x05, 0x04]),
            T::write(a, vec![0x04, 0x40]),
            T::write(a, vec![0x00, 0x06]),
            T::write_read(
                a,
                vec![0x0A],
                vec![
                    0xF4, 0x01, 0x00, // IR 500
                    0xE8, 0x03, 0x00, // green 1000
                    0x07, 0x08, 0x09, // red
                    0x0A, 0x0B, 0xFC, // blue, top nibble outside 20 bits
                ],
            ),
        ];
        let mut light = Light::new(I2cMock::new(&expectations));
        light.init().unwrap();
        let reading = light.read().unwrap();
        assert_eq!(
            (reading.ir, reading.green, reading.red, reading.blue),
            (500, 1000, 0x090807, 0x0C0B0A)
        );
        assert!(
            (reading.lux - 800.0 / 4.5 * 0.9835).abs() < 1e-3,
            "{reading:?}"
        );
        light.release().done();
    }

    #[test]
    fn lux_matches_the_0_3_worked_example() {
        // (0.8 * 1000) / (18 * 0.25) * (1 - 0.033 * 0.5) = 174.844
        let want = 800.0 / 4.5 * 0.9835;
        assert!((lux(1000, 500, Gain::X18, Resolution::Bits16) - want).abs() < 1e-3);
        assert_eq!(lux(0, 500, Gain::X18, Resolution::Bits16), 0.0);
        // IR far above green: the ratio is capped at 30, so the factor is
        // 1 - 0.033 * 30 = 0.01.
        let capped = lux(10, 10_000, Gain::X1, Resolution::Bits18);
        assert!((capped - 0.8 * 10.0 * 0.01).abs() < 1e-6, "{capped}");
    }

    #[test]
    fn gain_and_timing_follow_into_lux() {
        let a = 0x53;
        let expectations = [
            T::write(a, vec![0x05, 0x00]),
            T::write(a, vec![0x04, 0x31]),
            T::write_read(
                a,
                vec![0x0A],
                vec![0, 0, 0, 0xE8, 0x03, 0, 0, 0, 0, 0, 0, 0],
            ),
        ];
        let mut light = Light::new(I2cMock::new(&expectations));
        light.set_gain(Gain::X1).unwrap();
        light.set_timing(Resolution::Bits17, Rate::Ms50).unwrap();
        let LightReading { lux: got, .. } = light.read().unwrap();
        // 0.8 * 1000 / (1 x * 0.5)
        assert_eq!(got, 1600.0);
        light.release().done();
    }

    #[test]
    fn ready_reads_the_data_status_bit() {
        let a = 0x53;
        let expectations = [
            T::write_read(a, vec![0x07], vec![0x08]),
            T::write_read(a, vec![0x07], vec![0x20]),
        ];
        let mut light = Light::new(I2cMock::new(&expectations));
        assert!(light.ready().unwrap());
        assert!(!light.ready().unwrap(), "power-on status alone is not data");
        light.release().done();
    }

    #[test]
    fn wrong_part_id_is_wrong_chip() {
        let mut light = Light::new(chips(vec![FakeChip::new(0x53, 1).reg(0x06, 0xB2)]));
        assert_eq!(
            light.verify(),
            Err(Error::WrongChip {
                expected: ChipKind::Ltr381rgb,
                got: Some(0xB2)
            })
        );
        assert_eq!(Light::new(FakeBus::default()).release().transactions, 0);
    }
}

mod thermo {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, NoAcknowledgeSource};
    use modulino_driver::chips::{hs3003, sht31};
    use modulino_driver::spec::sensirion_crc8;
    use modulino_driver::Thermo;
    use support::FakeDelay;

    const NACK: ErrorKind = ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address);

    /// HS3003 fetch: status 00, humidity 0x2000 (50 %), temperature
    /// 6454 << 2 (25.0 C).
    const HS_FRESH: [u8; 4] = [0x20, 0x00, 0x64, 0xD8];
    /// The same with status 01: a sample already read.
    const HS_STALE: [u8; 4] = [0x60, 0x00, 0x64, 0xD8];

    fn sht_result(t: u16, rh: u16) -> Vec<u8> {
        let (t, rh) = (t.to_be_bytes(), rh.to_be_bytes());
        vec![
            t[0],
            t[1],
            sensirion_crc8(&t),
            rh[0],
            rh[1],
            sensirion_crc8(&rh),
        ]
    }

    fn sht_status() -> Vec<u8> {
        vec![0x80, 0x10, sensirion_crc8(&[0x80, 0x10])]
    }

    #[test]
    fn hs3003_decodes_and_skips_stale_samples() {
        let r = hs3003::decode(&HS_FRESH).unwrap();
        assert!((r.temperature - 25.0).abs() < 0.01, "{r:?}");
        assert!((r.humidity - 50.0).abs() < 0.01, "{r:?}");
        assert_eq!(hs3003::decode(&HS_STALE), None);
    }

    #[test]
    fn hs3003_sequence_with_one_stale_fetch() {
        let a = 0x44;
        let expectations = [
            T::read(a, vec![0x00]),
            T::write(a, vec![0x00]),
            T::read(a, HS_STALE.to_vec()),
            T::read(a, HS_FRESH.to_vec()),
        ];
        let mut thermo = Thermo::new(I2cMock::new(&expectations));
        assert_eq!(thermo.verify().unwrap(), ChipKind::Hs3003);
        let mut delay = FakeDelay::default();
        let r = thermo.read(&mut delay).unwrap();
        assert!((r.temperature - 25.0).abs() < 0.01);
        assert_eq!(
            delay.total_ns,
            u64::from(hs3003::CONVERSION_MS + hs3003::RETRY_MS) * 1_000_000
        );
        thermo.release().done();
    }

    #[test]
    fn hs3003_that_stays_stale_times_out() {
        let a = 0x44;
        let mut expectations = vec![T::read(a, vec![0x00]), T::write(a, vec![0x00])];
        expectations.extend((0..hs3003::FETCHES).map(|_| T::read(a, HS_STALE.to_vec())));
        let mut thermo = Thermo::new(I2cMock::new(&expectations));
        let mut delay = FakeDelay::default();
        assert_eq!(thermo.read(&mut delay), Err(Error::Timeout));
        let waited = hs3003::CONVERSION_MS + hs3003::RETRY_MS * hs3003::FETCHES as u32;
        assert_eq!(delay.total_ns, u64::from(waited) * 1_000_000);
        thermo.release().done();
    }

    #[test]
    fn falls_back_to_sht31_and_retries_a_busy_read() {
        let expectations = [
            T::read(0x44, vec![0x00]).with_error(NACK),
            T::write(0x45, vec![0xF3, 0x2D]),
            T::read(0x45, sht_status()),
            T::write(0x45, vec![0x24, 0x00]),
            T::read(0x45, vec![0; 6]).with_error(NACK),
            T::read(0x45, sht_result(0x6666, 0x8000)),
        ];
        let mut thermo = Thermo::new(I2cMock::new(&expectations));
        let mut delay = FakeDelay::default();
        let r = thermo.read(&mut delay).unwrap();
        assert_eq!(thermo.chip(), Some(ChipKind::Sht31));
        assert_eq!(thermo.address().map(|a| a.get()), Some(0x45));
        assert!((r.temperature - 25.0).abs() < 0.01, "{r:?}");
        assert!((r.humidity - 50.0).abs() < 0.01, "{r:?}");
        assert_eq!(
            delay.total_ns,
            u64::from(sht31::MEASURE_MS + sht31::RETRY_MS) * 1_000_000
        );
        thermo.release().done();
    }

    #[test]
    fn sht31_bad_crc_is_a_data_error() {
        let mut bad = sht_result(0x6666, 0x8000);
        bad[5] ^= 0xFF;
        assert_eq!(sht31::decode(&bad.clone().try_into().unwrap()), None);
        let expectations = [
            T::read(0x44, vec![0x00]).with_error(NACK),
            T::write(0x45, vec![0xF3, 0x2D]),
            T::read(0x45, sht_status()),
            T::write(0x45, vec![0x24, 0x00]),
            T::read(0x45, bad),
        ];
        let mut thermo = Thermo::new(I2cMock::new(&expectations));
        assert_eq!(
            thermo.read(&mut FakeDelay::default()),
            Err(Error::DataError)
        );
        thermo.release().done();
    }

    #[test]
    fn nothing_there_is_the_sht31_nack() {
        let mut thermo = Thermo::new(FakeBus::default());
        assert_eq!(thermo.verify(), Err(Error::I2c(Nack(0x45))));
        assert_eq!(thermo.chip(), None);
        assert_eq!(Thermo::new(FakeBus::default()).release().transactions, 0);
    }
}
