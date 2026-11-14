//! # modulino-driver
//!
//! A layered, hardware-agnostic `no_std` driver for Arduino Modulino boards.
//!
//! This crate provides drivers for the following Modulino devices:
//!
//! - [`Buttons`] - Three-button module with LEDs
//! - [`Buzzer`] - Piezo speaker module
//! - [`Pixels`] - 8 RGB LED module (APA102-compatible)
//! - [`Distance`] - Time-of-Flight distance sensor (VL53L4CD; the VL53L4ED variant is not yet supported)
//! - [`Movement`] - IMU module (LSM6DSOX accelerometer/gyroscope)
//! - [`Knob`] - Rotary encoder with button
//! - [`Thermo`] - Temperature and humidity sensor (HS3003, wraps the [`hs3003`](https://crates.io/crates/hs3003) crate;
//!   the SHT31 variant is not yet supported)
//! - [`Joystick`] - Analog joystick with button
//! - [`LatchRelay`] - Latching relay module
//! - [`Vibro`] - Vibration motor module
//! - [`LedMatrix`] - 12x8 LED matrix display
//! - [`Pressure`] - Barometric pressure and temperature sensor
//! - [`Light`] - RGB and ambient light sensor
//! - [`OptoRelay`] - Solid-state relay module
//! - [`Hub`] - TCA9548A I2C multiplexer; each [`HubPort`] is an I2C bus
//! - [`Motors`] - Dual DC or stepper motor driver (experimental)
//!
//! ## Example
//!
//! ```rust,ignore
//! use modulino_driver::{Pixels, Color};
//!
//! // Constructors do no I/O; verify() checks the board's identity
//! let mut pixels = Pixels::new(i2c);
//! pixels.verify()?;
//!
//! // Set the first LED to red at 50% brightness
//! pixels.set(0, Color::RED, 50)?;
//!
//! // Apply the changes
//! pixels.show()?;
//! ```
//!
//! ## Features
//!
//! - `defmt`: Enable `defmt` debug formatting for error types
//!
//! ## Hardware Requirements
//!
//! All Modulino devices communicate over I2C at 100kHz. They use the Qwiic/STEMMA QT
//! connector standard for easy daisy-chaining.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![deny(unsafe_code)]

mod addr;
pub mod chips;
pub mod codec;
mod color;
mod devices;
mod distance;
mod error;
mod hub;
mod i2c_device;
mod node;
pub mod scan;
pub mod spec;
mod thermo;

pub use addr::{Address, NewAddress, Strap};
pub use chips::lps22hb::Lps22hb;
pub use chips::lsm6dsox::{Axes, Lsm6dsox};
pub use chips::ltr381rgb::{LightReading, Ltr381rgb};
pub use codec::buttons::ButtonState;
pub use codec::joystick::{JoystickRaw, JoystickState};
pub use codec::knob::KnobRaw;
pub use codec::led_matrix::Mode as MatrixMode;
pub use codec::motors::{DecayMode, Mode as MotorMode, MotorStatus};
pub use codec::vibro::PowerLevel;
pub use color::Color;
pub use devices::buttons::Buttons;
pub use devices::buzzer::{Buzzer, Note};
pub use devices::joystick::Joystick;
pub use devices::knob::{Knob, KnobState};
pub use devices::latch_relay::LatchRelay;
pub use devices::led_matrix::LedMatrix;
pub use devices::motors::Motors;
pub use devices::opto_relay::OptoRelay;
pub use devices::pixels::Pixels;
pub use devices::vibro::Vibro;
pub use distance::Distance;
pub use error::{Error, Result};
pub use hub::{Hub, HubPort};
pub use i2c_device::I2cDevice;
pub use node::Node;
pub use scan::{scan, scan_except, Found};
pub use spec::{ChipKind, Kind};
pub use thermo::{Hs3003Error, Thermo, ThermoMeasurement};

/// Default I2C addresses for Modulino devices.
///
/// These are the factory-default 7-bit I2C addresses. Some modules support
/// multiple addresses via hardware configuration (solder jumpers).
pub mod addresses {
    /// Default address for Modulino Distance
    pub const DISTANCE: u8 = 0x29;

    /// Default address for Modulino Thermo
    pub const THERMO: u8 = 0x44;
}

/// Modulino Movement: an [`Lsm6dsox`] at 0x6A or 0x6B.
pub type Movement<I2C> = Lsm6dsox<I2C>;

/// Modulino Pressure: an [`Lps22hb`] at 0x5C.
pub type Pressure<I2C> = Lps22hb<I2C>;

/// Modulino Light: an [`Ltr381rgb`] at 0x53.
pub type Light<I2C> = Ltr381rgb<I2C>;
