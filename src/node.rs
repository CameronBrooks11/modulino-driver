//! Node framing: the one place that knows the node protocol.
//!
//! A read returns the strap byte and then the payload, with no register
//! byte. A write must be exactly a length the board accepts, or the firmware
//! drops it without telling the host. The firmware checks the start of every
//! write for its `CF` (address change) and `DIE` (bootloader) commands, so a
//! data frame that starts that way is refused here rather than executed.

use embedded_hal::i2c::I2c;

use crate::addr::{Address, Strap};
use crate::spec::{Kind, WriteLen, MAX_READ_PAYLOAD};
use crate::Error;

/// A node board at an address, expected to be of one kind.
#[derive(Debug)]
pub struct Node<I2C> {
    i2c: I2C,
    addr: Address,
    kind: Kind,
}

impl<I2C: I2c> Node<I2C> {
    /// A node of `kind` at `addr`. Does no I/O.
    pub fn new(i2c: I2C, addr: Address, kind: Kind) -> Self {
        Self { i2c, addr, kind }
    }

    /// The node's address.
    pub fn address(&self) -> Address {
        self.addr
    }

    /// The kind this node is expected to be.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Read the strap byte alone.
    pub fn identify(&mut self) -> Result<Strap, Error<I2C::Error>> {
        let mut buf = [0u8; 1];
        self.i2c
            .read(self.addr.get(), &mut buf)
            .map_err(Error::I2c)?;
        Ok(Strap(buf[0]))
    }

    /// Check that the board at this address reports this node's kind.
    pub fn verify(&mut self) -> Result<(), Error<I2C::Error>> {
        let strap = self.identify()?;
        self.check(strap.0)
    }

    /// Read `P` payload bytes, after checking the strap byte that precedes
    /// them.
    ///
    /// `P` may be shorter than the kind's payload but not longer.
    pub fn read<const P: usize>(&mut self) -> Result<[u8; P], Error<I2C::Error>> {
        let max = self.kind.spec().read_payload;
        if P > max {
            return Err(Error::FrameLength {
                expected: max,
                got: P,
            });
        }
        let mut buf = [0u8; 1 + MAX_READ_PAYLOAD];
        let buf = &mut buf[..1 + P];
        self.i2c.read(self.addr.get(), buf).map_err(Error::I2c)?;
        self.check(buf[0])?;
        let mut payload = [0u8; P];
        payload.copy_from_slice(&buf[1..]);
        Ok(payload)
    }

    /// Write one frame.
    ///
    /// Refused without I/O if `W` is not a length the kind accepts, or if the
    /// frame starts with a firmware command prefix.
    pub fn write<const W: usize>(&mut self, frame: &[u8; W]) -> Result<(), Error<I2C::Error>> {
        let accepted = self.kind.spec().write_len;
        if !accepted.accepts(W) {
            let expected = match accepted {
                WriteLen::Fixed(n) => n,
                WriteLen::Mode { mono, .. } => mono,
            };
            return Err(Error::FrameLength { expected, got: W });
        }
        if has_command_prefix(frame) {
            return Err(Error::ReservedPrefix);
        }
        self.i2c.write(self.addr.get(), frame).map_err(Error::I2c)
    }

    /// Give back the bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    fn check(&self, strap: u8) -> Result<(), Error<I2C::Error>> {
        if self.kind.has_strap(strap) {
            Ok(())
        } else {
            Err(Error::WrongIdentity {
                expected: self.kind,
                got: strap,
            })
        }
    }
}

/// Whether the firmware would execute `frame` as a command.
fn has_command_prefix(frame: &[u8]) -> bool {
    frame.starts_with(b"CF") || frame.starts_with(b"DIE")
}

#[cfg(test)]
mod tests {
    use super::has_command_prefix;

    #[test]
    fn command_prefixes() {
        assert!(has_command_prefix(b"CF\x10"));
        assert!(has_command_prefix(b"DIE"));
        assert!(!has_command_prefix(b"DI\x00"));
        assert!(!has_command_prefix(b"C\x00F"));
        assert!(!has_command_prefix(&[0x01, 0x00, 0x01]));
    }
}
