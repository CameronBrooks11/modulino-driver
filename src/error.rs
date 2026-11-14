//! Error types for Modulino operations.

use core::fmt;

use crate::spec::{ChipKind, Kind};

/// Error type for Modulino operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error<E> {
    /// I2C communication error
    I2c(E),
    /// Device not found on the I2C bus
    DeviceNotFound,
    /// Invalid address provided
    InvalidAddress,
    /// Invalid parameter value
    InvalidParameter,
    /// Value out of range
    OutOfRange,
    /// Operation timed out
    Timeout,
    /// The board sent a reply the protocol does not define, such as an
    /// LED matrix mode other than `MON` or `GS4`.
    DataError,
    /// A node answered with a strap byte that is not one of its kind's.
    WrongIdentity {
        /// The kind the caller addressed
        expected: Kind,
        /// The strap byte the board sent
        got: u8,
    },
    /// A frame length the node does not accept; nothing was sent.
    FrameLength {
        /// A length the node accepts
        expected: usize,
        /// The length given
        got: usize,
    },
    /// Something answered at a chip's address but failed its identity
    /// check.
    WrongChip {
        /// The chip expected there
        expected: ChipKind,
    },
    /// A data frame starts with a firmware command prefix (`CF` or `DIE`)
    /// and would be executed as that command; nothing was sent.
    ReservedPrefix,
}

impl<E> From<E> for Error<E> {
    fn from(e: E) -> Self {
        Error::I2c(e)
    }
}

impl<E: fmt::Debug> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::I2c(e) => write!(f, "I2C error: {:?}", e),
            Error::DeviceNotFound => write!(f, "Device not found on I2C bus"),
            Error::InvalidAddress => write!(f, "Invalid I2C address"),
            Error::InvalidParameter => write!(f, "Invalid parameter value"),
            Error::OutOfRange => write!(f, "Value out of range"),
            Error::Timeout => write!(f, "Operation timed out"),
            Error::DataError => write!(f, "reply not defined by the protocol"),
            Error::WrongIdentity { expected, got } => {
                write!(
                    f,
                    "expected a {:?} node, got strap byte {:#04x}",
                    expected, got
                )
            }
            Error::FrameLength { expected, got } => {
                write!(
                    f,
                    "frame length {} not accepted, expected {}",
                    got, expected
                )
            }
            Error::WrongChip { expected } => {
                write!(f, "the chip at this address is not a {:?}", expected)
            }
            Error::ReservedPrefix => write!(f, "frame starts with a firmware command prefix"),
        }
    }
}

/// Result type alias for Modulino operations.
pub type Result<T, E> = core::result::Result<T, Error<E>>;
