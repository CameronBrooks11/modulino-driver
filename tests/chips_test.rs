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
        // 40 ms conversion, 5 ms before the second fetch.
        assert_eq!(delay.total_ns, 45 * 1_000_000);
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
        // 40 ms conversion, then 5 ms before each fetch after the first.
        let waited = 40 + 5 * (hs3003::FETCHES as u64 - 1);
        assert_eq!(delay.total_ns, waited * 1_000_000);
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
        // 15 ms measurement, 1 ms before the second read.
        assert_eq!(delay.total_ns, 16 * 1_000_000);
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
    fn a_bus_fault_at_0x44_outranks_the_sht31_nack() {
        let expectations = [
            T::read(0x44, vec![0x00]).with_error(ErrorKind::Bus),
            T::write(0x45, vec![0xF3, 0x2D]).with_error(NACK),
        ];
        let mut thermo = Thermo::new(I2cMock::new(&expectations));
        assert_eq!(thermo.verify(), Err(Error::I2c(ErrorKind::Bus)));
        thermo.release().done();
    }

    #[test]
    fn a_knob_at_0x45_is_not_taken_for_an_sht31() {
        // Count 144 makes the knob's reply pass the SHT3x status CRC.
        let knob = support::FakeNode::new(0x74, &[0x90, 0x00, 0x00], 4).at(0x45);
        let mut thermo = Thermo::new(FakeBus::new(vec![knob]));
        assert_eq!(
            thermo.verify(),
            Err(Error::WrongChip {
                expected: ChipKind::Sht31,
                got: Some(0x74)
            })
        );
        assert_eq!(thermo.chip(), None);
    }

    #[test]
    fn nothing_there_is_the_sht31_nack() {
        let mut thermo = Thermo::new(FakeBus::default());
        assert_eq!(thermo.verify(), Err(Error::I2c(Nack(0x45))));
        assert_eq!(thermo.chip(), None);
        assert_eq!(Thermo::new(FakeBus::default()).release().transactions, 0);
    }
}

mod distance {
    use super::*;
    use modulino_driver::chips::vl53l4::{macro_period_us, range_config, Variant};
    use modulino_driver::{Distance, DistanceReading};
    use support::FakeDelay;

    /// ST's default configuration for 0x2D..=0x87, copied from
    /// `VL53L4CD_DEFAULT_CONFIGURATION` (stm32duino/VL53L4CD@b6ed0e2).
    const ST_CONFIGURATION: [u8; 91] = [
        0x12, 0x00, 0x00, 0x11, 0x02, 0x00, 0x02, 0x08, 0x00, 0x08, 0x10, 0x01, 0x01, 0x00, 0x00,
        0x00, 0x00, 0xff, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x0b, 0x00, 0x00, 0x02,
        0x14, 0x21, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0xc8, 0x00, 0x00, 0x38, 0xff, 0x01,
        0x00, 0x08, 0x00, 0x00, 0x01, 0xcc, 0x07, 0x01, 0xf1, 0x05, 0x00, 0xa0, 0x00, 0x80, 0x08,
        0x38, 0x00, 0x00, 0x00, 0x00, 0x0f, 0x89, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        0x07, 0x05, 0x06, 0x06, 0x00, 0x00, 0x02, 0xc7, 0xff, 0x9B, 0x00, 0x00, 0x00, 0x01, 0x00,
        0x00,
    ];

    /// A VL53L4 that has booted and reports data ready, with an
    /// `OSC_FREQUENCY` of 3213.
    fn sensor(model: u8) -> FakeChip {
        FakeChip::new(0x29, 2)
            .reg(0x010F, model)
            .reg(0x0110, 0xAA)
            .reg(0x00E5, 0x03)
            .reg(0x0030, 0x01)
            .reg(0x0031, 0x01)
            .reg(0x0006, 0x0C)
            .reg(0x0007, 0x8D)
    }

    fn byte(bus: &FakeBus, reg: u16) -> u8 {
        bus.chips[0].regs.get(&reg).copied().unwrap_or(0)
    }

    fn word(bus: &FakeBus, reg: u16) -> u16 {
        u16::from_be_bytes([byte(bus, reg), byte(bus, reg + 1)])
    }

    fn dword(bus: &FakeBus, reg: u16) -> u32 {
        u32::from_be_bytes([0, 1, 2, 3].map(|i| byte(bus, reg + i)))
    }

