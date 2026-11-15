//! Simulated Modulino boards on a simulated I2C bus, for running the real
//! drivers on a host with no hardware: a downstream crate's mock mode, or
//! its CI. Start from [`SimBus`].
//!
//! Behind the `sim` feature, which brings in `std`; the default build stays
//! `no_std` with no allocation. The models follow the node firmware source
//! (`node_modulino_firmware` at `7d578e0`) and the chips' datasheets. None
//! has been checked against hardware, and this module is outside the
//! crate's semver promise until the bench run confirms the models.
//!
//! The lower-level `Fake*` types the crate's own tests build on are hidden
//! from these docs and may change in any release.
//!
//! How a node board behaves here, from the firmware:
//! - a read returns the strap byte and then the payload (main.c:471-529);
//! - the firmware arms a receive of exactly the board's length
//!   (main.c:531-561), and the STM32C0 HAL completes it when that many bytes
//!   have arrived (stm32c0xx_hal_i2c.c, I2C_Slave_ISR_IT): a shorter write
//!   is dropped with no error on the bus, and a longer one has its first
//!   bytes processed. The model then accepts the rest; the real board may
//!   instead stretch the clock or NACK, which is unverified;
//! - a processed frame starting `CF` moves the board to a new address and
//!   one starting `DIE` sends it to the ROM bootloader (main.c:235-254), which
//!   answers at 0x64 (Arduino_Modulino FirmwareUpdater.ino:63-64).
//!
//! What the board does with the bytes past its length is not modelled.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::vec;
use std::vec::Vec;

use embedded_hal::i2c::{self, ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};

#[doc(hidden)]
/// The address the STM32C0 ROM bootloader answers at.
pub const BOOTLOADER: u8 = 0x64;

#[doc(hidden)]
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
    pub queued: VecDeque<Vec<u8>>,
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
    /// A `CF` taken and not yet acted on: the new 7-bit address and the log
    /// length when the frame arrived. The board keeps listening at its old
    /// address, since it acts on the command from its main loop
    /// (main.c:219-221, 238-253), and is at the new one once a delay is
    /// logged after that point. When it stops listening and reboots in
    /// between is unverified, so the driver must not depend on either.
    /// Only modelled when the bus has a log; without one the address
    /// changes at once.
    pub pending: Option<(u8, usize)>,
    /// The length of every `CF` frame the board received, as sent.
    pub cf_lengths: Vec<usize>,
    /// `Some(grayscale)` for an LED matrix, whose write length and read
    /// payload follow its mode (main.c:324-338, 506-516, 555-556).
    pub matrix_grayscale: Option<bool>,
    /// Off the bus: the board answers nothing.
    pub absent: bool,
    /// How a processed frame shows in later reads.
    pub echo: Echo,
}

