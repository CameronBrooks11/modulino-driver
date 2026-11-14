# Examples

Host examples. Each runs with `cargo run --example <name>` against a
stand-in I2C bus, so it shows how the API fits together without hardware.

| Example | What it shows |
|---|---|
| `multiplexer` | Two Buttons modules at the same address on two ports of a Modulino Hub. Each `HubPort` selects its port for one transaction and deselects after, so the drivers need no hub code. |

```sh
cargo run --example multiplexer
```

## On hardware

Every driver takes any bus that implements `embedded_hal::i2c::I2c` (1.0),
and the drivers that wait take an `embedded_hal::delay::DelayNs`. To run an
example on a board, replace its stand-in bus (`DummyI2c`) with your HAL's
I2C peripheral; on a Linux host, `linux-embedded-hal` provides one, and the
bench plan in [docs/design.md](../docs/design.md) section 7 uses it. No
module has been verified on hardware yet.
