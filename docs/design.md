# modulino-driver design

Status: draft for review. Date: 2026-11-14.

`modulino-driver` is a layered, hardware-agnostic `no_std` Rust driver for
the Arduino Modulino family. It started from
[modulino-rs](https://github.com/melastmohican/modulino-rs) 0.3.0 (MIT OR
Apache-2.0), whose history it keeps, and restructures it around the node
protocol the Modulino firmware actually speaks.

This document fixes the architecture, the decisions behind it, how it is
verified, and the milestones. Facts about the firmware cite
`arduino/node_modulino_firmware` at commit `7d578e0` (2026-09-28) as
`main.c:<line>`, meaning `Core/Src/main.c`. Facts about the existing host
libraries cite `Arduino_Modulino` at `24d08a9` and `arduino-modulino-mpy` at
`f80607b`.

## 1. Goals and non-goals

Goals:

- **Simple.** Few concepts, each in one place. Less code than the crate it
  starts from (4730 lines of `src/` + `tests/`, `wc -l`).
- **General.** Any host with an `embedded-hal` 1.0 I2C implementation: MCU
  HALs (esp-hal, rp-hal, embassy, stm32, nrf), Linux through
  `linux-embedded-hal`, USB-I2C bridges.
- **Properly layered.** Bus, routing, framing, encoding, devices, discovery
  and service are separate layers with narrow interfaces, and each is
  testable without hardware.
- **Correct against stock firmware.** No firmware changes required. Every
  frame length, address and byte layout traces to a firmware line.
- **Spec-ready.** The protocol tables live as data from day one, so a
  formal spec, code generation or a second-language port can be adopted
  later without restructuring (section 6).

Non-goals for v1:

- Arduino C++, MicroPython, CircuitPython or CPython bindings.
- Firmware update over the ST bootloader. Arduino's FirmwareUpdater stays the
  reflash tool.
- Async (`embedded-hal-async`).
- Multi-task bus sharing behind a hub (section 4.2 says what is supported).
- Full chip feature sets (IMU FIFO and pedometer, VL53 calibration). Only
  what the Modulino product exposes.
- API compatibility with `modulino` 0.3 or with Arduino's class names.

## 2. The protocol in one page

Two kinds of board exist.

**Node boards** run one STM32C011 firmware. Board type comes from strap pins,
read into an even byte (`main.c:819-845`) that the firmware uses directly as
its HAL own-address (`main.c:651-653`). So the constants in `main.c:32-42`
are 8-bit, and the 7-bit bus address is `strap >> 1`.

| Kind | Strap | Default addr | Read: 1 + payload | Write length | Source |
|---|---|---|---|---|---|
| Buttons | 0x7C | 0x3E | 1+3: A, B, C | 3: ledA, ledB, ledC | `main.c:478-482, 544` |
| Buzzer | 0x3C | 0x1E | 1+3: stale | 8: freq u32, dur_ms u32 | `main.c:262-283, 546` |
| Vibro | 0x70 | 0x38 | 1+3: stale | 12: freq u32, dur_ms u32, duty u32 | `main.c:284-297, 548` |
| Knob | 0x74 / 0x76 | 0x3A / 0x3B | 1+3: pos i16, pressed | 4: target i16, pad 2 | `main.c:298-306, 483-488, 550` |
| Pixels | 0x6C | 0x36 | 1+32: not meaningful | 32: 8 x [0xE0\|bri, B, G, R] | `main.c:307-308, 553` |
| Joystick | 0x58 | 0x2C | 1+3: x u8, y u8, pressed | none (3, ignored) | `main.c:491-495, 560` |
| Opto relay | 0x28 | 0x14 | 1+3: state, 0, 0 | 3: on, pad 2 | `main.c:312-314, 496-500` |
| Latch relay | 0x04 | **0x02** | 1+3: PA2, PA3, 0 | 3: set, pad 2 (blocks 100 ms) | `main.c:315-323, 501-505` |
| LED matrix | 0x72 (forced) | 0x39 | 1+3: "MON" or "GS4" | 12 mono / 48 grayscale | `main.c:324-340, 506-516, 556` |
| Motors | 0x48 | 0x24 | 1+5: curA u16, curB u16, flags | 8: command letter + args | `motors.c:613-657, 815-828` |

