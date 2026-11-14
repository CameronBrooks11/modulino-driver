//! Scan identifies by evidence, never by address alone.

mod support;

use core::cell::RefCell;

use modulino_driver::spec::{sensirion_crc8, NODES};
use modulino_driver::{scan, scan_except, Address, ChipKind, Found, Hub, Kind};
use support::{FakeBus, FakeChip, FakeHub, FakeNode};

fn a(raw: u8) -> Address {
    Address::new(raw).unwrap()
}

fn run(bus: &mut FakeBus) -> Vec<Found> {
    let mut out = [Found::Unknown(a(1)); 32];
    let n = scan(bus, &mut out).unwrap();
    assert!(n <= out.len());
    out[..n].to_vec()
}

#[test]
fn hub_and_vibro_are_told_apart() {
    // The Vibro's strap byte is 0x70, the hub's address: a table that mixes
    // 7-bit and 8-bit values confuses them (arduino-modulino-mpy).
    let mut bus = FakeBus {
        hubs: vec![FakeHub::new(0x70)],
        ..FakeBus::new(vec![FakeNode::new(0x70, &[0; 3], 12)])
    };
    assert_eq!(
        run(&mut bus),
        vec![Found::Node(a(0x38), Kind::Vibro), Found::Hub(a(0x70))]
    );
    let hub = bus.hub(0x70);
    assert_eq!(hub.control, 0, "restored");
    assert_eq!(
        hub.writes,
        vec![0x01, 0x00],
        "odd mask, then the probed byte"
    );
}

#[test]
fn latch_relay_at_its_factory_address() {
    let mut bus = FakeBus::new(vec![FakeNode::new(0x04, &[0; 3], 3)]);
    assert_eq!(run(&mut bus), vec![Found::Node(a(0x02), Kind::LatchRelay)]);
}

#[test]
fn buttons_moved_to_the_distance_address_is_buttons() {
    let mut bus = FakeBus::new(vec![FakeNode::new(0x7C, &[0; 3], 3).at(0x29)]);
    assert_eq!(run(&mut bus), vec![Found::Node(a(0x29), Kind::Buttons)]);
    assert!(
        bus.node(0x29).frames.is_empty(),
        "ID check writes are dropped"
    );
}

#[test]
fn unknown_device_is_unknown() {
    let mut bus = FakeBus {
        chips: vec![FakeChip::new(0x50, 1).reg(0, 0xFF)],
        ..Default::default()
    };
    assert_eq!(run(&mut bus), vec![Found::Unknown(a(0x50))]);
}

#[test]
fn pixels_at_0x6a_reads_as_movement() {
    // Documented ambiguity: the Pixels strap byte equals the LSM6DSOX
    // WHO_AM_I value, and a node returns its strap byte for any read.
    let mut bus = FakeBus::new(vec![FakeNode::new(0x6C, &[0; 32], 32).at(0x6A)]);
    assert_eq!(
        run(&mut bus),
        vec![Found::Chip(a(0x6A), ChipKind::Lsm6dsox)]
    );
}

#[test]
fn node_moved_into_the_hub_range_is_a_node() {
    let mut bus = FakeBus::new(vec![FakeNode::new(0x7C, &[0; 3], 3).at(0x71)]);
    assert_eq!(run(&mut bus), vec![Found::Node(a(0x71), Kind::Buttons)]);
}

#[test]
fn every_node_kind_at_its_default_address() {
    let nodes = NODES
        .iter()
        .map(|n| FakeNode::new(n.straps[0], &[0; 32], 3))
        .collect();
    let mut bus = FakeBus::new(nodes);
    let mut want: Vec<Found> = NODES
        .iter()
        .map(|n| Found::Node(a(n.straps[0] >> 1), n.kind))
        .collect();
    want.sort_by_key(|f| f.address());
    assert_eq!(run(&mut bus), want);
}

