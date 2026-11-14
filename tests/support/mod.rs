//! A simulated I2C bus of Modulino node boards.
//!
//! `FakeNode` follows the node firmware (node_modulino_firmware@7d578e0):
//! - a read returns the strap byte and then the payload (main.c:471-529);
//! - the firmware arms a receive of exactly the board's length
//!   (main.c:531-561), and the STM32C0 HAL completes it when that many bytes
//!   have arrived (stm32c0xx_hal_i2c.c, I2C_Slave_ISR_IT): a shorter write
//!   is dropped with no error on the bus, and a longer one has its first
//!   bytes processed. The fake then accepts the rest; the real board may
//!   instead stretch the clock or NACK, which is unverified;
//! - a processed frame starting `CF` moves the board to a new address and
//!   one starting `DIE` sends it to the ROM bootloader (main.c:235-254), which
//!   answers at 0x64 (Arduino_Modulino FirmwareUpdater.ino:63-64).
//!
//! What the board does with the bytes past its length is not modelled.

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
    /// Payloads for the next reads, oldest first; each read takes one and
    /// makes it the new `payload`.
    pub queued: std::collections::VecDeque<Vec<u8>>,
    /// The only write length the board processes.
    pub write_len: usize,
    /// Every frame the board processed, oldest first.
    pub frames: Vec<Vec<u8>>,
    /// Writes the board dropped for their length.
    pub dropped: usize,
    /// Set by a `CF` command.
    pub address_changes: usize,
    /// `Some(grayscale)` for an LED matrix, whose write length and read
    /// payload follow its mode (main.c:324-338, 506-516, 555-556).
    pub matrix_grayscale: Option<bool>,
}

impl FakeNode {
    /// An LED matrix at its default address, in the given mode.
    pub fn led_matrix(grayscale: bool) -> Self {
        let mut node = Self::new(0x72, b"MON", 12);
        node.matrix_grayscale = Some(false);
        node.set_matrix_mode(grayscale);
        node
    }

    fn set_matrix_mode(&mut self, grayscale: bool) {
        self.matrix_grayscale = Some(grayscale);
        self.write_len = if grayscale { 48 } else { 12 };
        self.payload = if grayscale { b"GS4" } else { b"MON" }.to_vec();
    }

    /// A board at its strap default address.
    pub fn new(strap: u8, payload: &[u8], write_len: usize) -> Self {
        Self {
            addr: strap >> 1,
            strap,
            payload: payload.to_vec(),
            queued: Default::default(),
            write_len,
            frames: Vec::new(),
            dropped: 0,
            address_changes: 0,
            matrix_grayscale: None,
        }
    }

    /// The same board at another address.
    pub fn at(mut self, addr: u8) -> Self {
        self.addr = addr;
        self
    }

    fn read(&mut self, buf: &mut [u8]) {
        if let Some(next) = self.queued.pop_front() {
            self.payload = next;
        }
        for (i, b) in buf.iter_mut().enumerate() {
            *b = match i {
                0 => self.strap,
                _ => self.payload.get(i - 1).copied().unwrap_or(0xFF),
            };
        }
    }

