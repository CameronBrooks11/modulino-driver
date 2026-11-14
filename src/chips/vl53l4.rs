//! VL53L4CD and VL53L4ED time-of-flight sensors (Modulino Distance).
//!
//! The procedure and register facts follow ST's ultra-low-power drivers as
//! stm32duino ships them, which Arduino_Modulino uses: VL53L4CD at
//! `b6ed0e2` and VL53L4ED at `a94cc0b` (`src/vl53l4*_api.cpp`,
//! `src/vl53l4*_class.h`). Facts are taken from them; code is not. The two
//! chips share the register map and procedure; they differ in model ID
//! (0xEBAA and 0xECAA at 0x010F, `class.h` `InitSensor`), in the default
//! timing budget (50 and 100 ms, `SensorInit`), and in that the CD's start
//! consumes the first sample (`StartRanging`). Registers have 16-bit
//! indices, and words are big-endian.

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

use crate::addr::Address;
use crate::spec::{ChipId, ChipKind, CHIPS};
use crate::Error;

const OSC_FREQUENCY: u16 = 0x0006;
const VHV_CONFIG_TIMEOUT_MACROP_LOOP_BOUND: u16 = 0x0008;
const GPIO_HV_MUX_CTRL: u16 = 0x0030;
const GPIO_TIO_HV_STATUS: u16 = 0x0031;
const RANGE_CONFIG_A: u16 = 0x005E;
const RANGE_CONFIG_B: u16 = 0x0061;
const INTERMEASUREMENT_MS: u16 = 0x006C;
const SYSTEM_INTERRUPT_CLEAR: u16 = 0x0086;
const SYSTEM_START: u16 = 0x0087;
const RESULT_RANGE_STATUS: u16 = 0x0089;
const RESULT_SPAD_NB: u16 = 0x008C;
const RESULT_SIGNAL_RATE: u16 = 0x008E;
const RESULT_AMBIENT_RATE: u16 = 0x0090;
const RESULT_SIGMA: u16 = 0x0092;
const RESULT_DISTANCE: u16 = 0x0096;
const RESULT_OSC_CALIBRATE_VAL: u16 = 0x00DE;
const FIRMWARE_SYSTEM_STATUS: u16 = 0x00E5;

/// `FIRMWARE_SYSTEM_STATUS` once booted.
const BOOTED: u8 = 0x03;
/// Start the VHV calibration, or ranging with an inter-measurement period.
const START_AUTONOMOUS: u8 = 0x40;
/// Start continuous ranging (inter-measurement period 0).
const START_CONTINUOUS: u8 = 0x21;

/// The default configuration written to 0x2D..=0x87 at init, identical in
/// both drivers (`VL53L4CD_DEFAULT_CONFIGURATION`, `vl53l4ed_config_default.h`).
const DEFAULT_CONFIGURATION: [u8; 91] = [
    0x12, 0x00, 0x00, 0x11, 0x02, 0x00, 0x02, 0x08, 0x00, 0x08, 0x10, 0x01, 0x01, 0x00, 0x00, 0x00,
    0x00, 0xff, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x0b, 0x00, 0x00, 0x02, 0x14, 0x21,
    0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0xc8, 0x00, 0x00, 0x38, 0xff, 0x01, 0x00, 0x08, 0x00,
    0x00, 0x01, 0xcc, 0x07, 0x01, 0xf1, 0x05, 0x00, 0xa0, 0x00, 0x80, 0x08, 0x38, 0x00, 0x00, 0x00,
    0x00, 0x0f, 0x89, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x07, 0x05, 0x06, 0x06, 0x00,
    0x00, 0x02, 0xc7, 0xff, 0x9B, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
];
const CONFIG_START: u16 = 0x2D;

/// Polls, 1 ms apart, before a wait gives up: about the 1 s ST's drivers
/// allow (their loops poll up to 1001 times).
pub const WAIT_POLLS: u32 = 1000;

/// Which of the two chips answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Variant {
    /// VL53L4CD, model ID 0xEB
    Cd,
    /// VL53L4ED, model ID 0xEC
    Ed,
}

impl Variant {
    /// The chip kind in `spec`.
    pub fn kind(self) -> ChipKind {
        match self {
            Variant::Cd => ChipKind::Vl53l4cd,
            Variant::Ed => ChipKind::Vl53l4ed,
        }
    }

