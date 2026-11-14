# AGENTS.md

Instructions for AI coding agents working in this repository.

## Project

`modulino-driver` is a layered, hardware-agnostic `no_std` Rust driver for
the Arduino Modulino boards over `embedded-hal` 1.0 I2C. It started from
modulino-rs 0.3.0 and is being restructured per
[docs/design.md](docs/design.md); the milestones there are the work queue.

## Stack

- **Language**: Rust, edition 2021, `no_std` library
- **Interfaces**: `embedded-hal` 1.0 `I2c` and `DelayNs`
- **Tests**: `cargo test`, `embedded-hal-mock`
- **CI**: fmt, clippy `-D warnings`, docs, tests, `cargo check` for
  `thumbv6m-none-eabi`, `thumbv7em-none-eabihf`, `riscv32imac-unknown-none-elf`,
  `cargo check --lib` at the declared MSRV (1.75), `cargo audit`

## Layout

```
src/              library (layers in docs/design.md section 3)
tests/            integration tests, one file per module
examples/         host examples
docs/design.md    architecture, decisions, milestones
```

## Commands

```sh
just setup        # rustfmt + clippy components
just fmt          # format
just check        # fmt-check + clippy + docs (CI-equivalent)
just test         # run tests
```

## Conventions

- Every protocol fact (address, strap byte, frame length, byte layout) cites
  its source: firmware file and line at a pinned commit, or a datasheet.
- Protocol constants live in one place (design section 6); do not repeat them
  in device code.
- Constructors do no I/O. Every fallible operation returns `Result`; cached
  state changes only after a successful transfer.
- A fix comes with a test that fails when the fix is reverted.

## Constraints

- The library stays `no_std` with no allocation; `std` is for tests only.
- Do not copy code from the MPL-2.0 Arduino libraries or firmware; cite facts.
- Do not add dependencies without justification.
- Changing a board's address, flashing firmware, or publishing to crates.io
  needs explicit approval.
- Do not commit secrets or credentials.
- Do not add AI attribution to commits or PR descriptions: no co-author
  trailers, session links, or "generated with" footers.
- Do not let a check report success when it could not run.
