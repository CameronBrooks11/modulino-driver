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
2. On the Pi, in a clone of this repository:
   `cargo build --example bench` (no cross-compiling needed).
3. Run each command as
   `cargo run --example bench -- <command> [address]`. The bus is
   `/dev/i2c-1`; set `MODULINO_I2C` for another.
4. Note the Pi model, OS, kernel (`uname -r`) and the commit of this
   repository at the top of the results.

Connect one board at a time unless a step says otherwise; two boards of a
kind share a default address.

## 1. The host

| Step | Command | Look for |
|---|---|---|
| 1.1 | `nack` (nothing connected) | The `errno`, `host kind` and `bench kind` lines. This answers "Does the Linux host report a NACK as `NoAcknowledge`?" |
| 1.2 | `i2cdetect -y 1` with all boards on the bus | The addresses present, to compare with 2.1 |

The Pi's driver (i2c-bcm2835) is expected to return `EREMOTEIO` (121) for an
unacknowledged transfer, which linux-embedded-hal 0.5 reports as `Other`.
The `bench` bus maps `EREMOTEIO` to a NACK so the device steps do not depend
on this. If 1.1 shows `host kind: Other`, then `scan`, the address-change
checks and the SHT31 retry fail on a plain `I2cdev` at the first NACK, and
the README needs to say so.

## 2. Discovery

| Step | Command | Look for |
|---|---|---|
| 2.1 | `scan` with every board connected (one of each kind) | One line per board: `Node(addr, Kind)` with its strap byte, `Chip(addr, ChipKind)`, `Hub(addr)`. The Thermo's HS3003 shows as `Unknown(0x44)`. Record every strap byte: it answers "Which firmware revision do shipped boards run?" |
| 2.2 | `hub` with the hub connected and a board on two ports | Each port lists its own boards and no others |

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
| Joystick | `joystick` | Axis signs: right is +x, up is +y |
| Opto relay | `opto-relay` | |
| Latch relay | `latch-relay` | Default address 0x02 |
| LED matrix | `led-matrix` | Mono border, then grayscale ramp |
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

| Question | Commands (`frames` section) | Look for |
|---|---|---|
| Is a short write really dropped? | "Short write": Vibro on, then the 8-byte off | The motor keeps running after the 8-byte off, and stops on the full 12-byte off |
| What does a node do with bytes beyond its write length? | "Over-length write": Buttons with 4, then 5 bytes | Whether LED A lights; whether the read after it succeeds or the bus hangs (power-cycle ready). Then `i2cdetect -y 1` |
| Is there a write-then-read race on the shared buffer? | "Write-then-read race": matrix frame and mode read in one transfer | The matrix shows the full border, and the read returns `MON` (0x4D 0x4F 0x4E) after the strap byte |
| Does the prefix hazard reproduce? | "Prefix hazard": Buzzer at 17987 Hz. **Ask first**: the board writes its flash and reboots | The Buzzer does not sound; `scan` afterwards finds it at its strap default |

Changing an address on purpose (`service::set_address`) and the general-call
reset are not on this list; each is a separate ask-first step.

## Results

Copy this table into the results, one row per step.

| Step | Board / variant | Result | Matches `expect:` | Issue |
|---|---|---|---|---|
| 1.1 | | | | |