    fn write(&mut self, data: &[u8]) {
        if data.len() < self.write_len {
            self.dropped += 1;
            return;
        }
        let data = &data[..self.write_len];
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
        if self.matrix_grayscale.is_some() {
            if data.starts_with(b"GS4") {
                self.set_matrix_mode(true);
                return;
            }
            if data.starts_with(b"MON") {
                self.set_matrix_mode(false);
                return;
            }
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

/// A bus with node boards and hubs on it.
#[derive(Debug, Default)]
pub struct FakeBus {
    /// Boards on the bus.
    pub nodes: Vec<FakeNode>,
    /// Hubs on the bus; their ports carry further buses.
    pub hubs: Vec<FakeHub>,
    /// Transactions attempted, answered or not.
    pub transactions: usize,
    /// Shared with a `FakeDelay` to record the order of bus and delay.
    pub log: Option<Log>,
}

/// A TCA9548A: one control byte at its address, bit `n` connecting port
/// `n` to the upstream bus. A write sets the byte (the last byte written
/// wins); a read returns it. It does not answer general call.
#[derive(Debug)]
pub struct FakeHub {
    /// 7-bit address, 0x70-0x77.
    pub addr: u8,
    /// Control byte; 0 at power-up.
    pub control: u8,
    /// The eight downstream buses.
    pub ports: Vec<FakeBus>,
    /// Every control byte written, oldest first.
    pub writes: Vec<u8>,
}

impl FakeHub {
    /// A hub at `addr` with empty ports.
    pub fn new(addr: u8) -> Self {
        Self {
            addr,
            control: 0,
            ports: (0..8).map(|_| FakeBus::default()).collect(),
            writes: Vec::new(),
        }
    }

    /// The same hub with `nodes` on `port`.
    pub fn with_nodes(mut self, port: usize, nodes: Vec<FakeNode>) -> Self {
        self.ports[port].nodes.extend(nodes);
        self
    }

    /// The same hub with `hub` on `port`.
    pub fn with_hub(mut self, port: usize, hub: FakeHub) -> Self {
        self.ports[port].hubs.push(hub);
        self
    }
}

/// One addressable thing a transaction reached.
enum Target<'a> {
    Node(&'a mut FakeNode),
    Hub(&'a mut FakeHub),
}

/// One bus transaction or delay, in the order they happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A transaction to this 7-bit address.
    Bus(u8),
    /// A delay of this many nanoseconds (consecutive delays merged).
    Delay(u64),
}

/// An event log shared by a `FakeBus` and a `FakeDelay`.
pub type Log = std::rc::Rc<std::cell::RefCell<Vec<Event>>>;

impl FakeBus {
    /// A bus with these boards.
    pub fn new(nodes: Vec<FakeNode>) -> Self {
        Self {
            nodes,
            ..Default::default()
        }
    }

    /// A bus with these hubs and no boards of its own.
    pub fn with_hubs(hubs: Vec<FakeHub>) -> Self {
        Self {
            hubs,
            ..Default::default()
        }
    }

    /// The board at `addr`, searching every port whether connected or not.
    pub fn node(&self, addr: u8) -> &FakeNode {
        self.find_node(addr)
            .unwrap_or_else(|| panic!("no node at {addr:#04x}"))
    }

    fn find_node(&self, addr: u8) -> Option<&FakeNode> {
        self.nodes.iter().find(|n| n.addr == addr).or_else(|| {
            self.hubs
                .iter()
                .flat_map(|h| h.ports.iter())
                .find_map(|p| p.find_node(addr))
        })
    }

    /// The hub at `addr`, searching every port.
    pub fn hub(&self, addr: u8) -> &FakeHub {
        self.find_hub(addr)
            .unwrap_or_else(|| panic!("no hub at {addr:#04x}"))
    }

    fn find_hub(&self, addr: u8) -> Option<&FakeHub> {
        self.hubs.iter().find(|h| h.addr == addr).or_else(|| {
            self.hubs
                .iter()
                .flat_map(|h| h.ports.iter())
                .find_map(|p| p.find_hub(addr))
        })
    }

    /// Everything that answers `addr` from here: this bus's boards and
    /// hubs, and whatever is on the hubs' connected ports. Address 0
    /// reaches every board but no hub.
    fn targets<'a>(&'a mut self, addr: u8, out: &mut Vec<Target<'a>>) {
        for node in self.nodes.iter_mut() {
            if addr == 0 || node.addr == addr {
                out.push(Target::Node(node));
            }
        }
        for hub in self.hubs.iter_mut() {
            let (hub_addr, control) = (hub.addr, hub.control);
            if addr != 0 && hub_addr == addr {
                // A hub answering here cannot also route the same
                // transaction downstream in this model.
                out.push(Target::Hub(hub));
                continue;
            }
            for (i, port) in hub.ports.iter_mut().enumerate() {
                if control >> i & 1 == 1 {
                    port.targets(addr, out);
                }
            }
        }
    }
}

impl ErrorType for FakeBus {
    type Error = Nack;
}

impl I2c for FakeBus {
    fn transaction(&mut self, address: u8, ops: &mut [Operation<'_>]) -> Result<(), Nack> {
        self.transactions += 1;
        if let Some(log) = &self.log {
            log.borrow_mut().push(Event::Bus(address));
        }
        let mut targets = Vec::new();
        self.targets(address, &mut targets);
        if targets.is_empty()
            || (address == 0 && ops.iter().any(|op| matches!(op, Operation::Read(_))))
        {
            return Err(Nack(address));
        }
        for op in ops.iter_mut() {
            match op {
                Operation::Write(data) => {
                    for t in targets.iter_mut() {
                        match t {
                            Target::Node(n) => n.write(data),
                            Target::Hub(h) => {
                                if let Some(&last) = data.last() {
                                    h.control = last;
                                    h.writes.push(last);
                                }
                            }
                        }
                    }
                }
                Operation::Read(buf) => {
                    // Open-drain: with several devices answering, a bit
                    // reads 1 only if every device sends 1.
                    buf.fill(0xFF);
                    let mut mine = vec![0u8; buf.len()];
                    for t in targets.iter_mut() {
                        match t {
                            Target::Node(n) => n.read(&mut mine),
                            Target::Hub(h) => mine.fill(h.control),
                        }
                        buf.iter_mut().zip(&mine).for_each(|(b, m)| *b &= m);
                    }
                }
            }
        }
        Ok(())
    }
}

/// A delay that only records how long it was asked to wait.
#[derive(Debug, Default)]
pub struct FakeDelay {
    /// Total requested, in nanoseconds.
    pub total_ns: u64,
    /// Shared with a `FakeBus` to record the order of bus and delay.
    pub log: Option<Log>,
}

impl embedded_hal::delay::DelayNs for FakeDelay {
    fn delay_ns(&mut self, ns: u32) {
        self.total_ns += ns as u64;
        if let Some(log) = &self.log {
            let mut log = log.borrow_mut();
            match log.last_mut() {
                Some(Event::Delay(total)) => *total += ns as u64,
                _ => log.push(Event::Delay(ns as u64)),
            }
        }
    }
}
