# modulino-driver task runner. `check` and `test` cover the CI rust-checks job;
# `lint` also runs clippy on tests and examples, which CI does not. The
# cross-target builds and the MSRV (1.75) check run in CI only.

set dotenv-load := false

# Show available recipes
default:
    @just --list

# Install toolchain components
setup:
    rustup component add rustfmt clippy

# Format code (mutates the working tree)
fmt:
    cargo fmt --all

# Verify formatting
fmt-check:
    cargo fmt --all -- --check

# Run clippy with warnings as errors
lint:
    cargo clippy --all-targets --all-features --workspace -- -D warnings

# Build the docs, failing on rustdoc warnings
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Format check, lint and docs (CI-equivalent)
check: fmt-check lint doc

# Run tests
test:
    cargo test --workspace

# Regenerate tests/readme_examples.rs from the README's Rust blocks
readme:
    README_UPDATE=1 cargo test --test readme_sync