#[test]
fn chips_are_identified_by_their_id_checks() {
    let status = [0x80, 0x10];
    let sht31 = [status[0], status[1], sensirion_crc8(&status)];
    let mut bus = FakeBus {
        chips: vec![
            FakeChip::new(0x29, 2).reg(0x010F, 0xEB).reg(0x0110, 0xAA),
            FakeChip::new(0x44, 1),
            FakeChip::new(0x45, 1).command(&[0xF3, 0x2D], &sht31),
            FakeChip::new(0x53, 1).reg(0x06, 0xC2),
            FakeChip::new(0x5C, 1).reg(0x0F, 0xB1),
            FakeChip::new(0x6B, 1).reg(0x0F, 0x6C),
        ],
        ..Default::default()
    };
    assert_eq!(
        run(&mut bus),
        vec![
            Found::Chip(a(0x29), ChipKind::Vl53l4),
            Found::Unknown(a(0x44)),
            Found::Chip(a(0x45), ChipKind::Sht31),
            Found::Chip(a(0x53), ChipKind::Ltr381rgb),
            Found::Chip(a(0x5C), ChipKind::Lps22hb),
            Found::Chip(a(0x6B), ChipKind::Lsm6dsox),
        ]
    );
}

#[test]
fn wrong_chip_id_is_unknown() {
    let mut bus = FakeBus {
        chips: vec![
            FakeChip::new(0x5C, 1).reg(0x0F, 0xB3),
            FakeChip::new(0x53, 1).reg(0x06, 0xB2),
            FakeChip::new(0x45, 1).command(&[0xF3, 0x2D], &[0x80, 0x10, 0x00]),
        ],
        ..Default::default()
    };
    assert_eq!(
        run(&mut bus),
        vec![
            Found::Unknown(a(0x45)),
            Found::Unknown(a(0x53)),
            Found::Unknown(a(0x5C)),
        ]
    );
}

#[test]
fn scan_behind_a_port_skips_the_hubs_above() {
    let inner = FakeHub::new(0x71).with_nodes(2, vec![FakeNode::new(0x7C, &[0; 3], 3)]);
    let outer = FakeHub::new(0x70)
        .with_nodes(0, vec![FakeNode::new(0x58, &[0; 3], 3)])
        .with_hub(1, inner);
    let bus = RefCell::new(FakeBus::with_hubs(vec![outer]));
    let hub = Hub::new(&bus);
    let mut out = [Found::Unknown(a(1)); 8];

    // Without the skip list, the hub answers through its own port.
    let n = scan(&mut hub.port(0).unwrap(), &mut out).unwrap();
    assert_eq!(
        &out[..n],
        [Found::Node(a(0x2C), Kind::Joystick), Found::Hub(a(0x70))]
    );

    let n = scan_except(&mut hub.port(0).unwrap(), &[a(0x70)], &mut out).unwrap();
    assert_eq!(&out[..n], [Found::Node(a(0x2C), Kind::Joystick)]);

    // Port 1 carries the inner hub, which is found as a hub.
    let n = scan_except(&mut hub.port(1).unwrap(), &[a(0x70)], &mut out).unwrap();
    assert_eq!(&out[..n], [Found::Hub(a(0x71))]);

    // Through the inner hub's port 2, skip both hubs above.
    let outer_port = RefCell::new(hub.port(1).unwrap());
    let inner = Hub::with_address(&outer_port, a(0x71)).unwrap();
    let n = scan_except(&mut inner.port(2).unwrap(), &[a(0x70), a(0x71)], &mut out).unwrap();
    assert_eq!(&out[..n], [Found::Node(a(0x3E), Kind::Buttons)]);

    let bus = bus.into_inner();
    assert_eq!(bus.hub(0x70).control, 0);
    assert_eq!(bus.hub(0x71).control, 0);
}

#[test]
fn a_full_buffer_still_counts_everything() {
    let mut bus = FakeBus::new(vec![
        FakeNode::new(0x7C, &[0; 3], 3),
        FakeNode::new(0x3C, &[0; 3], 8),
        FakeNode::new(0x58, &[0; 3], 3),
    ]);
    let mut out = [Found::Unknown(a(1)); 2];
    assert_eq!(scan(&mut bus, &mut out).unwrap(), 3);
    assert_eq!(
        out,
        [
            Found::Node(a(0x1E), Kind::Buzzer),
            Found::Node(a(0x2C), Kind::Joystick)
        ]
    );
}

#[test]
fn empty_bus_finds_nothing() {
    assert!(run(&mut FakeBus::default()).is_empty());
}