    /// The single bytes written to `reg`, in order.
    fn written(bus: &FakeBus, reg: u16) -> Vec<u8> {
        let [hi, lo] = reg.to_be_bytes();
        bus.chips[0]
            .writes
            .iter()
            .filter(|w| w.len() == 3 && w[0] == hi && w[1] == lo)
            .map(|w| w[2])
            .collect()
    }

    #[test]
    fn timing_registers_match_st_arithmetic() {
        // Vectors from a separate transcription of ST's SetRangeTiming.
        let macro_us = macro_period_us(3213);
        assert_eq!(macro_us, 0xB792E8);
        assert_eq!(range_config(50_000 - 2500, macro_us), (0x40, 0x55));
        assert_eq!(range_config(100_000 - 2500, macro_us), (0x84, 0xB0));
        assert_eq!(range_config((20_000 - 4300) / 2, macro_us), (0x0A, 0x0D));
        assert_eq!(
            range_config(200_000 - 2500, macro_period_us(3072)),
            (0x180, 0x1AB)
        );
        // A degenerate oscillator reading wraps like ST's C instead of
        // panicking.
        let _ = range_config(10_000, macro_period_us(1));
    }

    #[test]
    fn verify_tells_the_variants_apart() {
        let mut cd = Distance::new(chips(vec![sensor(0xEB)]));
        assert_eq!(cd.verify(), Ok(Variant::Cd));
        let mut ed = Distance::new(chips(vec![sensor(0xEC)]));
        assert_eq!(ed.verify(), Ok(Variant::Ed));
        assert_eq!(ed.variant().map(Variant::kind), Some(ChipKind::Vl53l4ed));
        let mut other = Distance::new(chips(vec![sensor(0xEA)]));
        assert_eq!(
            other.verify(),
            Err(Error::WrongChip {
                expected: ChipKind::Vl53l4cd,
                got: Some(0xEA)
            })
        );
        assert_eq!(Distance::new(FakeBus::default()).release().transactions, 0);
    }

    #[test]
    fn init_follows_st_sensor_init() {
        for (model, a, b) in [(0xEB, 0x40, 0x55), (0xEC, 0x84, 0xB0)] {
            let mut d = Distance::new(chips(vec![sensor(model)]));
            d.init(&mut FakeDelay::default()).unwrap();
            let bus = d.release();
            // The load: one byte per register from 0x2D, in order.
            let load: Vec<(u16, u8)> = bus.chips[0]
                .writes
                .iter()
                .filter(|w| w.len() == 3)
                .take(91)
                .map(|w| (u16::from_be_bytes([w[0], w[1]]), w[2]))
                .collect();
            let expected: Vec<(u16, u8)> = (0x2D..=0x87).zip(ST_CONFIGURATION).collect();
            assert_eq!(load, expected, "configuration for {model:#x}");
            // Configuration, VHV start, stop.
            assert_eq!(written(&bus, 0x0087), [0x00, 0x40, 0x00]);
            // Configuration, then the clear after VHV.
            assert_eq!(written(&bus, 0x0086), [0x00, 0x01]);
            assert_eq!(written(&bus, 0x0008), [0x09]);
            assert_eq!(written(&bus, 0x000B), [0x00]);
            assert_eq!(word(&bus, 0x0024), 0x0500);
            assert_eq!(word(&bus, 0x005E), a, "RANGE_CONFIG_A for {model:#x}");
            assert_eq!(word(&bus, 0x0061), b, "RANGE_CONFIG_B for {model:#x}");
            // The configuration leaves 0x00000F89 here; continuous is 0.
            assert_eq!(dword(&bus, 0x006C), 0, "continuous mode");
        }
    }

    #[test]
    fn a_sensor_that_never_boots_times_out() {
        let mut d = Distance::new(chips(vec![sensor(0xEB).reg(0x00E5, 0x00)]));
        let mut delay = FakeDelay::default();
        assert_eq!(d.init(&mut delay), Err(Error::Timeout));
        // 1000 polls, 1 ms apart.
        assert_eq!(delay.total_ns, 1000 * 1_000_000);
    }

