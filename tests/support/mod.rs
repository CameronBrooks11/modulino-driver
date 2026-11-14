//! A simulated I2C bus of Modulino node boards.
//!
//! `FakeNode` follows the node firmware's rules (node_modulino_firmware@7d578e0,
//! Core/Src/main.c): a read returns the strap byte and then the payload; a
//! write of any length other than the board's receive length is dropped with
//! no error on the bus (531-561); a write starting `CF` moves the board to a
//! new address and `DIE` sends it to the ROM bootloader at 0x64 (235-254).

#![allow(dead_code)]

use embedded_hal::i2c::{self, ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};

/// The address the STM32C0 ROM bootloader answers at.
pub const BOOTLOADER: u8 = 0x64;

/// One simulated node board.
#[derive(Debug, Clone)]
pub struct FakeNode {
    /// Current 7-bit address.
    pub addr: u8,
    /// Strap byte, sent first on every read.
    pub strap: u8,
    /// Bytes sent after the strap byte.
    pub payload: Vec<u8>,
    /// The only write length the board processes.
    pub write_len: usize,
    /// Every frame the board processed, oldest first.
    pub frames: Vec<Vec<u8>>,
    /// Writes the board dropped for their length.
    pub dropped: usize,
    /// Set by a `CF` command.
    pub address_changes: usize,
}

impl FakeNode {
    /// A board at its strap default address.
    pub fn new(strap: u8, payload: &[u8], write_len: usize) -> Self {
        Self {
            addr: strap >> 1,
            strap,
            payload: payload.to_vec(),
            write_len,
            frames: Vec::new(),
            dropped: 0,
            address_changes: 0,
        }
    }

    /// The same board at another address.
    pub fn at(mut self, addr: u8) -> Self {
        self.addr = addr;
        self
    }

    fn read(&self, buf: &mut [u8]) {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = match i {
                0 => self.strap,
                _ => self.payload.get(i - 1).copied().unwrap_or(0xFF),
            };
        }
    }

    fn write(&mut self, data: &[u8]) {
        if data.len() != self.write_len {
            self.dropped += 1;
            return;
        }
        if data.starts_with(b"DIE") {
            self.addr = BOOTLOADER;
            return;
        }
        if data.starts_with(b"CF") {
            // main.c:163-167: 0x00 or 0xFF restores the strap default.
            let new8 = if data[2] == 0 || data[2] == 0xFF {
                self.strap
            } else {
                data[2]
            };
            self.addr = new8 >> 1;
            self.address_changes += 1;
            return;
        }
        self.frames.push(data.to_vec());
    }
}

/// Bus error: nothing acknowledged the address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nack(pub u8);

impl i2c::Error for Nack {
    fn kind(&self) -> ErrorKind {
        ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address)
    }
}

/// A bus with node boards on it.
#[derive(Debug, Default)]
pub struct FakeBus {
    /// Boards on the bus.
    pub nodes: Vec<FakeNode>,
}

impl FakeBus {
    /// A bus with these boards.
    pub fn new(nodes: Vec<FakeNode>) -> Self {
        Self { nodes }
    }

    /// The board currently at `addr`.
    pub fn node(&self, addr: u8) -> &FakeNode {
        self.nodes
            .iter()
            .find(|n| n.addr == addr)
            .unwrap_or_else(|| panic!("no node at {addr:#04x}"))
    }
}

impl ErrorType for FakeBus {
    type Error = Nack;
}

impl I2c for FakeBus {
    fn transaction(&mut self, address: u8, ops: &mut [Operation<'_>]) -> Result<(), Nack> {
        if address == 0 {
            // General call: every node processes writes; nothing reads.
            for op in ops.iter_mut() {
                match op {
                    Operation::Write(data) => self.nodes.iter_mut().for_each(|n| n.write(data)),
                    Operation::Read(_) => return Err(Nack(0)),
                }
            }
            return Ok(());
        }
        let node = self
            .nodes
            .iter_mut()
            .find(|n| n.addr == address)
            .ok_or(Nack(address))?;
        for op in ops.iter_mut() {
            match op {
                Operation::Read(buf) => node.read(buf),
                Operation::Write(data) => node.write(data),
            }
        }
        Ok(())
    }
}
