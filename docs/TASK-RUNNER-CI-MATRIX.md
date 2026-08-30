# Task runner verification matrix

This matrix records what each recipe proves, what it writes, and which
prerequisites it needs. It is useful when choosing one command during local
iteration and when reviewing whether `just ci` covers the repository's
verification surface.

| Recipe | Real command | Reads | Writes | Network or secrets |
| --- | --- | --- | --- | --- |
| `build` | `cargo build` | Rust source and lockfile | ignored `target/` files | none |
| `test` | `cargo test` | source, fixtures, lockfile | ignored `target/` files | none |
| `wasm-release` | package-specific release Cargo build | contract source and lockfile | ignored release wasm | none |
| `stellar-build` | `stellar contract build` | project manifest and contract source | CLI build output | none |
| `format` | `cargo fmt --all` | Rust source and config | Rust source files | none |
| `fmt-check` | `cargo fmt --all -- --check` | Rust source and config | none | none |
| `lint` | `cargo clippy --all-targets` | source and lockfile | ignored `target/` files | none |
| `tools` | executable lookup | `PATH` | none | none |
| `check` | dependency chain | all inputs above | ignored build output | none |
| `ci` | `check` alias | all inputs above | ignored build output | none |

## Exit-status contract

The task runner preserves the exit status from every underlying command. A
zero status means the wrapped command completed successfully. A non-zero status
means the caller should inspect the original command's output; the runner does
not turn a failed compiler, test, formatter, or linter into a success.

The `check` recipe uses dependencies rather than a custom shell loop. This
provides a visible order and stops immediately when a prerequisite fails. The
order is:

1. `test`
2. `wasm-release`
3. `stellar-build`
4. `fmt-check`
5. `lint`

Tests run first because a failing test makes later artifact checks less useful.
The release build runs before the Stellar CLI flow because the executable
artifact is the most direct compiler signal. Formatting and linting run last
and remain independently available when a contributor wants fast feedback.

## CI job translation

A CI provider can invoke one command:

```sh
just ci
```

Or it can expose each recipe as a separate job while preserving the same
commands:

```text
job: tests          -> just test
job: wasm           -> just wasm-release
job: stellar-build  -> just stellar-build
job: formatting     -> just fmt-check
job: lint           -> just lint
```

The second form can make failures easier to identify in a provider UI, while
the first form is the canonical local reproduction. Neither form should add
deployment credentials or a network-dependent step to the task runner.

## What the matrix does not promise

The recipes do not promise that pre-existing repository debt is absent. For
example, a formatting or clippy task may report findings that existed on the
base branch. That is still a valid result from the real command. The runner
does not hide warnings, pin a private toolchain, or modify source to make a
check green.

The recipes also do not replace security review. A passing test and a valid
wasm build are necessary engineering signals, not an audit or a deployment
approval. Signing keys, contract IDs, network configuration, and treasury
operations intentionally remain outside this local task map.

## Review scenarios

### New contributor

Run `just` to discover the commands, `just tools` to check prerequisites, and
`just test` for a first feedback loop. Continue with `just ci` before opening a
pull request.

### Contract logic change

Run `just test`, `just wasm-release`, and `just stellar-build` while iterating.
Run `just ci` before requesting review. If the change affects metadata, also
run `scripts/inspect-contract-metadata.sh --no-build` against the resulting
artifact.

### Documentation-only change

Use `just fmt-check` only if Rust files changed. A full `just ci` remains the
most reproducible PR check and confirms the checkout is not accidentally
dependent on an untracked local artifact.

### Existing check failure

Run the individual recipe on the base commit, record the output, apply the
change, and rerun it. This distinguishes a pre-existing warning from a
regression without changing the task runner's behavior.

## Maintenance rules

- Keep recipe names stable once documented in the README.
- Keep one-line descriptions useful in `just --list`.
- Show the complete underlying command in the `justfile`.
- Do not add flags that change production behavior for convenience.
- Do not add hidden environment variables or secret lookups.
- Update this matrix when the `check` dependency chain changes.
- Keep command examples copy-pasteable from the repository root.
- Run `just --list` after editing the file.
- Check `git diff --check` before committing documentation changes.
- Explain compatibility changes in the PR rather than silently changing aliases.

The matrix is intentionally explicit so a future maintainer can compare the
task runner with CI without reverse engineering shell scripts.
