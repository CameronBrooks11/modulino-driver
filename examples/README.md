# Examples

Host examples. Each runs with `cargo run --example <name>` against a
stand-in I2C bus, so it shows how the API fits together without hardware.

| Example | What it shows |
|---|---|
| `multiplexer` | Two Buttons modules at the same address on two ports of a Modulino Hub, each driven through a `HubPort`. A `HubPort` selects its port for one transaction and deselects after, so the drivers need no hub code. The stand-in bus accepts everything and prints nothing, so the run shows the code compiles and runs, not the bus traffic. |

```sh
cargo run --example multiplexer
```

## On hardware

Every driver takes any bus that implements `embedded_hal::i2c::I2c` (1.0),
and the drivers that wait take an `embedded_hal::delay::DelayNs`. To run an
example on a Linux host (a Raspberry Pi, or a USB-I2C bridge), replace its
stand-in bus (`DummyI2c`) and error type with a real I2C bus, such as the one
`linux-embedded-hal` provides; the bench plan in
[docs/design.md](../docs/design.md) section 7 uses it. On a microcontroller
the examples also need your HAL's entry point and a `no_std` setup. No
module has been verified on hardware yet.