#[doc(hidden)]
/// How a node's processed frame shows in its read payload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Echo {
    /// It does not: the payload stays as set.
    #[default]
    None,
    /// Opto relay: byte 0 is the relay pin, set from the frame's byte 0
    /// (main.c:312-314, 496-500).
    OptoRelay,
    /// Latch relay: bytes 0 and 1 are the reset- and set-side pins, `[0, 1]`
    /// after "on" and `[1, 0]` after "off" (main.c:315-323, 501-505).
    LatchRelay,
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
            pending: None,
            cf_lengths: Vec::new(),
            matrix_grayscale: None,
            absent: false,
            echo: Echo::None,
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
        if data.starts_with(b"CF") {
            self.cf_lengths.push(data.len());
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
        match self.echo {
            Echo::None => {}
            Echo::OptoRelay => self.payload[0] = u8::from(data[0] != 0),
            Echo::LatchRelay => {
                let pins = if data[0] != 0 { [0, 1] } else { [1, 0] };
                self.payload[..2].copy_from_slice(&pins);
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

#[doc(hidden)]
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

#[doc(hidden)]
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
    /// Off the bus: neither the hub nor anything behind it answers.
    pub absent: bool,
}

impl FakeHub {
    /// A hub at `addr` with empty ports.
    pub fn new(addr: u8) -> Self {
        Self {
            addr,
            control: 0,
            ports: (0..8).map(|_| FakeBus::default()).collect(),
            writes: Vec::new(),
            absent: false,
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

#[doc(hidden)]
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
    pub regs: BTreeMap<u16, u8>,
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
    /// Off the bus: the chip answers nothing.
    pub absent: bool,
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
            absent: false,
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

#[doc(hidden)]
/// One bus transaction or delay, in the order they happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A transaction to this 7-bit address.
    Bus(u8),
    /// A delay of this many nanoseconds (consecutive delays merged).
    Delay(u64),
}

#[doc(hidden)]
/// An event log shared by a `FakeBus` and a `FakeDelay`. `Arc<Mutex<..>>`
/// so a bus that holds one is `Send`.
pub type Log = Arc<Mutex<Vec<Event>>>;

/// The log, even if a thread panicked while holding it: a poisoned lock
/// would otherwise make every later bus call panic too.
fn lock(log: &Log) -> MutexGuard<'_, Vec<Event>> {
    log.lock().unwrap_or_else(PoisonError::into_inner)
}

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

    /// Finish every pending address change, here and behind every hub
    /// port, that a delay logged since the `CF` has let through.
    pub fn settle(&mut self) {
        let Some(log) = self.log.clone() else { return };
        let log = lock(&log);
        self.settle_with(&log);
    }

    fn settle_with(&mut self, log: &[Event]) {
        for node in self.nodes.iter_mut() {
            if let Some((new, mark)) = node.pending {
                if log[mark..].iter().any(|e| matches!(e, Event::Delay(_))) {
                    node.addr = new;
                    node.pending = None;
                }
            }
        }
        for hub in self.hubs.iter_mut() {
            for port in hub.ports.iter_mut() {
                port.settle_with(log);
            }
        }
    }

    /// Everything that answers `addr` from here: this bus's boards and
    /// hubs, and whatever is on the hubs' connected ports. Address 0
    /// reaches every board but no hub.
    fn targets<'a>(&'a mut self, addr: u8, out: &mut Vec<Target<'a>>) {
        for node in self.nodes.iter_mut() {
            if !node.absent && (addr == 0 || node.addr == addr) {
                out.push(Target::Node(node));
            }
        }
        for chip in self.chips.iter_mut() {
            if !chip.absent && addr != 0 && chip.addr == addr {
                out.push(Target::Chip(chip));
            }
        }
        for hub in self.hubs.iter_mut().filter(|h| !h.absent) {
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
        self.settle();
        if let Some(log) = &self.log {
            lock(log).push(Event::Bus(address));
        }
        let log = self.log.clone();
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
                            Target::Node(n) => {
                                let (old, changes) = (n.addr, n.address_changes);
                                n.write(data);
                                // With a log, the new address waits for a delay.
                                if let (true, Some(log)) = (n.address_changes > changes, &log) {
                                    n.pending = Some((n.addr, lock(log).len()));
                                    n.addr = old;
                                }
                            }
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

#[doc(hidden)]
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
            let mut log = lock(log);
            match log.last_mut() {
                Some(Event::Delay(total)) => *total += ns as u64,
                _ => log.push(Event::Delay(ns as u64)),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

use crate::spec::{sensirion_crc8, ChipKind, Kind, WriteLen, CHIPS};
use crate::Address;

/// The error a [`SimBus`] returns: nothing acknowledged the transfer. Its
/// [`kind`](i2c::Error::kind) is [`ErrorKind::NoAcknowledge`], as a real
/// bus reports for an absent device.
pub type SimError = Nack;

/// A simulated I2C bus holding simulated Modulino boards, for running the
/// real drivers on a host with no hardware.
///
/// Add node boards by [`Kind`], chips by [`ChipKind`], and TCA9548A hubs
/// with boards on their ports. Each starts at its default address with
/// plausible, stable readings: no button pressed, the joystick centred, a
/// room at about 25 °C and 50 %RH, 1013.25 hPa, about 175 lux, a target
/// 200 mm away, and a board lying flat. Output boards keep what was last
/// written: read it back with [`SimBus::last_write`], or through the driver
/// where the firmware reports it (the opto and latch relays, the LED
/// matrix's mode).
///
/// The models follow the node firmware source and the chips' datasheets.
/// None has been checked against hardware, and this API is outside the
/// crate's semver promise until the bench run confirms the models.
///
/// ```
/// use modulino_driver::sim::SimBus;
/// use modulino_driver::{Buttons, Kind, OptoRelay};
///
/// let bus = SimBus::new().with_node(Kind::OptoRelay);
/// let mut relay = OptoRelay::new(bus);
/// relay.on().unwrap();
/// assert!(relay.is_on().unwrap());
///
/// // An absent board is a NACK, as on real hardware.
/// let mut buttons = Buttons::new(relay.release());
/// assert!(buttons.verify().is_err());
/// ```
#[derive(Debug, Default)]
pub struct SimBus {
    bus: FakeBus,
}

/// A simulated TCA9548A hub, to add to a [`SimBus`] with boards on its
/// ports. Drive it with [`Hub`](crate::Hub) over a `RefCell<SimBus>`.
#[derive(Debug)]
pub struct SimHub {
    hub: FakeHub,
}

impl SimHub {
    /// A hub at `addr` (0x70-0x77) with nothing on its ports.
    pub fn new(addr: Address) -> Self {
        Self {
            hub: FakeHub::new(addr.get()),
        }
    }

    /// The same hub with the boards of `bus` on `port` (0-7).
    ///
    /// # Panics
    ///
    /// If `port` is above 7.
    pub fn with_port(mut self, port: u8, bus: SimBus) -> Self {
        let target = &mut self.hub.ports[usize::from(port)];
        target.nodes.extend(bus.bus.nodes);
        target.chips.extend(bus.bus.chips);
        target.hubs.extend(bus.bus.hubs);
        self
    }
}

impl SimBus {
    /// An empty bus: every address NACKs.
    pub fn new() -> Self {
        Self::default()
    }

    /// The same bus with a node board of `kind` at its default address.
    pub fn with_node(self, kind: Kind) -> Self {
        self.with_node_at(kind, kind.default_address())
    }

    /// The same bus with a node board of `kind` at `addr`, as if its address
    /// had been changed. Its strap byte is still the kind's first.
    pub fn with_node_at(mut self, kind: Kind, addr: Address) -> Self {
        self.bus.nodes.push(node(kind).at(addr.get()));
        self
    }

    /// The same bus with a `chip` at its default address.
    pub fn with_chip(self, chip: ChipKind) -> Self {
        let addr = Address::new(chip_address(chip)).expect("a chip address is 7-bit");
        self.with_chip_at(chip, addr)
    }

    /// The same bus with a `chip` at `addr`.
    pub fn with_chip_at(mut self, chip: ChipKind, addr: Address) -> Self {
        let mut fake = self::chip(chip);
        fake.addr = addr.get();
        self.bus.chips.push(fake);
        self
    }

    /// The same bus with `hub` on it.
    pub fn with_hub(mut self, hub: SimHub) -> Self {
        self.bus.hubs.push(hub.hub);
        self
    }

    /// Take every board, chip and hub at `addr` off the bus, or put it
    /// back, here and behind every hub port. A board off the bus answers
    /// nothing, so a transfer to it fails with a NACK; it keeps its state
    /// for when it comes back. Returns whether anything is at `addr`.
    pub fn set_present(&mut self, addr: Address, present: bool) -> bool {
        set_present(&mut self.bus, addr.get(), !present)
    }

    /// The last frame the node board at `addr` processed, searching every
    /// hub port: what the driver last told it to do. `None` if no node is
    /// there or it has processed nothing.
    pub fn last_write(&self, addr: Address) -> Option<&[u8]> {
        self.bus
            .find_node(addr.get())
            .and_then(|n| n.frames.last())
            .map(Vec::as_slice)
    }

    /// Set what the node board at `addr` reports after its strap byte, as
    /// the inputs would: buttons pressed, a knob position, a joystick
    /// deflection. Searches every hub port. Returns whether a node is there.
    ///
    /// The bytes are the board's read payload as the firmware sends it
    /// (`codec::<kind>::decode` reads them); missing bytes read 0xFF.
    pub fn set_reading(&mut self, addr: Address, payload: &[u8]) -> bool {
        match find_node_mut(&mut self.bus, addr.get()) {
            Some(n) => {
                n.payload = payload.to_vec();
                true
            }
            None => false,
        }
    }
}

impl ErrorType for SimBus {
    type Error = SimError;
}

impl I2c for SimBus {
    fn transaction(&mut self, address: u8, ops: &mut [Operation<'_>]) -> Result<(), SimError> {
        self.bus.transaction(address, ops)
    }
}

fn set_present(bus: &mut FakeBus, addr: u8, absent: bool) -> bool {
    let mut found = false;
    for n in bus.nodes.iter_mut().filter(|n| n.addr == addr) {
        n.absent = absent;
        found = true;
    }
    for c in bus.chips.iter_mut().filter(|c| c.addr == addr) {
        c.absent = absent;
        found = true;
    }
    for h in bus.hubs.iter_mut() {
        if h.addr == addr {
            h.absent = absent;
            found = true;
        }
        for port in h.ports.iter_mut() {
            found |= set_present(port, addr, absent);
        }
    }
    found
}

fn find_node_mut(bus: &mut FakeBus, addr: u8) -> Option<&mut FakeNode> {
    if let Some(i) = bus.nodes.iter().position(|n| n.addr == addr) {
        return bus.nodes.get_mut(i);
    }
    for hub in bus.hubs.iter_mut() {
        for port in hub.ports.iter_mut() {
            if let Some(n) = find_node_mut(port, addr) {
                return Some(n);
            }
        }
    }
    None
}

/// The first address `spec::CHIPS` lists for `kind`.
fn chip_address(kind: ChipKind) -> u8 {
    CHIPS
        .iter()
        .find(|c| c.kind == kind)
        .expect("every chip kind has a spec row")
        .addresses[0]
}

/// A node board of `kind` at its default address, idle.
fn node(kind: Kind) -> FakeNode {
    let spec = kind.spec();
    let mut node = match spec.write_len {
        WriteLen::Fixed(len) => FakeNode::new(spec.straps[0], &vec![0; spec.read_payload], len),
        // Mono at power-up (main.c:75).
        WriteLen::Mode { .. } => FakeNode::led_matrix(false),
    };
    match kind {
        // The ADC's midpoint: centred (codec::joystick, `128 - raw`).
        Kind::Joystick => node.payload = vec![128, 128, 0],
        Kind::OptoRelay => node.echo = Echo::OptoRelay,
        Kind::LatchRelay => node.echo = Echo::LatchRelay,
        _ => {}
    }
    node
}

/// A `chip` at its default address that gets its driver through init and
/// returns a plausible reading.
fn chip(kind: ChipKind) -> FakeChip {
    let addr = chip_address(kind);
    match kind {
        // LSM6DSOX: WHO_AM_I 0x6C; the reset bit reads back cleared; both
        // data-ready bits set; accelerometer (0, 0, +1 g) at ±2 g, 0.061
        // mg/LSB (16393 = 0x4009); gyroscope at rest.
        ChipKind::Lsm6dsox => FakeChip::new(addr, 1)
            .fixed(0x0F, 0x6C)
            .fixed(0x12, 0x00)
            .fixed(0x1E, 0x03)
            .reg(0x2C, 0x09)
            .reg(0x2D, 0x40),
        // LPS22HB: WHO_AM_I 0xB1; SWRESET reads back cleared with
        // IF_ADD_INC set; both data-ready bits set; 1013.25 hPa
        // (x 4096 = 0x3F5400) and 25.00 C (x 100 = 0x09C4).
        ChipKind::Lps22hb => FakeChip::new(addr, 1)
            .fixed(0x0F, 0xB1)
            .fixed(0x11, 0x10)
            .fixed(0x27, 0x03)
            .reg(0x28, 0x00)
            .reg(0x29, 0x54)
            .reg(0x2A, 0x3F)
            .reg(0x2B, 0xC4)
            .reg(0x2C, 0x09),
        // LTR-381RGB: PART_ID 0xC2; new data always flagged; IR 500 and
        // green 1000, which at the driver's 18x gain and 16 bits is about
        // 175 lux; red and blue 1000.
        ChipKind::Ltr381rgb => FakeChip::new(addr, 1)
            .fixed(0x06, 0xC2)
            .fixed(0x07, 0x08)
            .reg(0x0A, 0xF4)
            .reg(0x0B, 0x01)
            .reg(0x0D, 0xE8)
            .reg(0x0E, 0x03)
            .reg(0x10, 0xE8)
            .reg(0x11, 0x03)
            .reg(0x13, 0xE8)
            .reg(0x14, 0x03),
        // VL53L4CD/ED: model ID, booted, the GPIO showing data ready, an
        // OSC_FREQUENCY of 3213, and a result of 200 mm with a valid range
        // status (raw 9) and 12 SPADs. The 0x2D-0x87 configuration the
        // driver loads is beyond the result registers.
        ChipKind::Vl53l4cd | ChipKind::Vl53l4ed => {
            let model = if kind == ChipKind::Vl53l4cd {
                0xEB
            } else {
                0xEC
            };
            FakeChip::new(addr, 2)
                .fixed(0x010F, model)
                .fixed(0x0110, 0xAA)
                .fixed(0x00E5, 0x03)
                .reg(0x0030, 0x01)
                .reg(0x0031, 0x01)
                .reg(0x0006, 0x0C)
                .reg(0x0007, 0x8D)
                .fixed(0x0089, 0x09)
                .fixed(0x008C, 0x0C)
                .fixed(0x008E, 0x01)
                .fixed(0x0091, 0x10)
                .fixed(0x0093, 0x28)
                .fixed(0x0096, 0x00)
                .fixed(0x0097, 0xC8)
        }
        // HS3003: a fetch after the measurement request returns status 00,
        // humidity 0x2000 (50 %) and temperature 6454 << 2 (25.0 C).
        ChipKind::Hs3003 => FakeChip::new(addr, 1)
            .reg(0x00, 0x20)
            .reg(0x01, 0x00)
            .reg(0x02, 0x64)
            .reg(0x03, 0xD8),
        // SHT31: NACKs reads until commanded; answers the status command
        // and the single-shot measurement (0x6666 = 25.0 C, 0x8000 = 50 %).
        ChipKind::Sht31 => {
            let crc = |w: [u8; 2]| [w[0], w[1], sensirion_crc8(&w)];
            let mut result = Vec::new();
            result.extend(crc([0x66, 0x66]));
            result.extend(crc([0x80, 0x00]));
            let mut c = FakeChip::new(addr, 1)
                .command(&[0xF3, 0x2D], &crc([0x80, 0x10]))
                .command(&[0x24, 0x00], &result);
            c.nack_idle_reads = true;
            c
        }
    }
}

/// A bus that holds simulated boards must be `Send + 'static`, so an owner
/// can move it to another thread; this fails to compile otherwise.
#[allow(dead_code)]
fn assert_send() {
    fn is_send<T: Send + 'static>() {}
    is_send::<FakeBus>();
    is_send::<FakeDelay>();
    is_send::<SimBus>();
}