All multi-byte fields are little-endian. Rules that apply to every node:

- **Reads have no register byte.** Byte 0 is always the strap byte, even
  after an address change (`main.c:476`). It is the board's identity.
- **Writes must be exactly the board's receive length.** A shorter write is
  dropped by the HAL with no error visible to the host
  (`main.c:531-561`, `stm32c0xx_hal_i2c.c:6372-6384`). A longer write is
  processed as soon as the board's length has arrived, so it acts on a
  truncated frame (`I2C_Slave_ISR_IT`); what the bus does with the extra
  bytes is unverified, and the source suggests the slave may stretch the
  clock. There is no application-level
  acknowledgement.
- **Command prefixes are checked on every write.** `CF` + addr8 rewrites the
  address in flash and resets the board. `DIE` jumps to the ROM bootloader at
  7-bit 0x64 (`main.c:235-254`). The check runs on raw data, so a data frame
  that happens to start with those bytes is executed as a command.
- **General call is enabled** (`main.c:655`): a write to 0x00 reaches every
  node.

**Chip boards** are off-the-shelf parts at their datasheet addresses:
Movement LSM6DSOX (0x6A/0x6B), Distance VL53L4CD or VL53L4ED (0x29), Thermo
HS3003 (0x44) or SHT31 (0x45), Light LTR381RGB (0x53), Pressure LPS22HB
(0x5C). The Hub is a TCA9548A (0x70-0x77): one control byte, bit n selects
port n.

### Defects in existing libraries this design removes by construction

| Defect | Where | Layer that removes it |
|---|---|---|
| Vibro `off()` sends 8 bytes, firmware needs 12, frame is dropped | `Modulino.h:369-373`, `vibro.rs:170-172` | Exact-length frames (4.3) |
| A data write starting `CF`/`DIE` reprograms or bootloads the board (Knob set 17987, Buzzer 17987 Hz, matrix frames) | `main.c:235-254` | Prefix guard (4.3) |
| AddressChanger pads CF to 40 bytes; a grayscale matrix needs 48 | `AddressChanger.ino:119` | Service pads to current length (4.7) |
| Hub select forgotten or not cleared on error | `Modulino.h:134-153`, `examples/multiplexer.rs:66-79` | Hub port is a bus (4.2) |
| Discovery trusts an ACK, or a colliding address table | `Modulino.h:180-194`, `device_manager.py:85-86` | Identity by evidence (4.6) |
| Write errors swallowed, failed reads update cached state | `Modulino.h:164-172, 223-231` | One `Result` contract (4.8) |
| Per-driver copies of framing and discovery | `buttons.rs:118-163` and 14 others | Node layer (4.3) |
| Wait loops that exit silently | `distance.rs:75-86, 97-107` | Bounded waits return `Timeout` (4.5) |
| 7-bit and 8-bit addresses mixed in one field | `modulino.py:189-195`, `lib.rs:96-174` | `Address` and `Strap` types (4.1) |

These come from reading code; none has been observed on hardware yet.
Section 7 says how each is confirmed at the bench.

## 3. Architecture

```
            ┌──────────────────────────────────────────────┐
  L6        │ service: set_address, reset_address,         │  hazardous commands only
            │          general_call_reset                  │
            ├──────────────────────────────────────────────┤
  L5        │ scan: identify by strap byte / chip ID       │
            ├───────────────────────┬──────────────────────┤
  L4        │ devices (nodes)       │ devices (chips)      │  user-facing API
            ├───────────────────────┼──────────────────────┤
  L3        │ codecs (pure)         │ chip drivers         │
            ├───────────────────────┤                      │
  L2        │ node framing          │                      │
            ├───────────────────────┴──────────────────────┤
  L1        │ hub routing: HubPort implements I2c          │  optional
            ├──────────────────────────────────────────────┤
  L0        │ embedded_hal::i2c::I2c                       │  not ours
            └──────────────────────────────────────────────┘
  spec/     protocol tables + test vectors (data; checked against src/spec.rs)
```