    /// The model ID register, value and mask, from `spec::CHIPS`.
    fn model_id(self) -> (u16, u8, u8) {
        let spec = CHIPS
            .iter()
            .find(|c| c.kind == self.kind())
            .expect("both variants are in spec::CHIPS");
        match spec.id {
            ChipId::Register16 {
                register,
                value,
                mask,
            } => (register, value, mask),
            _ => unreachable!("VL53L4 identities are 16-bit registers"),
        }
    }

    /// The timing budget ST's driver sets at init, in ms.
    pub fn default_budget_ms(self) -> u32 {
        match self {
            Variant::Cd => 50,
            Variant::Ed => 100,
        }
    }
}

/// ST's range status codes, from the raw status (`GetResult`'s table).
const STATUS_MAP: [u8; 24] = [
    255, 255, 255, 5, 2, 4, 1, 7, 3, 0, 255, 255, 9, 13, 255, 255, 255, 255, 10, 6, 255, 255, 11,
    12,
];

/// One ranging result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DistanceReading {
    /// Distance in mm
    pub distance_mm: u16,
    /// ST's range status: 0 is a valid measurement; other values flag
    /// problems (see ST's user manual); 255 means no target or unknown
    pub status: u8,
    /// Return signal rate, kcps
    pub signal_kcps: u32,
    /// Ambient rate, kcps
    pub ambient_kcps: u32,
    /// Distance standard deviation estimate, mm
    pub sigma_mm: u16,
    /// SPADs enabled for the measurement
    pub spads: u16,
}

/// The `RANGE_CONFIG_A` and `RANGE_CONFIG_B` values for a timing budget,
/// given the macro period: ST's `SetRangeTiming` encoding, in the same
/// wrapping 32-bit arithmetic as ST's C.
pub fn range_config(budget_us: u32, macro_period_us: u32) -> (u16, u16) {
    let encode = |factor: u32| {
        let budget = budget_us << 12;
        let step = (macro_period_us.wrapping_mul(factor) >> 6).max(1);
        let mut ls = (budget.wrapping_add(step >> 1) / step).wrapping_sub(1);
        let mut ms: u16 = 0;
        while ls & 0xFFFF_FF00 != 0 {
            ls >>= 1;
            ms += 1;
        }
        (ms << 8) + (ls & 0xFF) as u16
    };
    (encode(16), encode(12))
}

/// The macro period for a non-zero `OSC_FREQUENCY` reading, in ST's units
/// and wrapping 32-bit arithmetic.
pub fn macro_period_us(osc_frequency: u16) -> u32 {
    2304u32.wrapping_mul(0x4000_0000 / osc_frequency.max(1) as u32) >> 6
}

/// VL53L4CD/ED driver; [`Distance`](crate::Distance) is this type.
///
/// ```rust,ignore
/// use modulino_driver::Distance;
///
/// let mut distance = Distance::new(i2c);
/// distance.init(&mut delay)?; // finds CD or ED, boots, calibrates
/// distance.start_ranging(&mut delay)?;
/// if distance.data_ready()? {
///     let r = distance.read()?;
/// }
/// ```
#[derive(Debug)]
pub struct Vl53l4<I2C> {
    i2c: I2C,
    addr: Address,
    variant: Option<Variant>,
}

impl<I2C: I2c> Vl53l4<I2C> {
    /// The sensor at the Modulino's 0x29. Does no I/O.
    pub fn new(i2c: I2C) -> Self {
        Self::with_address(i2c, Address::new(0x29).expect("0x29 is an address"))
    }

    /// The sensor at `addr`. Does no I/O.
    pub fn with_address(i2c: I2C, addr: Address) -> Self {
        Self {
            i2c,
            addr,
            variant: None,
        }
    }

    /// The sensor's address.
    pub fn address(&self) -> Address {
        self.addr
    }

    /// The variant the last successful [`Vl53l4::verify`] found.
    pub fn variant(&self) -> Option<Variant> {
        self.variant
    }