#[test]
fn hub_check_restores_the_control_byte_it_found() {
    let mut hub = FakeHub::new(0x72);
    hub.control = 0x04;
    let mut bus = FakeBus::with_hubs(vec![hub]);
    assert_eq!(run(&mut bus), vec![Found::Hub(a(0x72))]);
    assert_eq!(bus.hub(0x72).control, 0x04);
}

/// A bus where one address fails with a bus error rather than a NACK.
struct Faulty(FakeBus, u8);

impl embedded_hal::i2c::ErrorType for Faulty {
    type Error = embedded_hal::i2c::ErrorKind;
}

impl embedded_hal::i2c::I2c for Faulty {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        if address == self.1 {
            return Err(embedded_hal::i2c::ErrorKind::Bus);
        }
        self.0.transaction(address, operations).map_err(|_| {
            embedded_hal::i2c::ErrorKind::NoAcknowledge(
                embedded_hal::i2c::NoAcknowledgeSource::Address,
            )
        })
    }
}

#[test]
fn a_bus_error_stops_the_scan() {
    let mut bus = Faulty(FakeBus::new(vec![FakeNode::new(0x7C, &[0; 3], 3)]), 0x10);
    let mut out = [Found::Unknown(a(1)); 4];
    assert_eq!(
        scan(&mut bus, &mut out),
        Err(modulino_driver::Error::I2c(
            embedded_hal::i2c::ErrorKind::Bus
        ))
    );
}

#[test]
fn hs3003_data_that_looks_like_a_strap_is_still_unknown() {
    // The HS3003's first byte is status bits and humidity; at ~38 % RH it
    // reads 0x58, the Joystick strap. Every strap byte is tried.
    for strap in NODES.iter().flat_map(|n| n.straps.iter().copied()) {
        let mut bus = FakeBus {
            chips: vec![FakeChip::new(0x44, 1).reg(0, strap)],
            ..Default::default()
        };
        assert_eq!(run(&mut bus), vec![Found::Unknown(a(0x44))], "{strap:#04x}");
    }
}

fn sht31(reply_after: usize) -> FakeChip {
    let status = [0x80, 0x10];
    let mut chip = FakeChip::new(0x45, 1).command(
        &[0xF3, 0x2D],
        &[status[0], status[1], sensirion_crc8(&status)],
    );
    chip.nack_idle_reads = true;
    chip.reply_after = reply_after;
    chip
}

#[test]
fn sht31_that_nacks_idle_reads_is_found() {
    let mut bus = FakeBus {
        chips: vec![sht31(5)],
        ..Default::default()
    };
    assert_eq!(run(&mut bus), vec![Found::Chip(a(0x45), ChipKind::Sht31)]);
}

#[test]
fn sht31_reply_retries_are_bounded() {
    let mut bus = FakeBus {
        chips: vec![sht31(1000)],
        ..Default::default()
    };
    assert!(run(&mut bus).is_empty());
}

#[test]
fn failed_hub_check_still_restores_the_control_byte() {
    // A second hub at 0x70 on port 0 joins the segment once port 0 is
    // connected, so the read-back no longer matches.
    let inner = FakeHub::new(0x70);
    let mut bus = FakeBus::with_hubs(vec![FakeHub::new(0x70).with_hub(0, inner)]);
    run(&mut bus);
    assert_eq!(bus.hubs[0].control, 0, "port 0 disconnected again");

    // A board moved to 0x70 behind port 0.
    let board = FakeNode::new(0x7C, &[0; 3], 3).at(0x70);
    let mut bus = FakeBus::with_hubs(vec![FakeHub::new(0x70).with_nodes(0, vec![board])]);
    run(&mut bus);
    assert_eq!(bus.hubs[0].control, 0, "port 0 disconnected again");
}

#[test]
fn parent_hub_through_port_2_looks_like_a_latch_relay() {
    // Why scan_except exists: through port n the hub reads back 1 << n.
    let bus = RefCell::new(FakeBus::with_hubs(vec![FakeHub::new(0x70)]));
    let hub = Hub::new(&bus);
    let mut out = [Found::Unknown(a(1)); 4];
    let n = scan(&mut hub.port(2).unwrap(), &mut out).unwrap();
    assert_eq!(&out[..n], [Found::Node(a(0x70), Kind::LatchRelay)]);
    let n = scan_except(&mut hub.port(2).unwrap(), &[a(0x70)], &mut out).unwrap();
    assert_eq!(n, 0);
}

