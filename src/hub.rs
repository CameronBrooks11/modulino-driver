//! Hub routing: the Modulino Hub's ports as I2C buses.
//!
//! The Modulino Hub is a TCA9548A multiplexer at 0x70-0x77 (A0-A2 straps).
//! Its single control byte has bit `n` set while port `n` is connected to
//! the upstream bus. [`HubPort`] implements [`I2c`], so every device in this
//! crate works behind a hub with no code of its own: each transaction
//! selects the port, runs the device's operations and deselects again, even
//! when the operations fail. Nothing is cached, because anything else on the
//! bus may have changed the control byte.
//!
//! The port borrows the whole upstream bus for the length of a transaction,
//! through a [`RefCell`]. That makes select, operations and deselect one
//! unit in a single execution context; sharing the bus between tasks or
//! interrupts needs a lock and is not supported yet. Devices on the upstream
//! bus itself can share the same `RefCell`, for example through
//! `embedded_hal_bus::i2c::RefCellDevice`.
//!
//! Two consequences of the hardware to keep in mind:
//!
//! - The hub stays addressable while a port is connected, so a transaction
//!   through a port to the hub's own address (or to any hub above it)
//!   reaches that hub. Do not put a device or a nested hub at the address of
//!   a hub above it.
//! - If the deselect fails after the operations succeeded, the operations
//!   took effect but the error is returned, and the port stays connected
//!   until the next transaction through this hub or a call to
//!   [`Hub::deselect`]. Retrying repeats a write that already happened.

use core::cell::RefCell;

use embedded_hal::i2c::{ErrorType, I2c, Operation};

use crate::addr::Address;

/// The lowest hub address (A0-A2 low).
pub(crate) const HUB_MIN_ADDRESS: u8 = 0x70;
/// The highest hub address (A0-A2 high).
pub(crate) const HUB_MAX_ADDRESS: u8 = 0x77;
/// Ports per hub.
pub(crate) const PORTS: u8 = 8;

/// A Modulino Hub on a shared bus.
///
/// ```rust,ignore
/// use core::cell::RefCell;
/// use modulino_driver::{Buttons, Hub};
///
/// let bus = RefCell::new(i2c);
/// let hub = Hub::new(&bus);
/// let mut left = Buttons::new(hub.port(0).unwrap());
/// let mut right = Buttons::new(hub.port(1).unwrap());
/// left.set_leds(true, false, false)?;
/// right.set_leds(false, false, true)?;
/// ```
#[derive(Debug)]
pub struct Hub<'a, B> {
    bus: &'a RefCell<B>,
    addr: Address,
}

impl<B> Clone for Hub<'_, B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B> Copy for Hub<'_, B> {}

impl<'a, B: I2c> Hub<'a, B> {
    /// The hub at its default address, 0x70. Does no I/O.
    pub fn new(bus: &'a RefCell<B>) -> Self {
        Self {
            bus,
            addr: Address::new(HUB_MIN_ADDRESS).expect("0x70 is a device address"),
        }
    }

    /// The hub at `addr`, which must be 0x70-0x77. Does no I/O.
    pub fn with_address(bus: &'a RefCell<B>, addr: Address) -> Option<Self> {
        (HUB_MIN_ADDRESS..=HUB_MAX_ADDRESS)
            .contains(&addr.get())
            .then_some(Self { bus, addr })
    }

    /// The hub's address.
    pub fn address(&self) -> Address {
        self.addr
    }

    /// Disconnect every port: write 0 to the control byte. Each
    /// [`HubPort`] transaction already ends this way; this is for recovering
    /// after a failed deselect.
    pub fn deselect(&self) -> Result<(), B::Error> {
        self.bus.borrow_mut().write(self.addr.get(), &[0])
    }

    /// Port `port` (0-7) as a bus. Does no I/O.
    pub fn port(&self, port: u8) -> Option<HubPort<'a, B>> {
        (port < PORTS).then(|| HubPort {
            hub: *self,
            mask: 1 << port,
        })
    }
}

/// One port of a [`Hub`], usable anywhere an [`I2c`] bus is.
#[derive(Debug)]
pub struct HubPort<'a, B> {
    hub: Hub<'a, B>,
    mask: u8,
}

impl<B> Clone for HubPort<'_, B> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<B> Copy for HubPort<'_, B> {}

impl<B: I2c> HubPort<'_, B> {
    /// The port number, 0-7.
    pub fn port(&self) -> u8 {
        self.mask.trailing_zeros() as u8
    }

    /// The hub this port belongs to.
    pub fn hub_address(&self) -> Address {
        self.hub.addr
    }
}

impl<B: I2c> ErrorType for HubPort<'_, B> {
    type Error = B::Error;
}

impl<B: I2c> I2c for HubPort<'_, B> {
    /// Select the port, run `operations`, deselect. The deselect runs even
    /// if the operations fail; their error is returned first.
    ///
    /// Panics if the bus is already borrowed, which happens only if the same
    /// `RefCell` is used re-entrantly in one context.
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        let mut bus = self.hub.bus.borrow_mut();
        let hub = self.hub.addr.get();
        if let Err(e) = bus.write(hub, &[self.mask]) {
            // The hub may have latched the byte before the error; disconnect
            // on a best-effort basis and report the select error.
            let _ = bus.write(hub, &[0]);
            return Err(e);
        }
        let result = bus.transaction(address, operations);
        let deselect = bus.write(hub, &[0]);
        result.and(deselect)
    }
}
