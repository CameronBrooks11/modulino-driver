//! Pure encoders and decoders for node frames: values to bytes and back,
//! with no bus, time or state. `spec/vectors/` holds their test vectors.

pub mod buttons;
pub mod buzzer;
pub mod joystick;
pub mod knob;
pub mod latch_relay;
pub mod opto_relay;
pub mod pixels;
pub mod vibro;

/// A value outside what the firmware can act on; nothing was encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct OutOfRange;

impl OutOfRange {
    pub(crate) fn into_error<E>(self) -> crate::Error<E> {
        crate::Error::OutOfRange
    }
}