Dependencies point down only. Codecs depend on nothing. Everything above L0
sees only `I2c`, so every device, chip driver and the scanner works behind a
hub with no code of its own.

Module layout:

```
src/
  lib.rs
  error.rs
  addr.rs          Address, NewAddress, Strap
  spec.rs          hand-written protocol tables (generated later, section 6)
  hub.rs           L1
  node.rs          L2
  codec/           L3, one file per node kind
  chips/           L3, one file per chip
  devices/         L4, one file per module
  scan.rs          L5
  service.rs       L6
spec/
  nodes.toml       node kinds: strap, addresses, lengths, source lines
  chips.toml       chip kinds: addresses, ID register and value
  vectors/*.toml   codec test vectors
tests/
  support/         FakeNode, FakeHub, FakeChip bus simulator
```

## 4. Layers

### 4.1 Addresses and identity types (`addr.rs`)

- `Address(u8)`: a 7-bit bus address, `0x01..=0x77`. The range includes
  0x02 because the latch relay ships there. 0x00 (general call) and
  0x78-0x7F are not addresses.
- `NewAddress(u8)`: the target of an address change, `0x08..=0x77`, the
  range Arduino's tools accept (`AddressChanger.ino:147-151`). It converts to
  `Address`, not the other way round.
- `Strap(u8)`: the 8-bit identity byte. `Strap::kind()` maps it to `Kind`;
  `Kind::default_addresses()` yields `Address` values. The `>> 1` happens in
  exactly one place.

8-bit values never appear as addresses in the public API.

### 4.2 Hub routing (`hub.rs`)

`HubPort` implements `embedded_hal::i2c::I2c`. Each `transaction` does
select, the caller's operations, then deselect. Deselect runs even when the
operations fail, and the first error wins. Because a port is a bus, a hub
behind a hub port is a nested `HubPort` with no extra code.

For the select-operate-deselect sequence to be atomic, the port borrows the
whole underlying bus for its duration. v1 provides `HubPort<'a, B>` over
`&'a RefCell<B>`, the same single-context pattern as
`embedded_hal_bus::i2c::RefCellDevice`. A critical-section or mutex variant is
added when someone needs multi-task access; it is a second constructor, not a
redesign.

There is no port caching: a cache goes stale when anything else touches the
TCA9548A, which silently misroutes.

### 4.3 Node framing (`node.rs`)

The one place that knows the node protocol.

```rust
pub struct Node<I2C> { i2c: I2C, addr: Address, kind: Kind }

impl<I2C: I2c> Node<I2C> {
    pub fn new(i2c: I2C, addr: Address, kind: Kind) -> Self;           // no I/O
    pub fn read<const P: usize>(&mut self) -> Result<[u8; P], Error<I2C::Error>>;
    pub fn write<const W: usize>(&mut self, frame: &[u8; W]) -> Result<(), Error<I2C::Error>>;
    pub fn identify(&mut self) -> Result<Strap, Error<I2C::Error>>;  // 1-byte read
    pub fn release(self) -> I2C;
}
```

- **Every read checks identity.** `read` fetches `1 + P` bytes and fails
  with `WrongIdentity { expected, got }` if byte 0 is not one of the kind's
  straps. The byte arrives anyway, so the check costs nothing and catches a
  board swapped or readdressed under a running program.
- **Reads stay inside the payload.** `P` may be shorter than the kind's
  read payload, never longer: bytes past what the firmware sends are not
  data. A longer `P` is `Error::FrameLength`.
