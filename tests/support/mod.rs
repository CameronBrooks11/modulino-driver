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
    /// Writes longer than the board's length. The board processes their
    /// first bytes; what the bus does with the rest is unverified, so a
    /// driver must never send one.
    pub long: usize,
    /// Set by a `CF` command.
    pub address_changes: usize,
    /// Writes still to fail: the board NACKs each one and processes
    /// nothing, as a busy or resetting board would.
    pub fail_writes: usize,
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
            long: 0,
            address_changes: 0,
            fail_writes: 0,
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
        if data.len() > self.write_len {
            self.long += 1;
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
    /// Register-based chips on the bus.
    pub chips: Vec<FakeChip>,
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

/// A register-based sensor: a write of `index_len` bytes sets the register
/// pointer (big-endian), further bytes write registers, and a read returns
/// registers from the pointer on, auto-incrementing. Unset registers read
/// 0x00. A write that equals one of `commands` instead queues that
/// command's reply for the next read (SHT3x style).
#[derive(Debug, Clone)]
pub struct FakeChip {
    /// 7-bit address.
    pub addr: u8,
    /// Register index width in bytes: 1, or 2 for the VL53L4.
    pub index_len: usize,
    /// Register contents.
    pub regs: std::collections::BTreeMap<u16, u8>,
    /// Command bytes and the reply each queues.
    pub commands: Vec<(Vec<u8>, Vec<u8>)>,
    /// NACK a read while no reply is pending, as an SHT3x does.
    pub nack_idle_reads: bool,
    /// Reads NACKed after a command before its reply is ready.
    pub reply_after: usize,
    /// Registers that ignore writes, as status registers do.
    pub read_only: Vec<u16>,
    /// Every write frame received, in order.
    pub writes: Vec<Vec<u8>>,
    pointer: u16,
    reply: Option<Vec<u8>>,
    not_ready: usize,
}

impl FakeChip {
    /// A chip at `addr` with `index_len`-byte register indices.
    pub fn new(addr: u8, index_len: usize) -> Self {
        Self {
            addr,
            index_len,
            regs: Default::default(),
            commands: Vec::new(),
            nack_idle_reads: false,
            reply_after: 0,
            read_only: Vec::new(),
            writes: Vec::new(),
            pointer: 0,
            reply: None,
            not_ready: 0,
        }
    }

    /// The same chip with `register` set to `value`.
    pub fn reg(mut self, register: u16, value: u8) -> Self {
        self.regs.insert(register, value);
        self
    }

    /// The same chip with a read-only `register` holding `value`.
    pub fn fixed(mut self, register: u16, value: u8) -> Self {
        self.read_only.push(register);
        self.reg(register, value)
    }

    /// The same chip answering `command` with `reply` on the next read.
    pub fn command(mut self, command: &[u8], reply: &[u8]) -> Self {
        self.commands.push((command.to_vec(), reply.to_vec()));
        self
    }

    fn write(&mut self, data: &[u8]) {
        self.writes.push(data.to_vec());
        if let Some((_, reply)) = self.commands.iter().find(|(c, _)| c == data) {
            self.reply = Some(reply.clone());
            self.not_ready = self.reply_after;
            return;
        }
        if data.len() < self.index_len {
            return;
        }
        let (index, values) = data.split_at(self.index_len);
        self.pointer = index.iter().fold(0u16, |acc, &b| acc << 8 | b as u16);
        for &v in values {
            if !self.read_only.contains(&self.pointer) {
                self.regs.insert(self.pointer, v);
            }
            self.pointer = self.pointer.wrapping_add(1);
        }
    }

    /// Whether this read is NACKed (and counts down a pending reply).
    fn nacks_read(&mut self) -> bool {
        if self.reply.is_some() && self.not_ready > 0 {
            self.not_ready -= 1;
            return true;
        }
        self.reply.is_none() && self.nack_idle_reads
    }

    fn read(&mut self, buf: &mut [u8]) {
        if let Some(reply) = self.reply.take() {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = reply.get(i).copied().unwrap_or(0xFF);
            }
            return;
        }
        for b in buf.iter_mut() {
            *b = self.regs.get(&self.pointer).copied().unwrap_or(0);
            self.pointer = self.pointer.wrapping_add(1);
        }
    }
}

/// One addressable thing a transaction reached.
enum Target<'a> {
    Node(&'a mut FakeNode),
    Chip(&'a mut FakeChip),
    /// A hub's control byte and its write log.
    Hub(&'a mut u8, &'a mut Vec<u8>),
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
        for chip in self.chips.iter_mut() {
            if addr != 0 && chip.addr == addr {
                out.push(Target::Chip(chip));
            }
        }
        for hub in self.hubs.iter_mut() {
            let (hub_addr, control) = (hub.addr, hub.control);
            // The hub answers its own address while its connected ports
            // carry the same transaction: the segments are joined.
            let FakeHub {
                ports,
                control: ctrl,
                writes,
                ..
            } = hub;
            if addr != 0 && hub_addr == addr {
                out.push(Target::Hub(ctrl, writes));
            }
            for (i, port) in ports.iter_mut().enumerate() {
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
                    let mut nacked = false;
                    for t in targets.iter_mut() {
                        if let Target::Node(n) = t {
                            if n.fail_writes > 0 {
                                n.fail_writes -= 1;
                                nacked = true;
                            }
                        }
                    }
                    if nacked {
                        return Err(Nack(address));
                    }
                    for t in targets.iter_mut() {
                        match t {
                            Target::Node(n) => n.write(data),
                            Target::Chip(c) => c.write(data),
                            Target::Hub(control, writes) => {
                                if let Some(&last) = data.last() {
                                    **control = last;
                                    writes.push(last);
                                }
                            }
                        }
                    }
                }
                Operation::Read(buf) => {
                    // On the wire any device's ACK wins: the read fails
                    // only if every device here NACKs it.
                    let mut acked = false;
                    for t in targets.iter_mut() {
                        acked |= match t {
                            Target::Chip(c) => !c.nacks_read(),
                            _ => true,
                        };
                    }
                    if !acked {
                        return Err(Nack(address));
                    }
                    // Open-drain: with several devices answering, a bit
                    // reads 1 only if every device sends 1.
                    buf.fill(0xFF);
                    let mut mine = vec![0u8; buf.len()];
                    for t in targets.iter_mut() {
                        match t {
                            Target::Node(n) => n.read(&mut mine),
                            Target::Chip(c) => c.read(&mut mine),
                            Target::Hub(control, _) => mine.fill(**control),
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
