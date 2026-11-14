//! Bus addresses and node identity.
//!
//! Addresses in the public API are always 7-bit [`Address`] values. The
//! firmware's 8-bit strap byte is a [`Strap`], and the one conversion
//! between them is [`Strap::default_address`]. A `Strap` comes only from a
//! node read, so it cannot be built from an arbitrary byte.

use crate::spec::{Kind, NODES};

/// A 7-bit I2C address in `0x01..=0x77`.
///
/// The range includes 0x02 because the latch relay ships there. 0x00 is the
/// general call address and 0x78-0x7F are reserved, so neither is a device
/// address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Address(u8);

impl Address {
    /// `Some` if `addr` is in `0x01..=0x77`.
    pub const fn new(addr: u8) -> Option<Self> {
        match addr {
            0x01..=0x77 => Some(Self(addr)),
            _ => None,
        }
    }

    /// The 7-bit address.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// The target of a node address change, in `0x08..=0x77`.
///
/// This is the range Arduino's address tools accept. It converts to
/// [`Address`]; an `Address` does not convert back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NewAddress(u8);

impl NewAddress {
    /// `Some` if `addr` is in `0x08..=0x77`.
    pub const fn new(addr: u8) -> Option<Self> {
        match addr {
            0x08..=0x77 => Some(Self(addr)),
            _ => None,
        }
    }

    /// The 7-bit address.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl From<NewAddress> for Address {
    fn from(a: NewAddress) -> Self {
        Address(a.0)
    }
}

/// A node's 8-bit strap byte: the first byte of every node read, and the
/// board's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Strap(u8);

impl Strap {
    pub(crate) const fn from_read(byte: u8) -> Self {
        Self(byte)
    }

    /// The raw strap byte.
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The node kind that reports this strap byte, if any.
    pub fn kind(self) -> Option<Kind> {
        NODES
            .iter()
            .find(|n| n.straps.contains(&self.0))
            .map(|n| n.kind)
    }

    /// The address a board with this strap answers at before any address
    /// change: the strap byte shifted right by one. `None` when that is not
    /// a device address, as for a strap byte of 0x00 or above 0xEF.
    pub const fn default_address(self) -> Option<Address> {
        Address::new(self.0 >> 1)
    }
}

impl Kind {
    /// The kind's primary default address.
    pub const fn default_address(self) -> Address {
        match Strap(self.spec().straps[0]).default_address() {
            Some(a) => a,
            None => panic!("spec strap outside the address range"),
        }
    }

    /// Every default address of the kind, primary first.
    pub fn default_addresses(self) -> impl Iterator<Item = Address> {
        self.spec()
            .straps
            .iter()
            .filter_map(|&s| Strap(s).default_address())
    }

    /// Whether `strap` is one this kind reports.
    pub fn has_strap(self, strap: u8) -> bool {
        self.spec().straps.contains(&strap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_range() {
        assert_eq!(Address::new(0x00), None);
        assert_eq!(Address::new(0x01).map(Address::get), Some(0x01));
        assert_eq!(Address::new(0x77).map(Address::get), Some(0x77));
        assert_eq!(Address::new(0x78), None);
    }

    #[test]
    fn new_address_range() {
        assert_eq!(NewAddress::new(0x07), None);
        assert_eq!(NewAddress::new(0x08).map(NewAddress::get), Some(0x08));
        assert_eq!(NewAddress::new(0x77).map(NewAddress::get), Some(0x77));
        assert_eq!(NewAddress::new(0x78), None);
    }

    #[test]
    fn default_addresses_are_strap_shifted() {
        assert_eq!(Kind::Buttons.default_address().get(), 0x3E);
        assert_eq!(Kind::LatchRelay.default_address().get(), 0x02);
        let knob: [u8; 2] = {
            let mut it = Kind::Knob.default_addresses().map(Address::get);
            [it.next().unwrap(), it.next().unwrap()]
        };
        assert_eq!(knob, [0x3A, 0x3B]);
    }

    #[test]
    fn every_default_address_is_valid() {
        for n in NODES {
            for &s in n.straps {
                assert_eq!(s & 1, 0, "{} strap {:#04x} is odd", n.name, s);
                assert!(Address::new(s >> 1).is_some());
            }
        }
    }

    #[test]
    fn strap_lookup() {
        assert_eq!(Strap(0x7C).kind(), Some(Kind::Buttons));
        assert_eq!(Strap(0x74).kind(), Some(Kind::Knob));
        assert_eq!(Strap(0x00).kind(), None);
    }

    #[test]
    fn strap_outside_address_range_has_no_default_address() {
        assert_eq!(Strap(0x00).default_address(), None);
        assert_eq!(Strap(0x01).default_address(), None);
        assert_eq!(Strap(0xF0).default_address(), None);
        assert_eq!(Strap(0x02).default_address().map(Address::get), Some(0x01));
        assert_eq!(Strap(0xEE).default_address().map(Address::get), Some(0x77));
    }
}
