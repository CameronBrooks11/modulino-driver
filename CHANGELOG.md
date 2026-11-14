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
  - `Motors` sends one frame per firmware command: `set_mode`,
    `set_speeds` (raw -32767..32767), `stop`, `set_dc_frequency`
    (200-60 000 Hz), `step` (period at least 1 tick, as 0.3 required),
    `hold`, `release_coils`, `set_decay`, `set_half_step`,
    `set_half_full_scale`; `status()` returns `MotorStatus`. Out-of-range
    values are `OutOfRange` (was `InvalidParameter`).
    `codec::motors::current_ma` converts currents, and `Motors::MAX_SPEED`
    becomes `codec::motors::MAX_DC_SPEED`.
- `Error::DataError` now means a reply the protocol does not define.
- `Hub` is a handle on a `&RefCell` bus and `Hub::port(n)` returns a
  `HubPort` that implements `I2c`: each transaction selects the port,
  runs the operations and deselects, even when they fail, so any device
  works behind a hub unchanged. A `HubPort` can carry a further hub.
  `Hub::select`/`clear`/`release`, `HubPort::new(&mut Hub, port)` and
  `addresses::HUB` are gone. Single execution context only for now.

### Added

- `spec/nodes.toml` and its mirror `spec`: node protocol constants with
  firmware citations, checked against each other by a test.
- `Address`, `NewAddress`, `Strap`, `Kind` and `Node`, which checks the
  strap byte on every read, sends only exact write lengths and refuses
  frames starting `CF` or `DIE`.
- `Error::WrongIdentity`, `Error::FrameLength`, `Error::ReservedPrefix`.
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

### Removed

- `ButtonLed`, the `Buttons` LED fields, `update_leds`, `all_leds_on`,
  `all_leds_off`, the cached getters, `Buttons::discover`,
  `addresses::BUTTONS` and `pinstrap::BUTTONS`.
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
