# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Renamed the package to `modulino-driver` (library `modulino_driver`),
  version 0.1.0-dev. The entries below are the history of modulino-rs,
  which this crate started from at 0.3.0.
- `Buttons` is rebuilt on the node core: `new` and `with_address` do no
  I/O and cannot fail, `verify()` checks the board's identity, and
  `read()` returns the state or an error. `new_with_address(i2c, u8)`
  becomes `with_address(i2c, Address)`, and `address()` returns
  `Address`. `ButtonState` moves to `codec::buttons`.
- `Error` is `#[non_exhaustive]`.
- `Buzzer`, `Vibro`, `Knob`, `Pixels`, `Joystick`, `OptoRelay` and
  `LatchRelay` are rebuilt on the node core with pure codecs, in the same
  shape as `Buttons`: `new`/`with_address(Address)` do no I/O,
  `verify()`, and reads return values or errors, never cached state.
  - `Buzzer::tone`/`play_note` and `Vibro::on`/`on_with_power` take `u32`
    durations (and `tone` a `u32` frequency). `play_note(Note::Rest, _)`
    sends the stop frame. A tone with
    duration 0 plays until the next call; `tone(0, _)`, which 0.3 sent as
    silence, is `OutOfRange` (use `no_tone`), as is anything above
    5 898 150 Hz. `Vibro::on` refuses a duration of 0, which the firmware
    stops at once.
  - `Knob::set_position` keeps the offset on the host and never writes
    the board; `read()` returns `KnobState`.
  - `Pixels::set`/`set_all` replace `set_color`/`set_all_color`, and the
    setters no longer return `&mut Self` for chaining.
  - `Joystick::read()` returns `JoystickState` centred on zero (raw 0 now
    gives 127 rather than overflowing); `read_raw()` gives the ADC values.
  - `OptoRelay::is_on()` and `LatchRelay::state()` (was `is_on`) read
    the board.
  - `LatchRelay::set`/`on`/`off` take a `DelayNs` and wait 120 ms after
    the command, even if the write reports an error (see Fixed).
  - `Buzzer::MIN_FREQUENCY`, `Vibro::DEFAULT_FREQUENCY` and
    `Joystick::DEFAULT_DEADZONE` move to `codec::buzzer::MIN_FREQUENCY`,
    `codec::vibro::FREQUENCY` and `codec::joystick::DEFAULT_DEADZONE`.
- `LedMatrix` and `Motors` are rebuilt on the node core the same way:
  `new`/`with_address(Address)` do no I/O and cannot fail.
  - `LedMatrix` tracks the board's mode: `verify()` and `set_mode()` read
    it from the board, `mode()` returns it, and the `show_*` calls trust
    it. It takes whole pictures: `show_mono(&[[bool; 12]; 8])` and
    `show_gray(&[[u8; 12]; 8])`. A picture whose frame would start `GS4`
    or `MON`, which the board takes as a mode switch, is refused, and a
    picture for the other mode is `FrameLength`. Mode commands are padded
    to the current mode's length.
    `codec::led_matrix::encode_mode_command(current, target)` takes the
    board's current mode and returns a `ModeCommand` of that mode's
    length. A codec caller can no longer build a mode command of a length
    no mode accepts; passing the board's real current mode is still the
    caller's job (`set_mode` reads it from the board).
  - `Motors` sends one frame per firmware command: `set_mode`,
    `set_speeds` (raw -32767..32767), `stop`, `set_dc_frequency`
    (200-60 000 Hz), `step` (period at least 1 tick, as 0.3 required),
    `hold`, `release_coils`, `set_decay`, `set_half_step`,
    `set_half_full_scale`; `status()` returns `MotorStatus`. Out-of-range
    values are `OutOfRange` (was `InvalidParameter`).
    `codec::motors::current_ma` converts currents, and `Motors::MAX_SPEED`
    becomes `codec::motors::MAX_DC_SPEED`.
- `Error::InvalidAddress` is now `Error::AddressInUse`: an address change
  refused because the target already answers.
- `Error::DataError` now means a reply the protocol does not define.
- `Movement` and `Pressure` are the new `chips::Lsm6dsox` and
  `chips::Lps22hb` drivers. `new` no longer does I/O or returns
  `Result`; `verify()` checks the chip's identity (`Error::WrongChip` if
  something else answers, where 0.3 returned `DeviceNotFound`); `init()`
  verifies, resets with a bounded poll (`Error::Timeout` if the reset
  bit never clears) and configures. `Movement::new` tries 0x6A and then
  0x6B in `verify()`; `with_address(Address)` fixes one, and `address()`
  returns `Address`. Axis readings are `Axes` (was `MovementValues`).
  LPS22HB pressure is sign-extended from 24 bits. `data_ready()` is
  split into `acceleration_ready`/`angular_velocity_ready` and
  `pressure_ready`/`temperature_ready`, and `Pressure::init` now resets
  the chip.