- **Writes are exact.** `W` must equal the kind's write length from
  `spec.rs`. The LED matrix has two lengths; the node accepts either and the
  device checks its tracked mode. A mismatch is `Error::FrameLength`, never a
  short frame on the wire.
- **Prefix guard.** A frame starting `C F` or `D I E` is refused with
  `Error::ReservedPrefix`. Only `service.rs` can send those, through a
  crate-private function. This changes behaviour on inputs other libraries
  accept (Knob position 17987, Buzzer 17987 Hz, some matrix frames); the
  alternative is that those inputs rewrite the board's address.
- **No write-to-read gap in v1.** The firmware handles writes in its main
  loop with one buffer shared by RX and TX (`main.c:476, 574-578`), so a read
  issued too soon after a write might corrupt it. This is unmeasured. If the
  bench shows it, `Node` gains an optional minimum gap; until then no
  `DelayNs` is needed on node devices. The latch relay is the exception:
  its handler re-reads the command byte between two 50 ms delays
  (`main.c:315-323`), and any transfer to the board meanwhile overwrites
  it from the I2C interrupt (`main.c:571-580`). That window is certain,
  not a race, so `LatchRelay::set` takes a `DelayNs` and waits it out.

### 4.4 Codecs (`codec/`)

Pure functions from values to bytes and back, one file per node kind. No bus,
no time, no state.

```rust
pub mod buzzer {
    pub const WRITE_LEN: usize = 8;
    pub fn encode(freq_hz: u32, duration_ms: u32) -> Result<[u8; WRITE_LEN], CodecError>;
}
pub mod knob {
    pub const READ_PAYLOAD: usize = 3;
    pub fn decode(p: &[u8; READ_PAYLOAD]) -> KnobRaw;   // { position: i16, pressed: bool }
}
```

Range checks live here: Buzzer below 180 Hz overflows the firmware's 16-bit
timer (`main.c:262-283`); Motors frequency is clamped to 200-60000 Hz
(`motors.c:613-657`). `WRITE_LEN` and `READ_PAYLOAD` are re-exported from
`spec.rs`, so a codec cannot disagree with the table.

This is the layer the language-neutral vectors bind to, and the one a port
to another language reproduces.

### 4.5 Chip drivers (`chips/`)

Minimal register drivers for what each module exposes, written in the crate:

| Chip | Module | Starting point | Notes |
|---|---|---|---|
| LSM6DSOX | Movement | `movement.rs` | Try 0x6A then 0x6B; WHO_AM_I 0x6C |
| VL53L4CD | Distance | ST's driver (stm32duino `VL53L4CD` at `b6ed0e2`) | Replaces `distance.rs`: bounded waits, timing computed from the oscillator |
| VL53L4ED | Distance | ST's driver (stm32duino `VL53L4ED` at `a94cc0b`) | Same register map and configuration; model ID 0xEC (not 0xEB), 100 ms default budget, no first-sample wait |
| HS3003 | Thermo | `hs3003` crate 0.1.1 | Replaced by a 60-line driver: the crate ignores the stale-sample status bits and waits a fixed 100 ms |
| SHT31 | Thermo | new, about 80 lines | Fallback at 0x45 (`Modulino.h:614-627`) |
| LTR381RGB | Light | `light.rs` | Trim to the lux/RGB path |
| LPS22HB | Pressure | `pressure.rs` | WHO_AM_I 0xB1 |

Rules for every chip driver:

- Takes an `I2c` and never reconfigures, re-clocks or ends the bus. Arduino's
  wrapped vendor libraries do, which is the source of `Wire.end()` and
  clock-reset bugs (`Modulino.h:716-719`, Arduino_Modulino issue #69).
- Every wait is bounded and returns `Error::Timeout`.
- `verify()` checks the ID register where the chip has one.
- Facts may come from vendor sources and datasheets. Code is not copied from
  the MPL-2.0 Arduino libraries.

Wrapping maintained `embedded-hal` 1.0 crates was considered and rejected:
the crates that exist expose more than the module needs, and each adds its
own error and delay conventions to reconcile. The HS3003 was the planned
exception, until the `hs3003` crate turned out not to read the status bits
that mark a stale sample, so it could neither refuse stale data nor time
out.

### 4.6 Devices (`devices/`)

One thin struct per module, same shape everywhere:

```rust
let mut buttons = Buttons::new(bus);                 // default address, no I/O
let mut buttons = Buttons::with_address(bus, addr);  // no I/O
buttons.verify()?;                                   // explicit identity check
let state = buttons.read()?;                         // Result, never a stale value
buttons.set_leds(true, false, true)?;
let bus = buttons.release();
```

- Constructors never do I/O, so a device can be built before the bus or hub
  is powered, and tests build devices without a bus.
- A getter never returns a value after a failed transfer, and cached state is
  updated only on success.
- Host-side state only where the protocol requires it:
  - Knob keeps its position offset on the host, as arduino-modulino-mpy does
    (`knob.py:38-54`). It never writes the firmware's set command, which
    sidesteps the old-firmware sign bug (`Modulino.h:448-462`) and the `CF`
    prefix hazard.
  - LED matrix tracks its mode, read from the board on `verify()` and updated
    on every mode change.
- Chip modules use the same shape. A module with variants finds its chip
  in `verify()` and keeps which one it found: `Thermo` takes an HS3003
  that acknowledges at 0x44 (it has no identity, as in Arduino's library)
  or else an SHT31 proven by its status CRC at 0x45; `Distance` chooses
  between `Vl53l4cd` and `Vl53l4ed` by model ID.

### 4.7 Scan (`scan.rs`)

```rust
pub fn scan<I: I2c>(bus: &mut I, out: &mut [Found]) -> Result<usize, Error<I::Error>>;
pub fn scan_except<I: I2c>(bus: &mut I, skip: &[Address], out: &mut [Found])
    -> Result<usize, Error<I::Error>>;
pub enum Found { Node(Address, Kind), Chip(Address, ChipKind), Hub(Address), Unknown(Address) }
```

No allocation; the caller supplies the buffer, and the returned count can
exceed its length, so truncation is visible. Scanning behind a hub is
calling `scan_except` on a `HubPort`, skipping that hub and any above it:
a TCA9548A stays addressable while a port is connected, so it would
otherwise be found through its own port. Only a NACK means absent; any
other bus error ends the scan.

Each address in `0x01..=0x77` is probed with a 1-byte read, not a
zero-length write, because some HALs (RP2040's I2C block is the known case)
refuse zero-length writes. Addresses that answer are classified by evidence:

1. **0x70-0x77**: hub check first. Write an odd port mask, read it back,
   and restore the byte the probe read whatever the read-back shows (with a
   same-address device on an already connected port, that byte is their
   wired-AND). A 1-byte write to a node is shorter than any node frame, so a
   node drops it, and a node's read-back is its strap byte, which is always
   even, so it can never echo the mask.
2. **A chip's datasheet address**: the chip's identity check from
   `spec/chips.toml` (an ID register, or the SHT31's status CRC). The HS3003
   has no readable identity and its first data byte can equal a strap byte,
   so anything at 0x44 is `Unknown`. The SHT3x NACKs reads with no data
   pending (SHT3x-DIS datasheet v7, p. 10), so at 0x45 a NACKed probe still
   runs the status check. A node moved to 0x45 drops the 2-byte status
   command (main.c:531-561) and answers with its strap byte first
   (main.c:476), so its reply can pass the status CRC by chance: a Knob
   strapped 0x74 at 1 in 256 counts. When the status starts with a strap
   value, the check reads once more and a node repeats its strap byte. The
   SHT3x is assumed to NACK that read, as it NACKs a read with no data
   after a measurement command; the datasheet does not say what a read
   after the status returns, so this is a bench item. The strap byte alone
   does not decide,
   because 0x04 and 0x28 fit the status register's defined bits
   (datasheet Table 18); every other strap needs reserved bit 14 or 12,
   which the datasheet gives as 0.
3. **Anywhere**: strap byte from the probe read maps to a node kind.
4. Otherwise `Unknown`. Never a guess from the address table.

One known ambiguity: LSM6DSOX's WHO_AM_I is 0x6C, which is also the Pixels
strap. A Pixels board moved to 0x6A/0x6B would be reported as Movement. It is
documented and has a test; a second distinguishing read is added if it
matters in practice.

### 4.8 Service (`service.rs`)

The only code that sends `CF`, `DIE` or general call.

- `set_address(node, new: NewAddress, delay)`: pads `CF` to the board's
  current write length (48 for a grayscale matrix), waits for the reset
  (500 ms default, as AddressChanger does), then confirms the strap at the
  new address. Returns the new `Node`.
- `reset_address(node, delay)`: `CF 0x00`, which the firmware treats as
  "back to the strap default" (`main.c:165-167`).
- `general_call_reset(bus, delay)`: `CF 0x00` to address 0x00, which resets
  every node on the bus. Named separately because it touches every board.
- Bootloader entry (`DIE`) and firmware update are deferred (section 9).

### 4.9 Errors

One error type for the crate:

```rust
pub enum Error<E> {
    I2c(E),
    InvalidAddress,
    OutOfRange,
    Timeout,
    DataError,
    WrongIdentity { expected: Kind, got: u8 },
    FrameLength { expected: usize, got: usize },
    WrongChip { expected: ChipKind, got: Option<u8> },
    ReservedPrefix,
}
```

`WrongIdentity` is a node whose strap byte belongs to another kind;
`WrongChip` is something at a chip's address that fails the chip's
identity check, with the identity byte where the check reads one.
Both 0.3-only variants are gone. 0.3 returned `DeviceNotFound` only for an
identity mismatch (Light, Movement, Pressure), which is now `WrongChip`; a
device that does not answer was already the bus error. `InvalidParameter`
became `OutOfRange` for values, `FrameLength` for an LED matrix frame of the
wrong length, and `None` from `Hub::port` for a port above 7; its pixel
coordinate checks went with the framebuffer API. `InvalidAddress` stays for the service module (M7), whose address-in-use
check is its likely first user. `DataError` stays, narrowed to a reply the
protocol does not define or a corrupted one: an LED matrix mode other than
`MON` or `GS4`, an SHT31 measurement whose CRC fails, or a VL53L4
oscillator or PLL calibration reading of 0. A `defmt` feature derives
`defmt::Format`, as today.

## 5. Decisions

Each records the choice, the alternative, and why.

| # | Decision | Alternative rejected | Reason |
|---|---|---|---|
| D1 | Rust, `no_std`, `embedded-hal` 1.0 | C core with FFI; Python | One source reaches MCUs, Linux and bridges; mock-bus testing exists |
| D2 | Restructure modulino-rs in place, keep its history | Clean rewrite | Its bus boundary, chip drivers and tests are reusable; layers are added, copies deleted |
| D3 | New crate name `modulino-driver`, version 0.1.0 | Continue `modulino` 0.x | The `modulino` crate belongs to upstream; the API breaks anyway |
| D4 | MIT OR Apache-2.0; no Arduino code copied | MPL-2.0 fork of an Arduino library | Firmware and Arduino libraries are MPL-2.0; facts are cited, code is not taken |
| D5 | Constructors do no I/O; `verify()` is explicit | Identity check in constructors | Construction before power-up and bus-free tests |
| D6 | Every node read checks the strap byte | Check only on discovery | Free, and catches a swapped or readdressed board |
| D7 | `HubPort` implements `I2c`, deselects every time | Caller-managed select; cached port | Routing cannot be forgotten; no stale cache |
| D8 | Prefix guard refuses `CF`/`DIE` data frames | Send whatever the caller gives | The firmware executes them as commands |
| D9 | Knob position offset on the host | Firmware set command | Avoids the old-firmware sign bug and the `CF` hazard |
| D10 | Own minimal chip drivers | Wrap general chip crates | Smaller, uniform errors and waits, no bus ownership; the `hs3003` crate, first kept, cannot report stale samples |
| D11 | Spec tables as data from day one; Rust tables hand-written and checked | Code generation now; no data at all | Section 6 |
| D12 | Scan probes with a 1-byte read over 0x01-0x77 | Zero-length write over 0x08-0x77 | RP2040 refuses zero-length writes; the latch relay is at 0x02 |
| D13 | No write-to-read gap until measured, except the latch relay's known 100 ms handler (4.3) | Conservative gap now | Avoids a `DelayNs` on every node device for an unconfirmed race |

## 6. Spec-readiness: keeping a formal spec and codegen open

The protocol is small enough today that code generation would cost more than
it saves. The design makes adopting it later a contained change, not a
restructure.

**Now:**

- `spec/nodes.toml` and `spec/chips.toml` hold every protocol constant: kind,
  strap bytes (from which default addresses follow), read payload and write lengths (per mode
  where they vary), the firmware image, and the firmware source lines each
  value came from. A `revision` field exists from the start, so a new frame
  variant (Buttons going from 3 to 4 bytes in firmware PR #28) is a new row,
  not a new concept.
- `spec/vectors/<kind>.toml` holds codec test vectors as language-neutral
  data: an operation, its arguments or input bytes, and the expected bytes,
  values or error. TOML rather than JSON, so one parser reads all of
  `spec/` and byte strings can be written in hex.
- `src/spec.rs` is written by hand and is the only place in Rust those
  constants appear. A test parses `spec/*.toml` (std, dev-dependency only)
  and fails on any disagreement with `spec.rs`.
- A CI job extracts the `NODE_*` constants and `getReceiveBufferSize()`
  lengths from `node_modulino_firmware` at a pinned commit and diffs them
  against `spec/nodes.toml`. Moving the pin is a deliberate commit, so
  firmware changes surface as a red check rather than silent drift. The
  extractor is deliberately narrow; if a firmware refactor breaks it, it is
  replaced by a reviewed manual check, not widened.

**Later, if wanted:** replace `src/spec.rs` with output of a `build.rs` or a
checked-in generator reading the same TOML. Nothing above `spec.rs` changes,
because codecs and the node layer take lengths from it already. A port to
another language reads the same TOML and runs the same vectors. Codecs could
also be generated, since they are pure and table-shaped; that stays a
separate decision.

What this rules out: constants written anywhere other than `spec.rs`, and
test vectors expressed only as Rust tables. Both are review rules, and the
conformance test enforces the first.

## 7. Verification

From cheapest to most expensive. Each rung proves only itself.

1. **Spec conformance.** `spec.rs` matches `spec/*.toml`; the firmware drift
   job matches `spec/nodes.toml` against the pinned firmware.
2. **Codec vectors.** Every file in `spec/vectors/` runs against the codecs:
   encode, decode and rejection cases, including Vibro off at 12 bytes and
   Buzzer 17987 Hz refused.
3. **Simulated bus.** `tests/support/` implements `I2c` with a `FakeNode`
   that follows the firmware's rules (strap byte first, short writes dropped
   silently, `CF` readdresses after a reset, general call), a `FakeHub`
   (TCA9548A), and `FakeChip` register maps with ID registers. Device
   round-trips, hub nesting and deselect-on-error, scan classification
   (including the hub/Vibro collision and the 0x02 latch relay), and
   `set_address` on a grayscale matrix are tested as behaviour, not as byte
   sequences.
4. **Exact sequences.** `embedded-hal-mock` for chip register sequences,
   where the order of register writes is the contract.
5. **Break-it check.** For each fix, break the guarded line and watch the
   named test fail. `cargo-mutants` over `node`, `codec` and `hub` in CI once
   the suite is stable.
6. **Cross-target build.** `cargo check` for `thumbv6m-none-eabi`,
   `thumbv7em-none-eabihf` and `riscv32imac-unknown-none-elf` (already in CI),
   plus clippy `-D warnings`, rustdoc and `cargo audit`.
7. **Bench.** A Linux host (Raspberry Pi or USB-I2C bridge) with
   `linux-embedded-hal`, one example per module, run against every module and
   the hub. The results table in the README records, per module, the highest
   rung reached. The bench also settles the open questions below.

### Open questions the bench settles

| Question | Why it matters | Test |
|---|---|---|
| What does a node do with bytes beyond its write length? | The HAL source says the first bytes are processed; the rest may hold SCL low until the board resets | Send N+1, then N+2 bytes to Buttons with a power cycle ready; check state and the bus trace |
| Is there a write-then-read race on the shared buffer? | Decides D13 | LED matrix frame immediately followed by a mode read; compare the display |
| Is a short write really dropped? | Confirms the Vibro `off()` bug | Vibro on for 5 s, send the 8-byte off, observe |
| Does the prefix hazard reproduce? | Confirms D8 is protecting against something real | Buzzer at 17987 Hz through a raw write, then scan: the board should come back at its strap default |
| Which firmware revision do shipped boards run? | Revision rows, Knob behaviour | Read strap and frame lengths from every board on arrival |
| Which Distance and Thermo variants arrive? | VL53L4ED and SHT31 paths | Read the ID registers; scan finds an SHT31 at 0x45 |
| Does Distance range correctly after `init`? | The configuration and timing are ported from ST's source, never run | Range a flat target at a measured distance; expect status 0 and a reading within the datasheet's accuracy |
| Does the Linux host report a NACK as `NoAcknowledge`? | Scan treats any other error as fatal | Scan an empty bus through `linux-embedded-hal` |

Flashing a board or changing an address on the bench is an ask-first step.

## 8. Milestones

Each one leaves `main` green and usable. Tracked as sub-issues of one epic.

| # | Milestone | Ships |
|---|---|---|
| M0 | Rebrand and gate | Crate renamed, README with attribution and corrected module table (the LED matrix is STM32-driven, not IS31FL3733), justfile, AGENTS.md, CI kept green |
| M1 | Buttons end to end on the new core | `spec/nodes.toml`, `src/spec.rs` and its conformance test; `addr.rs`; `node.rs` with identity check, exact length and prefix guard; new `Error`; `FakeNode`; Buttons rebuilt on them with codec and vectors, proving every layer once |
| M2 | Remaining node devices | Buzzer, Vibro, Knob, Pixels, Joystick, Opto relay, Latch relay on `Node`, with codecs and vectors; per-driver `discover()` deleted; Vibro off fixed |
| M3 | LED matrix and Motors | Mode-tracked matrix (12/48), Motors commands and status, vectors |
| M4 | Hub routing | `HubPort` as `I2c`, nested ports, deselect on error, `FakeHub` |
| M5 | Scan | Evidence-based classification over 0x01-0x77, hub check, collision tests |
| M6 | Chip modules | `chips/` with ID checks and bounded waits; Movement 0x6B; Thermo SHT31; Distance VL53L4ED (`spec/chips.toml` arrived with M5) |
| M7 | Service | `set_address`, `reset_address`, `general_call_reset` |
| M8 | Firmware drift check | Pinned-commit extraction job in CI |
| M9 | Bench and 0.1.0 | Bench run on every module, open questions answered in `spec/`, results table, release |

M1-M8 need no hardware. The boards arrive before M9.

## 9. Deferred

Each waits for a stated need:

- Async API behind a feature.
- Multi-task `HubPort` (critical-section or mutex).
- Bootloader entry and firmware update (ST AN4221).
- Generated `spec.rs` and codecs (section 6).
- Ports to other languages against `spec/`.
- Upstream contributions: bug reports to Arduino (Vibro off, prefix hazard,
  AddressChanger padding) and fixes offered to modulino-rs. Drafted for
  review before anything is posted.
