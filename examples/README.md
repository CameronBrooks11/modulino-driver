# Examples

Host examples, run with `cargo run --example <name>`. `multiplexer` runs
against a stand-in I2C bus, so it shows how the API fits together without
hardware; `bench` needs real boards on a Linux host.

| Example | What it shows |
|---|---|
| `bench` | Real boards from a Linux host: one subcommand per module, plus `scan`, `hub`, `nack` (what the host reports for an unacknowledged read) and `frames` (the raw probes for the open protocol questions, printed, not sent). Linux only; [docs/bench.md](../docs/bench.md) is the checklist that runs it. |
| `multiplexer` | Two Buttons modules at the same address on two ports of a Modulino Hub, each driven through a `HubPort`. A `HubPort` selects its port for one transaction and deselects after, so the drivers need no hub code. The stand-in bus accepts everything and prints nothing, so the run shows the code compiles and runs, not the bus traffic. |

```sh
cargo run --example multiplexer
cargo run --example bench -- buttons   # on a Raspberry Pi, a Buttons board on I2C 1
```

## On hardware

Every driver takes any bus that implements `embedded_hal::i2c::I2c` (1.0),
and the drivers that wait take an `embedded_hal::delay::DelayNs`. To run
`multiplexer` on a Linux host (a Raspberry Pi, or a USB-I2C bridge), replace
its stand-in bus (`DummyI2c`) and error type with a real I2C bus, such as
the one `linux-embedded-hal` provides, as `bench` does. On a microcontroller
it also needs your HAL's entry point and a `no_std` setup. No module has
been verified on hardware yet.