- `Thermo` finds its sensor: an HS3003 at 0x44 or, on some boards, an
  SHT31 at 0x45 (`verify()` returns which, `chip()` reports it).
  `read(&mut delay)` returns `ThermoReading` (was `ThermoMeasurement`;
  it no longer derives `Default` or converts from `hs3003::Measurement`),
  and `address()` returns `Option<Address>`, `None` until the sensor is
  found (was `u8`).
  The HS3003 path refuses a stale sample (status 01) and retries,
  ending in `Error::Timeout`; the SHT31 path retries while the sensor
  NACKs and returns `Error::DataError` on a bad CRC. The `hs3003`
  dependency is replaced by a small driver in `chips::hs3003`, because
  the crate ignored the stale-sample status bits.
- `Light` is the new `chips::Ltr381rgb` driver, trimmed to the lux/RGB
  path (design 4.5). `new_with_address(u8)` is `with_address(Address)`,
  `verify()` checks `PART_ID` (`WrongChip` where 0.3 returned
  `DeviceNotFound`), `init()` verifies and starts measuring with 0.3's
  defaults, and `ready()` reports a new sample (`MAIN_STATUS` bit 3). `read()` returns
  `LightReading` with all four channels from one 12-byte burst, masked
  to 20 bits; 0.3 read them in four transactions. `Gain`, `Resolution`
  and `Rate` (was `MeasurementRate`) live in `chips::ltr381rgb`, with
  shorter variant names, and `set_config` is `set_timing`.
  `Resolution::factor` is `time_factor`; `Resolution::max_value` is
  gone.
- `Distance` is the new `chips::Vl53l4` driver for the VL53L4CD and
  the VL53L4ED, following ST's drivers for each. `new` does no I/O;
  `verify()` reads the model ID and returns the variant (`WrongChip` for
  anything else); `init(&mut delay)` boots, loads ST's configuration,
  calibrates and sets the variant's default timing budget (50 or 100 ms,
  continuous). `set_range_timing(budget_ms, inter_measurement_ms)`
  replaces `set_timing_budget` and `set_inter_measurement`, computing the
  registers from the chip's oscillator as ST does, and refuses values ST
  refuses (`OutOfRange`), as well as a period too long for its register;
  an oscillator or PLL reading of 0 is `DataError`.
  `start_ranging(&mut delay)` starts continuous or autonomous ranging by
  the period set, finding the variant first if `verify` has not run.
  `read()` returns `DistanceReading` (distance, ST's range status,
  signal, ambient, sigma, SPADs) and replaces `read_distance` and
  `read_range_status`.
- `Hub` is a handle on a `&RefCell` bus and `Hub::port(n)` returns a
  `HubPort` that implements `I2c`: each transaction selects the port,
  runs the operations and deselects, even when they fail, so any device
  works behind a hub unchanged. A `HubPort` can carry a further hub.
  `Hub::new(i2c)` becomes `Hub::new(&RefCell)`, and
  `Hub::new_with_address(i2c, u8)` becomes
  `Hub::with_address(&RefCell, Address) -> Option<Hub>` (0x70-0x77).
  `Hub::select`/`clear`/`release`, `HubPort::new(&mut Hub, port)` and
  `addresses::HUB` are gone; `Hub::deselect` disconnects every port
  after a failed deselect. Single execution context only for now.

### Added

- `service`: `set_address`, `reset_address` and `general_call_reset`, the
  only code that sends the firmware's `CF` command. An address change pads
  `CF` to the board's current write length (48 bytes for a grayscale LED
  matrix), refuses a target address that already answers
  (`Error::AddressInUse`, including an idle SHT3x found by its status
  command), waits 500 ms for the reboot and confirms the board at its new
  address before moving the `Node`. `general_call_reset` sends `CF 0x00` to
  address 0 once per node write length, shortest first, so every kind takes
  a frame of its own length before any longer one; it sends every length and
  waits even after a bus error.
- `spec/nodes.toml` and its mirror `spec`: node protocol constants with
  firmware citations, checked against each other by a test.
- Every public data type, the `spec` types included, derives
  `defmt::Format` under the `defmt` feature; a test that CI compiles with
  `--features defmt` checks them.
