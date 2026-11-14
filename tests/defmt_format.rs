//! Every public data type implements `defmt::Format` under the `defmt`
//! feature. Compiled by `cargo check --all-targets --features defmt` and
//! by `just lint`; without the feature the file is empty.
#![cfg(feature = "defmt")]

use modulino_driver::chips::{hs3003, lsm6dsox, ltr381rgb, vl53l4};
use modulino_driver::codec::{
    buttons, joystick, knob, led_matrix, motors, pixels, vibro, OutOfRange,
};
use modulino_driver::spec::{ChipId, ChipKind, ChipSpec, Kind, NodeSpec, WriteLen};
use modulino_driver::{Address, Color, Error, Found, KnobState, NewAddress, Note, Strap};

fn format<T: defmt::Format>() {}

#[test]
fn every_public_data_type_is_format() {
    format::<Error<u8>>();
    format::<Found>();
    format::<Address>();
    format::<NewAddress>();
    format::<Strap>();
    format::<Color>();
    format::<Kind>();
    format::<WriteLen>();
    format::<NodeSpec>();
    format::<ChipKind>();
    format::<ChipId>();
    format::<ChipSpec>();
    format::<OutOfRange>();
    format::<buttons::ButtonState>();
    format::<joystick::JoystickRaw>();
    format::<joystick::JoystickState>();
    format::<knob::KnobRaw>();
    format::<KnobState>();
    format::<led_matrix::Mode>();
    format::<motors::Mode>();
    format::<motors::DecayMode>();
    format::<motors::MotorStatus>();
    format::<pixels::Led>();
    format::<vibro::PowerLevel>();
    format::<Note>();
    format::<hs3003::ThermoReading>();
    format::<lsm6dsox::Axes>();
    format::<ltr381rgb::Gain>();
    format::<ltr381rgb::Resolution>();
    format::<ltr381rgb::Rate>();
    format::<ltr381rgb::LightReading>();
    format::<vl53l4::Variant>();
    format::<vl53l4::DistanceReading>();
}
