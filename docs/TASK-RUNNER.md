# Contributor task runner

This repository uses [`just`](https://github.com/casey/just) as a small command
index. It gives contributors one discoverable place for the commands that are
already used to build, test, lint, and inspect the Soroban contract. The
`justfile` does not replace Cargo or the Stellar CLI and does not alter their
arguments.

## Install the runner

Install `just` using the package manager appropriate for your workstation. The
following are examples; use the distribution's normal package policy when a
different installation method is required:

```sh
# macOS with Homebrew
brew install just

# Rust toolchain installation
cargo install just

# Debian/Ubuntu when the distribution package is available
sudo apt install just
```

Confirm that the executable is on `PATH`:

```sh
just --version
```

The repository also requires Cargo and the Rust target used by the contract.
The `stellar` command is needed for `stellar-build` and `check`, while
`rustfmt` and `clippy` are normally installed through `rustup component add`.

```sh
rustup component add rustfmt clippy
rustup target add wasm32-unknown-unknown
cargo --version
stellar --version
```

The task runner does not install tools automatically. A command map should be
safe to run in a controlled CI environment and must not unexpectedly mutate a
developer's toolchain.

## Discover the commands

Running `just` with no arguments invokes the default recipe, which delegates to
`just --list`. The output includes each public task and its one-line
description:

```text
Available recipes:
    build          # Compile the workspace in the default profile.
    check          # Run every local verification command in one reproducible invocation.
    ci             # Alias for CI-oriented language used in contributor documentation.
    fmt-check      # Check formatting without changing files.
    format         # Format all workspace Rust files using rustfmt's normal behavior.
    lint           # Run clippy with the same all-targets scope used by contributors.
    list           # Print the available task list.
    stellar-build  # Run the Stellar CLI's contract build flow.
    test           # Run the complete Rust test suite.
    tools          # Verify that expected local tools are available.
    wasm-release   # Produce the optimized contract artifact used for release review.
```

The exact ordering can change with a future `just` version, but the recipe
names and descriptions are kept intentionally short so the output remains a
useful first stop for a new contributor.

## Command reference

### `just build`

Runs:

```sh
cargo build
```

This is the normal debug-profile workspace build. It is useful for quick
compiler feedback and does not produce the optimized deployment artifact.

### `just test`

Runs:

```sh
cargo test
```

This is the full workspace test suite. It includes the escrow contract's unit
tests and the pinned Merkle vectors. Run it after changes to contract logic,
storage, or test fixtures.

### `just wasm-release`

Runs:

```sh
cargo build --package grainhack-escrow \
  --target wasm32-unknown-unknown --release
```

This is the explicit release build for the contract package. Its output is
normally `target/wasm32-unknown-unknown/release/grainhack_escrow.wasm`.
Metadata review should inspect this file with the helper documented in
`docs/CONTRACT-METADATA.md`.

### `just stellar-build`

Runs:

```sh
stellar contract build
```

This delegates to the Stellar CLI's project build flow. It is kept separate
from `wasm-release` because the two commands are useful at different points in
the developer workflow and the CLI may apply its own project conventions.

### `just format`

Runs:

```sh
cargo fmt --all
```

This may modify Rust files. Review the resulting diff before committing. The
task intentionally has no extra flags, so it follows rustfmt's repository and
toolchain configuration.

### `just fmt-check`

Runs:

```sh
cargo fmt --all -- --check
```

This is the non-mutating formatting check. It can report existing formatting
debt on `main`; a failure still accurately reports the real command's result.
The task runner does not silently format files or hide warnings.

### `just lint`

Runs:

```sh
cargo clippy --all-targets
```

This invokes clippy across library, binary, integration-test, and example
targets. The repository may have known warnings that are being handled in
separate work. The recipe preserves the command and its exit status.

### `just check` and `just ci`

`just check` runs the complete local verification chain:

```text
test -> wasm-release -> stellar-build -> fmt-check -> lint
```

The dependency syntax lets `just` stop at the first failed prerequisite and
avoids running later checks against an artifact that did not compile. `just
ci` is an alias for `check` so a contributor can use either term when matching
the repository's CI vocabulary.

The chain is intentionally explicit. It does not include deployment, signing,
network access, secret loading, or release publication. A local CI task must
be safe to run without credentials.

### `just tools`

Runs lightweight executable checks for Cargo, rustfmt, the Stellar CLI, and
just itself. It does not verify versions or install anything. Use it before a
long check when setting up a new workstation.

### `just versions`

Runs the version commands for Rust, Cargo, rustfmt, the Stellar CLI, and just.
Use it when recording a validation report or comparing a local result with CI:

```sh
just versions
```

The task reports versions only; it does not select, install, or modify a
toolchain. Keeping the output in a PR makes differences in compiler or CLI
behavior easier to investigate.

## Running individual recipes

Every recipe can be run independently:

```sh
just build
just test
just wasm-release
just stellar-build
just format
just fmt-check
just lint
just check
```

Running a single recipe is useful while iterating. Before opening a PR, run
the complete `just check` chain and record any environment limitation in the
PR description.

## Clean-checkout verification

The task runner is intended to work from a clean checkout. A maintainer or CI
operator can verify that property without changing tracked files:

```sh
git status --short
just tools
just test
just wasm-release
just stellar-build
just fmt-check
just lint
git status --short
```

The last status check should show no task-runner-generated tracked changes.
Build directories under `target/` are ignored and may be present. `format` is
the one intentional exception: it writes formatting changes by design.

## CI reproduction

When CI reports a failure, first identify the failed recipe from the job log.
Re-run that recipe locally to shorten the feedback loop, then run `just ci`
before pushing the fix. Examples:

```sh
# Formatting failure
just fmt-check

# Test failure
just test

# Wasm or contract-build failure
just wasm-release
just stellar-build

# Full reproduction
just ci
```

Because the recipes preserve the repository's real commands, a task failure
should be actionable without learning a second set of flags.

## Why `just` instead of a custom wrapper

The file is deliberately declarative and small. `just` provides a built-in
recipe list, dependency ordering, and predictable shell execution without
requiring contributors to remember make's tab-sensitive syntax. The commands
remain visible in the file, and each recipe has one responsibility.

No recipe calls an alias that hides a compiler flag. No recipe modifies source
files except `format`. No recipe contacts a network or uses credentials. These
constraints keep the command map a convenience layer rather than a second
build system.

## Troubleshooting

### `just: command not found`

Install `just`, confirm it is on `PATH`, and run `just --version`. The
repository does not vendor a platform-specific binary.

### The wasm target is missing

Install it with `rustup target add wasm32-unknown-unknown`, then rerun
`just wasm-release`. The runner reports Cargo's original error unchanged.

### `stellar-build` cannot find a CLI

Install the Stellar CLI and verify `stellar --version`. `just tools` performs a
presence check but intentionally does not claim that every CLI version is
compatible with the project.

### `fmt-check` reports hunks

This task is a check, not a formatter. Run `just format` only when formatting
changes are in scope, review the diff, and then rerun `just fmt-check`.

### `lint` reports warnings

The task invokes clippy with `--all-targets` and preserves its output. Known
warnings on `main` are not silently filtered. Fix warnings in a dedicated
issue or record why they are outside the current change.

### `check` stops early

That is expected. `just` dependencies stop when a prerequisite fails, so the
later tasks do not obscure the first failure. Run the failed recipe directly
after correcting the cause, then rerun `just check`.

## Maintainer checklist

- [ ] `just` with no arguments lists available recipes.
- [ ] Every required command is represented by a named recipe.
- [ ] Recipe descriptions state what each command does.
- [ ] The recipes invoke the real commands without behavior-changing flags.
- [ ] `check` covers tests, wasm release build, Stellar build, format check,
      and lint.
- [ ] `ci` reproduces the same complete local chain.
- [ ] Tooling prerequisites are documented and are not auto-installed.
- [ ] No recipe deploys, signs, or reads secrets.
- [ ] The README points contributors to the task list.
- [ ] A clean-checkout run leaves tracked files unchanged.

This checklist is documentation for maintainers; it is not a replacement for
the commands themselves.