- `Address`, `NewAddress`, `Strap`, `Kind` and `Node`, which checks the
  strap byte on every read, sends only exact write lengths and refuses
  frames starting `CF` or `DIE`.
- `Error::WrongIdentity`, `Error::FrameLength`, `Error::ReservedPrefix`.
- `scan` and `scan_except`: probe 0x01-0x77 with 1-byte reads and
  identify each answer by evidence (hub control register, chip identity
  check, node strap byte) as `Found::Node`, `Chip`, `Hub` or `Unknown`.
  `spec/chips.toml` and `spec::CHIPS` hold the chip identities, and
  `ChipKind` names them. A node moved to 0x45 is not taken for an SHT31,
  though its reply can pass the status CRC (a Knob strapped 0x74 at 1 in
  256 counts): when the status starts with a strap value, a further read
  that repeats it marks a node, and `Thermo::verify` returns `WrongChip`.
- `Error::WrongChip { expected, got }`, and the `chips` module.
  `Pressure::with_address` and `Pressure::verify`.
- `codec`: a pure encoder/decoder per node kind, with the protocol
  limits as constants (`buzzer::MIN_FREQUENCY`/`MAX_FREQUENCY`,
  `latch_relay::SETTLE_MS` and others), and `KnobState`, `KnobRaw`,
  `JoystickState`, `JoystickRaw`. `spec/vectors/` holds their vectors.

### Fixed

- `Vibro::off` sends the full 12-byte frame. 0.3 sent 8 bytes, which the
  firmware drops (main.c:548-549), so the motor ran on until its
  commanded duration ended. Read from the firmware source; not yet
  observed on hardware.
- A Buzzer frequency whose frame starts with `CF` or `DIE` (17987 Hz is
  `CF`) is refused instead of being executed by the firmware as an
  address change or bootloader jump.
- A latch relay command followed at once by a read, as 0.3's `off()`
  then `is_on()` did, could drive the wrong coil: the firmware re-reads
  the command byte through its ~100 ms handler, and any transfer to the
  board meanwhile overwrites it from the I2C interrupt (main.c:315-323,
  476, 571-580). `LatchRelay` now waits the handler out. Read from the
  firmware source; not yet observed on hardware.
- `Distance::init` returns `Error::Timeout` when the sensor never boots
  or never finishes calibrating; 0.3 gave up after 1001 polls and carried
  on. `start_ranging` on a VL53L4CD has the same bound.
- `Distance` timing budgets are computed from the oscillator reading;
  0.3 wrote fixed register values for a few budgets, used 20 ms for any
  other, and computed the inter-measurement period from an assumed
  64 kHz clock. `start_ranging` starts continuous ranging when the
  period is 0; 0.3 always started autonomous ranging. `init` no longer
  writes `0x0081` and `0x004B`, which neither ST's drivers nor
  arduino-modulino-mpy write.

### Removed

- `Error::DeviceNotFound` and `Error::InvalidParameter`, which nothing
  returns any more. An identity mismatch is `WrongChip` (was
  `DeviceNotFound`); out-of-range values are `OutOfRange`, a wrong LED
  matrix frame length `FrameLength`, and `Hub::port` returns `None` for a
  port above 7 (were `InvalidParameter`).
- `Color::to_apa102_data`. Nothing called it, and its `0xRRGGBB00` word
  is not what Pixels sends: `codec::pixels::encode` builds each LED as
  `[0xE0 | brightness, b, g, r]`.
- `ButtonLed`, the `Buttons` LED fields, `update_leds`, `all_leds_on`,
  `all_leds_off`, the cached getters, `Buttons::discover`,
  `addresses::BUTTONS` and `pinstrap::BUTTONS`.
- `Thermo::temperature`, `humidity` and `inner`, `Thermo::release`
  returning the `hs3003` driver (it returns the bus), `Hs3003Error`,
  `ThermoMeasurement::new`/`is_valid`, `addresses::THERMO`, and the
  `hs3003` dependency.
- `Light::new_with_address(u8)`, `enable`, `ir`, `raw_lux`, `lux`
  (use `read`), `LightMeasurement` (now `LightReading`, without
  `raw_lux`), `ColorName` and `color_name`; `addresses::LIGHT`.
- `Movement::new_with_address(u8)`, `Movement::acceleration_magnitude`
  and `Movement::gyro`; `MovementValues` with its `new`, `magnitude` and
  tuple conversions; `Pressure::reset`; `addresses::MOVEMENT`/`PRESSURE`;
  the `libm` dependency.
