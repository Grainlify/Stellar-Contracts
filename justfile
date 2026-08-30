# Contributor command map for Grainlify Stellar Contracts.
#
# Recipes intentionally wrap the repository's real commands. The task runner
# is an ergonomic index and does not introduce different compiler flags,
# feature flags, targets, or deployment behavior.

set shell := ["bash", "-euo", "pipefail", "-c"]

package := "grainhack-escrow"
wasm_target := "wasm32-unknown-unknown"

# `just` with no recipe prints this stable, contributor-facing task list.
default: list

list:
    @just --list

# Compile the workspace in the default profile.
build:
    cargo build

# Run the complete Rust test suite.
test:
    cargo test

# Produce the optimized contract artifact used for release review.
wasm-release:
    cargo build --package {{package}} --target {{wasm_target}} --release

# Run the Stellar CLI's contract build flow.
stellar-build:
    stellar contract build

# Format all workspace Rust files using rustfmt's normal behavior.
format:
    cargo fmt --all

# Check formatting without changing files.
fmt-check:
    cargo fmt --all -- --check

# Run clippy with the same all-targets scope used by contributors.
lint:
    cargo clippy --all-targets

# Run every local verification command in one reproducible invocation.
check: test wasm-release stellar-build fmt-check lint

# Alias for CI-oriented language used in contributor documentation.
ci: check

# Verify that the expected local tools are available before a long run.
tools:
    @command -v cargo >/dev/null || { echo "missing: cargo" >&2; exit 1; }
    @command -v rustfmt >/dev/null || { echo "missing: rustfmt" >&2; exit 1; }
    @command -v stellar >/dev/null || { echo "missing: stellar CLI" >&2; exit 1; }
    @command -v just >/dev/null || { echo "missing: just" >&2; exit 1; }
    @echo "cargo, rustfmt, stellar, and just are available"

# Print toolchain versions so CI and PR validation reports are reproducible.
versions:
    @rustc --version
    @cargo --version
    @rustfmt --version
    @stellar --version
    @just --version