    /// Read the model ID and keep the variant: 0xEB is a VL53L4CD, 0xEC a
    /// VL53L4ED (`spec::CHIPS`), anything else [`Error::WrongChip`].
    pub fn verify(&mut self) -> Result<Variant, Error<I2C::Error>> {
        let (register, _, _) = Variant::Cd.model_id();
        let got = self.read_u8(register)?;
        let found = [Variant::Cd, Variant::Ed].into_iter().find(|v| {
            let (_, value, mask) = v.model_id();
            got & mask == value
        });
        match found {
            Some(variant) => {
                self.variant = Some(variant);
                Ok(variant)
            }
            None => Err(Error::WrongChip {
                expected: ChipKind::Vl53l4cd,
                got: Some(got),
            }),
        }
    }

    /// ST's `SensorInit`: verify, wait for boot, load the default
    /// configuration, run the VHV calibration, then set the variant's default
    /// timing budget in continuous mode. Each wait polls every 1 ms, at most
    /// [`WAIT_POLLS`] times, then [`Error::Timeout`].
    pub fn init(&mut self, delay: &mut impl DelayNs) -> Result<(), Error<I2C::Error>> {
        let variant = self.verify()?;
        self.wait(delay, |s| Ok(s.read_u8(FIRMWARE_SYSTEM_STATUS)? == BOOTED))?;
        for (i, &byte) in DEFAULT_CONFIGURATION.iter().enumerate() {
            self.write_u8(CONFIG_START + i as u16, byte)?;
        }
        self.write_u8(SYSTEM_START, START_AUTONOMOUS)?;
        self.wait(delay, |s| s.data_ready())?;
        self.clear_interrupt()?;
        self.stop_ranging()?;
        self.write_u8(VHV_CONFIG_TIMEOUT_MACROP_LOOP_BOUND, 0x09)?;
        self.write_u8(0x000B, 0x00)?;
        self.write_u16(0x0024, 0x0500)?;
        self.set_range_timing(variant.default_budget_ms(), 0)
    }

    /// ST's `SetRangeTiming`: a timing budget of 10-200 ms, and either
    /// continuous ranging (`inter_measurement_ms` 0) or autonomous ranging
    /// with a period longer than the budget. Anything else is
    /// [`Error::OutOfRange`], as is a period too long for the register; an
    /// oscillator or PLL calibration reading of 0 is [`Error::DataError`].
    /// ST's driver checks neither of the last two.
    pub fn set_range_timing(
        &mut self,
        budget_ms: u32,
        inter_measurement_ms: u32,
    ) -> Result<(), Error<I2C::Error>> {
        if !(10..=200).contains(&budget_ms)
            || (inter_measurement_ms != 0 && inter_measurement_ms <= budget_ms)
        {
            return Err(Error::OutOfRange);
        }
        let osc = self.read_u16(OSC_FREQUENCY)?;
        if osc == 0 {
            return Err(Error::DataError);
        }
        let mut budget_us = budget_ms * 1000;
        if inter_measurement_ms == 0 {
            self.write_u32(INTERMEASUREMENT_MS, 0)?;
            budget_us -= 2500;
        } else {
            let clock_pll = self.read_u16(RESULT_OSC_CALIBRATE_VAL)? & 0x3FF;
            if clock_pll == 0 {
                return Err(Error::DataError);
            }
            // Left to right, as ST's C evaluates it.
            let period = 1.055_f32 * inter_measurement_ms as f32 * clock_pll as f32;
            if period >= 4_294_967_296.0 {
                return Err(Error::OutOfRange);
            }
            self.write_u32(INTERMEASUREMENT_MS, period as u32)?;
            budget_us = (budget_us - 4300) / 2;
        }
        let (a, b) = range_config(budget_us, macro_period_us(osc));
        self.write_u16(RANGE_CONFIG_A, a)?;
        self.write_u16(RANGE_CONFIG_B, b)
    }

    /// Start ranging, continuous or autonomous by the period last set. On
    /// the VL53L4CD this then waits for the first sample and discards it, as
    /// ST's driver does. Runs [`Vl53l4::verify`] first if the variant is not
    /// yet known.
    pub fn start_ranging(&mut self, delay: &mut impl DelayNs) -> Result<(), Error<I2C::Error>> {
        let variant = match self.variant {
            Some(v) => v,
            None => self.verify()?,
        };
        let start = if self.read_u32(INTERMEASUREMENT_MS)? == 0 {
            START_CONTINUOUS
        } else {
            START_AUTONOMOUS
        };
        self.write_u8(SYSTEM_START, start)?;
        if variant == Variant::Cd {
            self.wait(delay, |s| s.data_ready())?;
            self.clear_interrupt()?;
        }
        Ok(())
    }

