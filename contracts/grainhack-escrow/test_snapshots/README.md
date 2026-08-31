# `test_snapshots/` — what these files are and how to handle them

This directory contains **Soroban test snapshots** for the `grainhack-escrow` contract. They are JSON files that record the observable state of the Soroban test environment at the end of each test: ledger entries, authorization invocations, and emitted events.

## What writes these files

The snapshots are written by the **Soroban SDK** (`soroban-sdk`) when a test `Env` is dropped and `capture_snapshot_at_drop` is enabled in `EnvTestConfig`. The SDK uses the Rust test name to derive the file path, turning module separators (`::`) into directories. For example, a test named `test::a_claim_verifies_through_a_promoted_node_in_an_odd_tree` produces:

```text
test_snapshots/test/a_claim_verifies_through_a_promoted_node_in_an_odd_tree.1.json
```

The trailing number (`1`, `2`, …) is incremented for each distinct `Env` created inside that test.

## Generated or hand-maintained?

**Generated.** These files are not edited by hand. The current test suite explicitly disables automatic snapshot capture:

```rust
env.set_config(EnvTestConfig {
    capture_snapshot_at_drop: false,
});
```

This prevents `cargo test` from silently rewriting the snapshots on every run.

## How to regenerate them

If a deliberate contract or test change requires fresh snapshots:

1. Temporarily enable snapshot capture. The simplest way is to replace the `Env::new_with_config(...)` or `Env::default()` setup with an `Env` whose default `EnvTestConfig` has `capture_snapshot_at_drop: true` (this is the SDK default).
2. Run the relevant tests:
   ```bash
   cargo test
   ```
3. Inspect the resulting diff to confirm it reflects only the intended behavioral change.
4. Revert the temporary `capture_snapshot_at_drop: true` change before committing.

## What to do when `cargo test` produces a snapshot diff

A snapshot diff means the test's observable ledger/auth/event state changed. Treat it as a behavioral signal, not noise.

- **Expected diff:** you changed contract logic, test inputs, or the SDK version, and the resulting state change is exactly what you intended. Update the snapshot and include the diff in your PR.
- **Suspicious diff:** you made a "no-op" refactor, fixed a typo, or only touched documentation, but a snapshot still changed. This usually means the refactor was not actually a no-op, or the SDK/protocol produced different state. Stop and investigate before committing the snapshot.

### How to tell expected from suspicious

1. Identify which test produced the changed file from the diff path.
2. Read the diff and map the changed ledger entries, auth invocations, or events back to the test and the contract code it exercises.
3. Ask: does the diff directly follow from my change? If yes, it is expected. If not, revert the snapshot and figure out why the state shifted.

## What reviewers should look for

When a PR includes snapshot changes, reviewers should verify that:

- The number and identity of changed snapshot files matches the tests the PR claims to affect.
- The changed values (amounts, addresses, auth trees, ledger entries) are consistent with the described change.
- No unrelated snapshots drifted. Unrelated drift is a sign of an unstable test, an accidental SDK bump, or an incomplete revert.

## Why these files are committed

The snapshots are committed to version control because they act as **behavioral regression tests**. They catch unintended state changes caused by:

- contract logic edits,
- Soroban SDK upgrades,
- Stellar protocol upgrades, or
- changes to test fixtures that ripple through the contract state.

Ignoring them in `.gitignore` would lose this signal and force every contributor to regenerate them locally, making reviews harder and diffs non-deterministic.

## Verified behavior

The statements above were verified by running the test suite and a deliberate experiment:

- `cargo test` passes with the existing snapshots in place.
- `cargo build --target wasm32-unknown-unknown --release` succeeds.
- Changing a single test input (`f.client.fund(&Pool::Contributor, &f.sponsor, &600i128)` → `&601i128`) and re-running the test updated the corresponding snapshot file (`test_snapshots/test/a_claim_verifies_through_a_promoted_node_in_an_odd_tree.1.json`) to reflect the new amount in multiple ledger entries. Reverting the input and re-running restored the original snapshot content.

## Commands used to verify

```bash
# Run unit tests
cargo test

# Build the WASM release artifact
cargo build --target wasm32-unknown-unknown --release
```

The `stellar contract build` command requires the Stellar CLI, which is a separate install step not required to generate or validate these snapshots.