/// Fails the `n`th transaction (1-based) to `addr` with `kind`; otherwise
/// forwards to the fake bus, reporting its NACKs as address NACKs.
struct FailNth {
    bus: FakeBus,
    addr: u8,
    n: usize,
    seen: usize,
    kind: embedded_hal::i2c::ErrorKind,
}

impl FailNth {
    fn new(bus: FakeBus, addr: u8, n: usize, kind: embedded_hal::i2c::ErrorKind) -> Self {
        Self {
            bus,
            addr,
            n,
            seen: 0,
            kind,
        }
    }
}

impl embedded_hal::i2c::ErrorType for FailNth {
    type Error = embedded_hal::i2c::ErrorKind;
}

impl embedded_hal::i2c::I2c for FailNth {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        if address == self.addr {
            self.seen += 1;
            if self.seen == self.n {
                return Err(self.kind);
            }
        }
        self.bus.transaction(address, operations).map_err(|_| {
            embedded_hal::i2c::ErrorKind::NoAcknowledge(
                embedded_hal::i2c::NoAcknowledgeSource::Address,
            )
        })
    }
}

fn scan_hub_failing(
    n: usize,
    kind: embedded_hal::i2c::ErrorKind,
) -> (
    FakeBus,
    Result<usize, modulino_driver::Error<embedded_hal::i2c::ErrorKind>>,
) {
    // Transactions to the hub: 1 probe, 2 mask write, 3 read-back, 4 restore.
    let mut bus = FailNth::new(FakeBus::with_hubs(vec![FakeHub::new(0x70)]), 0x70, n, kind);
    let mut out = [Found::Unknown(a(1)); 4];
    let result = scan(&mut bus, &mut out);
    (bus.bus, result)
}

#[test]
fn hub_read_back_error_still_restores() {
    let (bus, result) = scan_hub_failing(3, embedded_hal::i2c::ErrorKind::Bus);
    assert_eq!(
        result,
        Err(modulino_driver::Error::I2c(
            embedded_hal::i2c::ErrorKind::Bus
        ))
    );
    assert_eq!(bus.hub(0x70).control, 0, "restored before the error");
}

#[test]
fn nacked_hub_restore_is_an_error() {
    let nack =
        embedded_hal::i2c::ErrorKind::NoAcknowledge(embedded_hal::i2c::NoAcknowledgeSource::Data);
    let (_, result) = scan_hub_failing(4, nack);
    assert_eq!(result, Err(modulino_driver::Error::I2c(nack)));
}

#[test]
fn sht31_status_read_error_stops_the_scan() {
    // 0x45: probe (NACKed, silent sensor), command, then the status read.
    let bus = FakeBus {
        chips: vec![sht31(0)],
        ..Default::default()
    };
    let mut bus = FailNth::new(bus, 0x45, 3, embedded_hal::i2c::ErrorKind::Bus);
    let mut out = [Found::Unknown(a(1)); 4];
    assert_eq!(
        scan(&mut bus, &mut out),
        Err(modulino_driver::Error::I2c(
            embedded_hal::i2c::ErrorKind::Bus
        ))
    );
}

#[test]
fn an_empty_bus_costs_one_probe_per_address() {
    let mut bus = FakeBus::default();
    run(&mut bus);
    // One probe per address, plus the SHT31 status command at 0x45, which
    // runs after a NACKed probe and stops at its own NACK.
    assert_eq!(bus.transactions, 0x77 + 1);
}

#[test]
fn any_ack_wins_on_a_shared_address() {
    // A node and a silent SHT31 both at 0x45: the node ACKs the read.
    use embedded_hal::i2c::I2c;
    let mut bus = FakeBus {
        chips: vec![sht31(0)],
        ..FakeBus::new(vec![FakeNode::new(0x7C, &[0; 3], 3).at(0x45)])
    };
    assert!(bus.read(0x45, &mut [0u8; 1]).is_ok());
}