- `Distance::new_with_address(u8)` (now `with_address(Address)`),
  `set_timing_budget`, `set_inter_measurement`, `read_distance`,
  `read_range_status`; `I2cDevice`, and the `addresses` module, whose
  last entry was `DISTANCE`.
- `discover()` on every module (scan replaces it), the `pinstrap` module,
  and the `addresses` entries for the node kinds above.
- `DisplayMode` and the `LedMatrix` framebuffer API: `init`, `clear`,
  `set_pixel`, `set_grayscale_pixel`, `set_frame`, `show`, `buffer`;
  `new_with_address(u8)` (now `with_address(Address)`); `mode(&self)`
  (now `mode(&mut self) -> Result<MatrixMode, _>`).
- `Motors` helpers kept on the host in 0.3: percentage speeds
  (`set_speed_a`/`b`, `speed_a`/`b`), inversion, stepper direction
  inversion, `steps_per_revolution` and `move_stepper_rpm`, and the
  getters cached by `update`, replaced by `status()` (except
  `frequency()`, which the board does not report);
  `impl From<u8> for DecayMode`. Renamed:
  `set_dc_speed_raw` to `set_speeds`, `move_stepper` to `step`,
  `hold_coils` to `hold`, `set_frequency` to `set_dc_frequency`,
  `set_stepper_mode_enabled` to `set_mode`, `set_half_step_enabled` to
  `set_half_step`, `set_half_full_scale_enabled` to
  `set_half_full_scale`.
- `Buzzer::tone_continuous`/`stop`; `Vibro::set_frequency`/`frequency`/
  `on_continuous`/`stop`/`pulse`; `Knob::update`/`value`/`set_value`/
  `reset`/`pressed`/the range and direction helpers; the `Pixels`
  per-channel, range and brightness-only setters and `set_color_show`;
  `Knob::rotation_delta`; `Joystick::update` and its cached getters,
  `is_centered`, `magnitude` and `angle`;
  `toggle` and the cached getters on both relays, `OptoRelay::update`;
  `Pixels::clear_range`; `PowerLevel::value` and `From<PowerLevel> for
  u8` (use `as u8`).

## [0.3.0] - 2026-07-02

Upstream released 0.3.0 without a changelog entry; this one is written
from the release commit (87e091a).

### Added
- Driver for **Modulino Motors** (`Motors`), experimental.

### Changed
- Joystick axes inverted: `x` and `y` are now `128 - raw` (were `raw - 128`).
- Hub docs no longer mark the driver experimental.

## [0.2.0] - 2026-05-24

### Added
- Native, optimized internal driver for **Modulino LED Matrix** (`LedMatrix`).
- Native internal driver for **Modulino Light** (`Light`) sensor with 1:1 parity with Arduino C++ implementation.
- Native internal driver for **Modulino Pressure** (`Pressure`) sensor (untested/experimental).
- Native internal driver for **Modulino Opto Relay** (`OptoRelay`) module (untested/experimental).
- Native internal driver for **Modulino Hub** (`Hub` / I2C multiplexer) for TCA9548A (untested/experimental).
- Examples and extensive unit/mock tests for the new drivers.

### Fixed
- Fixed redundant display buffer zeroing loops in the LED Matrix driver to optimize rendering performance.
- Resolved various Clippy lint warnings to improve code cleanliness.
- Modernized Continuous Integration (CI) workflows to use Node 24 and eliminate deprecation warnings.

## [0.1.0] - 2026-02-01

### Added

- Initial release
- Support for Modulino Buttons (three buttons with LEDs)
- Support for Modulino Buzzer (piezo speaker with musical notes)
- Support for Modulino Pixels (8 RGB LEDs)
- Support for Modulino Distance (VL53L4CD ToF sensor)
- Support for Modulino Movement (LSM6DSOX IMU)
- Support for Modulino Knob (rotary encoder with button)
- Support for Modulino Thermo (wraps `hs3003` crate for HS3003 sensor)
- Support for Modulino Joystick (analog joystick with button)
- Support for Modulino Latch Relay (latching relay)
- Support for Modulino Vibro (vibration motor)
- `defmt` feature for embedded debugging support
- Comprehensive documentation and examples

[Unreleased]: https://github.com/CameronBrooks11/modulino-driver/commits/main
[0.3.0]: https://github.com/melastmohican/modulino-rs/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/melastmohican/modulino-rs/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/melastmohican/modulino-rs/releases/tag/v0.1.0