    #[test]
    fn a_calibration_that_never_finishes_times_out() {
        // The configuration sets the line active low (0x30 = 0x11); a
        // status that stays high never reports ready.
        let mut d = Distance::new(chips(vec![sensor(0xEB).fixed(0x0031, 0x01)]));
        assert_eq!(d.init(&mut FakeDelay::default()), Err(Error::Timeout));
    }

    #[test]
    fn range_timing_is_checked() {
        let mut d = Distance::new(chips(vec![sensor(0xEB)
            .reg(0x00DE, 0x03)
            .reg(0x00DF, 0xFF)]));
        assert_eq!(d.set_range_timing(9, 0), Err(Error::OutOfRange));
        assert_eq!(d.set_range_timing(201, 0), Err(Error::OutOfRange));
        assert_eq!(d.set_range_timing(50, 50), Err(Error::OutOfRange));
        // 1.055 * 10 000 000 * 1023 does not fit the 32-bit register.
        assert_eq!(d.set_range_timing(20, 10_000_000), Err(Error::OutOfRange));
        // The boundary: 1.055 * 3979530 * 1023 is exactly 2^32 in f32, the
        // value below it fits.
        assert_eq!(d.set_range_timing(20, 3_979_530), Err(Error::OutOfRange));
        let bus = d.release();
        // Only register-pointer writes for the reads; no data written.
        assert!(
            bus.chips[0].writes.iter().all(|w| w.len() == 2),
            "nothing written"
        );
        let mut d = Distance::new(chips(vec![sensor(0xEB)
            .reg(0x00DE, 0x03)
            .reg(0x00DF, 0xFF)]));
        d.set_range_timing(20, 3_979_529).unwrap();
        assert_eq!(dword(&d.release(), 0x006C), 4_294_966_272);
        let mut zero = Distance::new(chips(vec![sensor(0xEB).reg(0x0006, 0).reg(0x0007, 0)]));
        assert_eq!(zero.set_range_timing(50, 0), Err(Error::DataError));
        let mut no_pll = Distance::new(chips(vec![sensor(0xEB)]));
        assert_eq!(no_pll.set_range_timing(20, 100), Err(Error::DataError));
    }

    #[test]
    fn autonomous_timing_writes_the_period() {
        let mut d = Distance::new(chips(vec![sensor(0xEB)
            .reg(0x00DE, 0xFD)
            .reg(0x00DF, 0xF3)]));
        d.set_range_timing(20, 100).unwrap();
        let bus = d.release();
        // The PLL value is the low 10 bits of 0xFDF3, 499; 1.055 * 100 * 499,
        // truncated, is 52644.
        assert_eq!(dword(&bus, 0x006C), 52644);
        assert_eq!((word(&bus, 0x005E), word(&bus, 0x0061)), (0x0A, 0x0D));

        // f32, left to right as in ST's C: (1.055 * 18) * 300 is 5697;
        // 1.055 * (18 * 300) would be 5696.
        let mut d = Distance::new(chips(vec![sensor(0xEB)
            .reg(0x00DE, 0x01)
            .reg(0x00DF, 0x2C)]));
        d.set_range_timing(10, 18).unwrap();
        assert_eq!(dword(&d.release(), 0x006C), 5697);
    }

    #[test]
    fn read_maps_status_and_clears_the_interrupt() {
        let chip = sensor(0xEB)
            .reg(0x0089, 0x09)
            .reg(0x008C, 0x0C)
            .reg(0x008E, 0x01)
            .reg(0x0091, 0x10)
            .reg(0x0093, 0x28)
            .reg(0x0096, 0x01)
            .reg(0x0097, 0xF4);
        let mut d = Distance::new(chips(vec![chip]));
        assert_eq!(
            d.read().unwrap(),
            DistanceReading {
                distance_mm: 500,
                status: 0,
                signal_kcps: 0x0100 * 8,
                ambient_kcps: 0x10 * 8,
                sigma_mm: 0x28 / 4,
                spads: 12
            }
        );
        assert_eq!(written(&d.release(), 0x0086), [0x01]);

        // No SPADs: no target, whatever the raw status.
        let mut d = Distance::new(chips(vec![sensor(0xEB).reg(0x0089, 0x09)]));
        assert_eq!(d.read().unwrap().status, 255);
    }

