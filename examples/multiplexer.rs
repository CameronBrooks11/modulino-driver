//! Two identical Buttons modules on two ports of a Modulino Hub (TCA9548A).
//!
//! Both answer at 0x3E, so they cannot share one bus segment. Each gets a
//! `HubPort`, which selects its port, runs the transaction and deselects
//! again, so the drivers need no hub code of their own.
//!
//! Replace `DummyI2c` with your HAL's I2C bus.

use core::cell::RefCell;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};
use modulino_driver::{Buttons, Hub};

fn main() {
    let bus = RefCell::new(DummyI2c);
    let hub = Hub::new(&bus);

    // Constructors do no I/O, and each port is only a handle on the bus.
    let mut left = Buttons::new(hub.port(0).expect("port 0 exists"));
    let mut right = Buttons::new(hub.port(1).expect("port 1 exists"));

    for _ in 0..3 {
        if let Ok(state) = left.read() {
            let _ = left.set_leds(state.a, state.b, state.c);
        }
        if let Ok(state) = right.read() {
            let _ = right.set_leds(state.a, state.b, state.c);
        }
    }
}

/// Stands in for a HAL bus: every read returns the Buttons strap byte and
/// nothing pressed; every write succeeds.
struct DummyI2c;

#[derive(Debug, Clone, Copy)]
struct DummyError;

impl embedded_hal::i2c::Error for DummyError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Other
    }
}

impl ErrorType for DummyI2c {
    type Error = DummyError;
}

impl I2c for DummyI2c {
    fn transaction(
        &mut self,
        _address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        for op in operations {
            if let Operation::Read(buf) = op {
                buf.fill(0);
                if let Some(first) = buf.first_mut() {
                    *first = 0x7C;
                }
            }
        }
        Ok(())
    }
}