    /// Stop ranging.
    pub fn stop_ranging(&mut self) -> Result<(), Error<I2C::Error>> {
        self.write_u8(SYSTEM_START, 0x00)
    }

    /// Whether a new result is ready: the interrupt line state against its
    /// configured polarity.
    pub fn data_ready(&mut self) -> Result<bool, Error<I2C::Error>> {
        let active_low = self.read_u8(GPIO_HV_MUX_CTRL)? & 0x10 != 0;
        let level = self.read_u8(GPIO_TIO_HV_STATUS)? & 0x01;
        Ok(level == if active_low { 0 } else { 1 })
    }

    /// Clear the result interrupt, letting the next result in.
    pub fn clear_interrupt(&mut self) -> Result<(), Error<I2C::Error>> {
        self.write_u8(SYSTEM_INTERRUPT_CLEAR, 0x01)
    }

    /// Read the result, then clear the interrupt for the next one. This does
    /// not wait; check [`Vl53l4::data_ready`] first. A result with no SPADs
    /// enabled has status 255 on both variants; ST's VL53L4CD driver does
    /// this and its VL53L4ED driver does not check.
    pub fn read(&mut self) -> Result<DistanceReading, Error<I2C::Error>> {
        let raw = self.read_u8(RESULT_RANGE_STATUS)? & 0x1F;
        let mut status = STATUS_MAP.get(raw as usize).copied().unwrap_or(raw);
        let spads = self.read_u16(RESULT_SPAD_NB)? / 256;
        if spads == 0 {
            status = 255;
        }
        let reading = DistanceReading {
            status,
            spads,
            signal_kcps: self.read_u16(RESULT_SIGNAL_RATE)? as u32 * 8,
            ambient_kcps: self.read_u16(RESULT_AMBIENT_RATE)? as u32 * 8,
            sigma_mm: self.read_u16(RESULT_SIGMA)? / 4,
            distance_mm: self.read_u16(RESULT_DISTANCE)?,
        };
        self.clear_interrupt()?;
        Ok(reading)
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    fn wait(
        &mut self,
        delay: &mut impl DelayNs,
        mut done: impl FnMut(&mut Self) -> Result<bool, Error<I2C::Error>>,
    ) -> Result<(), Error<I2C::Error>> {
        for _ in 0..WAIT_POLLS {
            if done(self)? {
                return Ok(());
            }
            delay.delay_ms(1);
        }
        Err(Error::Timeout)
    }

    fn read_into(&mut self, reg: u16, buf: &mut [u8]) -> Result<(), Error<I2C::Error>> {
        self.i2c
            .write_read(self.addr.get(), &reg.to_be_bytes(), buf)
            .map_err(Error::I2c)
    }

    fn read_u8(&mut self, reg: u16) -> Result<u8, Error<I2C::Error>> {
        let mut b = [0u8; 1];
        self.read_into(reg, &mut b)?;
        Ok(b[0])
    }

    fn read_u16(&mut self, reg: u16) -> Result<u16, Error<I2C::Error>> {
        let mut b = [0u8; 2];
        self.read_into(reg, &mut b)?;
        Ok(u16::from_be_bytes(b))
    }

    fn read_u32(&mut self, reg: u16) -> Result<u32, Error<I2C::Error>> {
        let mut b = [0u8; 4];
        self.read_into(reg, &mut b)?;
        Ok(u32::from_be_bytes(b))
    }

    fn write(&mut self, reg: u16, value: &[u8]) -> Result<(), Error<I2C::Error>> {
        let mut frame = [0u8; 6];
        frame[..2].copy_from_slice(&reg.to_be_bytes());
        frame[2..2 + value.len()].copy_from_slice(value);
        self.i2c
            .write(self.addr.get(), &frame[..2 + value.len()])
            .map_err(Error::I2c)
    }

    fn write_u8(&mut self, reg: u16, value: u8) -> Result<(), Error<I2C::Error>> {
        self.write(reg, &[value])
    }

    fn write_u16(&mut self, reg: u16, value: u16) -> Result<(), Error<I2C::Error>> {
        self.write(reg, &value.to_be_bytes())
    }

    fn write_u32(&mut self, reg: u16, value: u32) -> Result<(), Error<I2C::Error>> {
        self.write(reg, &value.to_be_bytes())
    }
}
