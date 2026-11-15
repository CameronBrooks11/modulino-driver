# Bench checklist

The bench rung of design section 7: every module on real hardware from a
Linux host, and the open protocol questions answered by measurement. Run it
on a Raspberry Pi with the boards on its I2C header (Qwiic, 3.3 V).

Each step names a `bench` subcommand (`examples/bench.rs`) or an
`i2ctransfer` command, what to look for, and where the result goes. Record
the observed result, not the expected one; a step that does not match its
`expect:` line is a finding, and gets an issue.

## Setup

1. Enable I2C (`sudo raspi-config`, Interface Options, I2C), reboot, and
   check `/dev/i2c-1` exists. Install `i2c-tools` for `i2cdetect` and
   `i2ctransfer`.
2. Install Rust 1.85 or later with rustup (<https://rustup.rs>). The
   example builds the crate's dev-dependencies, which need 1.85; the
   library alone needs 1.75. Raspberry Pi OS Bookworm's apt `rustc` is
   1.63, too old. Then, on the Pi, in a clone of this repository:
   `cargo build --example bench` (no cross-compiling needed).
3. Run each command as
   `cargo run --example bench -- <command> [address]`. The bus is
   `/dev/i2c-1`; set `MODULINO_I2C` for another.
4. Note at the top of the results: the Pi model, OS, kernel (`uname -r`),
   the I2C adapter's driver
   (`readlink /sys/bus/i2c/devices/i2c-1/../driver`), the bus speed if
   changed from the default, and the commit of this repository.

Connect one board at a time unless a step says otherwise; two boards of a
kind share a default address.

Each command sends its actuator's stop (motors, opto relay, LEDs, buzzer,
vibration) before it exits, even when a step fails. If that stop fails too,
the command says so, and the board may still be running: power it off. The
latch relay is the exception: it holds whatever state it was last set to.
Ctrl-C sends no stop: after a Ctrl-C, run the same command again to
completion, or power the board off.

If reads come back corrupted or time out, try a slower bus
(`dtparam=i2c_arm_baudrate=50000` in the boot config). The node firmware
enables clock stretching (`main.c:656` at `7d578e0`), so the boards may
stretch the clock, which the Pi 1-4's I2C controller is known to handle
poorly. That is a suspicion to test, not a known fault, so record it either
way.

## 1. The host

| Step | Command | Look for |
|---|---|---|
| 1.1 | `nack` (nothing connected) | The `errno`, `host kind` and `bench kind` lines. This answers "Does the Linux host report a NACK as `NoAcknowledge`?" |
| 1.2 | `i2cdetect -y 1` with all boards on the bus | The addresses present, to compare with 2.1 |

