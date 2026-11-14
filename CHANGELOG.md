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
  `read()` returns the state or an error. `ButtonState` moves to
  `codec::buttons`.

### Added

- `spec/nodes.toml` and its mirror `spec`: node protocol constants with
  firmware citations, checked against each other by a test.
- `Address`, `NewAddress`, `Strap`, `Kind` and `Node`, which checks the
  strap byte on every read, sends only exact write lengths and refuses
  frames starting `CF` or `DIE`.
- `Error::WrongIdentity`, `Error::FrameLength`, `Error::ReservedPrefix`.

### Removed

- `ButtonLed`, the `Buttons` LED fields and cached getters,
  `Buttons::discover`, `addresses::BUTTONS` and `pinstrap::BUTTONS`.

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