    #[test]
    fn start_ranging_by_variant_and_mode() {
        // CD, continuous: start, wait for the first sample, clear it.
        let mut cd = Distance::new(chips(vec![sensor(0xEB)]));
        cd.start_ranging(&mut FakeDelay::default()).unwrap();
        let bus = cd.release();
        assert_eq!(written(&bus, 0x0087), [0x21]);
        assert_eq!(written(&bus, 0x0086), [0x01], "first sample cleared");

        // A period set: autonomous.
        let mut auto = Distance::new(chips(vec![sensor(0xEB).reg(0x006F, 0x01)]));
        auto.start_ranging(&mut FakeDelay::default()).unwrap();
        assert_eq!(written(&auto.release(), 0x0087), [0x40]);

        // ED: start only.
        let mut ed = Distance::new(chips(vec![sensor(0xEC)]));
        ed.start_ranging(&mut FakeDelay::default()).unwrap();
        let bus = ed.release();
        assert_eq!(written(&bus, 0x0087), [0x21]);
        assert_eq!(written(&bus, 0x0086), [] as [u8; 0]);
    }

    #[test]
    fn only_the_cd_waits_for_its_first_sample() {
        // No verify beforehand: start_ranging finds the variant itself.
        let never_ready = |model| sensor(model).reg(0x0031, 0x00);
        let mut cd = Distance::new(chips(vec![never_ready(0xEB)]));
        assert_eq!(
            cd.start_ranging(&mut FakeDelay::default()),
            Err(Error::Timeout)
        );
        let mut ed = Distance::new(chips(vec![never_ready(0xEC)]));
        ed.start_ranging(&mut FakeDelay::default()).unwrap();
    }
}

mod failure_paths {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, NoAcknowledgeSource};
    use modulino_driver::chips::ltr381rgb::{lux, Gain, Rate, Resolution};
    use modulino_driver::spec::sensirion_crc8;
    use modulino_driver::{Light, Thermo};

    const NACK: ErrorKind = ErrorKind::NoAcknowledge(NoAcknowledgeSource::Data);
    const LIGHT: u8 = 0x53;
    /// IR 0, green 1000, red and blue 0.
    const BURST: [u8; 12] = [0, 0, 0, 0xE8, 0x03, 0, 0, 0, 0, 0, 0, 0];

    #[test]
    fn a_failed_set_gain_keeps_the_old_gain() {
        let expectations = [
            T::write(LIGHT, vec![0x05, Gain::X1 as u8]).with_error(NACK),
            T::write_read(LIGHT, vec![0x0A], BURST.to_vec()),
        ];
        let mut light = Light::new(I2cMock::new(&expectations));
        assert!(light.set_gain(Gain::X1).is_err());
        let reading = light.read().unwrap();
        // Still the default gain and resolution.
        assert_eq!(reading.lux, lux(1000, 0, Gain::X18, Resolution::Bits16));
        light.release().done();
    }

    #[test]
    fn a_failed_set_timing_is_an_error_and_keeps_the_old_resolution() {
        let code = (Resolution::Bits20 as u8) << 4 | Rate::Ms100 as u8;
        let expectations = [
            T::write(LIGHT, vec![0x04, code]).with_error(NACK),
            T::write_read(LIGHT, vec![0x0A], BURST.to_vec()),
        ];
        let mut light = Light::new(I2cMock::new(&expectations));
        assert_eq!(
            light.set_timing(Resolution::Bits20, Rate::Ms100),
            Err(Error::I2c(NACK))
        );
        let reading = light.read().unwrap();
        assert_eq!(reading.lux, lux(1000, 0, Gain::X18, Resolution::Bits16));
        light.release().done();
    }

    #[test]
    fn an_sht31_that_never_releases_its_status_is_a_timeout() {
        // Nothing at 0x44; at 0x45 the command is accepted and every
        // status read NACKs.
        let status = [0x80, 0x10];
        let mut sht = FakeChip::new(0x45, 1).command(
            &[0xF3, 0x2D],
            &[status[0], status[1], sensirion_crc8(&status)],
        );
        sht.nack_idle_reads = true;
        sht.reply_after = usize::MAX;
        let mut thermo = Thermo::new(chips(vec![sht]));
        assert_eq!(thermo.verify(), Err(Error::Timeout));
    }

    #[test]
    fn a_failed_movement_verify_keeps_the_first_address() {
        let mut movement = Movement::new(FakeBus::default());
        assert!(movement.verify().is_err());
        assert_eq!(movement.address(), Address::new(0x6A).unwrap());
    }
}