What 1.1 shows depends on the adapter driver noted in setup step 4. From
the kernel sources (Linux v6.12; the Pi 5 row from Raspberry Pi's
`rpi-6.12.y`, where RP1's I2C is `snps,designware-i2c`, `rp1.dtsi:313-335`):

| Driver | Used by | Unacknowledged address | linux-embedded-hal 0.5 reports |
|---|---|---|---|
| `i2c-bcm2835` | Pi 1-4, Zero | `EREMOTEIO` (121), `i2c-bcm2835.c:381-382` | `Other` |
| `i2c-designware` | Pi 5 (RP1) | `EREMOTEIO` (121), `i2c-designware-common.c:707-711` | `Other` |
| `i2c-gpio` (bit-banged overlay) | any Pi, if configured | `ENXIO` (`i2c-algo-bit.c:486, 493, 514`); `EIO` for a data NACK (`367, 505, 568, 579`) | `NoAcknowledge`; `Bus` for a data NACK |

The `bench` bus maps `EREMOTEIO` to a NACK so the device steps do not depend
on this. If 1.1 shows `host kind: Other`, then `scan`, the address-change
checks and the SHT31 retry fail on a plain `I2cdev` at the first NACK, and
the README needs to say so.

## 2. Discovery

| Step | Command | Look for |
|---|---|---|
| 2.1 | `scan` with every board connected (one of each kind) | One line per board: `Node(addr, Kind)` with its strap byte, `Chip(addr, ChipKind)`, `Hub(addr)`. The Thermo's HS3003 shows as `Unknown(0x44)`. Record every strap byte. With section 3, where each driver sends its board's exact write length, this answers "Which firmware revision do shipped boards run?" for the lengths the spec pins. The strap byte alone tells the Knob's two straps apart and nothing more |
| 2.2 | `hub` with only the hub on the Pi's bus and a board on two of its ports | Each port lists its own boards and no others. A board wired to the Pi directly answers on every port |

## 3. Modules

One row per module. Each command prints `expect:` lines; record what
happened.

| Module | Command | Notes |
|---|---|---|
| Buttons | `buttons` | |
| Buzzer | `buzzer` | |
| Vibro | `vibro` | `off()` must stop a 5 s buzz after 1 s |
| Knob | `knob` | Record which strap (0x74 or 0x76) from 2.1 |
| Pixels | `pixels` | |
| Joystick | `joystick` | Record which direction gives +x and which gives +y; nothing in the sources fixes them |
| Opto relay | `opto-relay` | |
| Latch relay | `latch-relay` | Default address 0x02 |
| LED matrix | `led-matrix` | Mono border, then grayscale ramp; record which side is full brightness. Leaves the board in mono mode, which 4.3 needs |
| Motors (DC) | `motors` | Motors free to turn |
| Motors (stepper) | `motors-stepper` | A stepper on the outputs |
| Thermo | `thermo` | Record the chip: HS3003 or SHT31. Answers "Which Distance and Thermo variants arrive?" |
| Distance | `distance` | Record the variant. A flat target at a measured distance: answers "Does Distance range correctly after `init`?" |
| Movement | `movement` | Record the address: 0x6A or 0x6B |
| Pressure | `pressure` | |
| Light | `light` | |

## 4. Open protocol questions

These send frames the drivers refuse to, so they are `i2ctransfer`
commands, not driver calls. `cargo run --example bench -- frames` prints
them with the bytes from the library's encoders; run them one at a time.

| Step | Question | Commands (`frames` section) | Look for |
|---|---|---|---|
| 4.1 | Is a short write really dropped? | "Short write": Vibro on for 60 s, then the 8-byte off | The motor keeps running after the 8-byte off, and stops on the full 12-byte off. Both offs must come within the 60 s |
| 4.2 | What does a node do with bytes beyond its write length? | "Over-length write": LED A as 4 bytes, then LED B as 5 bytes, each followed by a read | After the 4-byte write, whether LED A lights; after the 5-byte write, whether LED B lights and A goes out. Record each `i2ctransfer`'s exit status: the extra byte may be NACKed, a third outcome. Whether each read succeeds or the bus hangs (power-cycle ready). Then `i2cdetect -y 1` |
| 4.3 | Is there a write-then-read race on the shared buffer? | "Write-then-read race": the matrix (in mono mode, as `led-matrix` leaves it) gets a border and a read in one transfer | The matrix shows the full border. In grayscale mode the 12-byte frame would be dropped as short, which looks like a race. The read is a sanity check: it returns the strap byte and `MON` whether or not there is a race. A clean result at the bus speed in setup step 4 says nothing about faster speeds |
| 4.4 | Does the prefix hazard reproduce? | "Prefix hazard": Buzzer at 17987 Hz. **Ask first**: the board erases any stored address and reboots | The Buzzer does not sound; `scan` afterwards finds it at its strap default |

Changing an address on purpose (`service::set_address`) and the general-call
reset are not on this list; each is a separate ask-first step.

## Results

Copy this table into the results, one row per step.

| Step | Board / variant | Result | Matches `expect:` | Issue |
|---|---|---|---|---|
| 1.1 | | | | |
