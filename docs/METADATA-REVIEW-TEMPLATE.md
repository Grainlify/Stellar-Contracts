# Release metadata review record

Copy this template into a release review or pull request description. It
exists to make artifact-level evidence easy to reproduce and to keep the wasm
size discussion separate from source-only claims.

## Artifact identity

- Commit: `REPLACE_WITH_COMMIT`
- Release tag: `REPLACE_WITH_TAG`
- Package: `grainhack-escrow`
- Target: `wasm32-unknown-unknown`
- Profile: `release`
- Artifact path: `target/wasm32-unknown-unknown/release/grainhack_escrow.wasm`
- Artifact size in bytes: `REPLACE_WITH_SIZE`
- SHA-256: `REPLACE_WITH_DIGEST`
- Rust version: `REPLACE_WITH_RUST_VERSION`
- Cargo version: `REPLACE_WITH_CARGO_VERSION`
- Stellar CLI version: `REPLACE_WITH_STELLAR_VERSION`

## Required commands

Record the command and result for each item. A check mark without output is not
enough for an artifact review.

```text
[ ] git status --short
[ ] cargo test
[ ] rustup target list --installed
[ ] cargo build --package grainhack-escrow --target wasm32-unknown-unknown --release
[ ] stellar version
[ ] scripts/inspect-contract-metadata.sh --no-build
[ ] scripts/inspect-contract-metadata.sh --json
```

## Metadata values

Paste the output from the inspection command or transcribe it exactly. If the
CLI output uses a table, retain the table in the review record.

| Key | Expected | Observed | Match |
| --- | --- | --- | --- |
| `name` | GrainHack escrow |  | [ ] |
| `version` | Cargo package version |  | [ ] |
| `description` | Holds GrainHack prize pools, publishes commitments, and honours pull-based Merkle claims. |  | [ ] |
| `license` | MIT |  | [ ] |
| `repository` | https://github.com/Grainlify/Stellar-Contracts |  | [ ] |
| `contract` | grainhack-escrow |  | [ ] |

## Size comparison

Metadata is embedded in every release artifact. Measure both files with the
same compiler, SDK, optimization profile, and target. If the previous artifact
is unavailable, say so rather than inventing a baseline.

- Previous artifact commit: `REPLACE_WITH_COMMIT_OR_NOT_AVAILABLE`
- Previous size: `REPLACE_WITH_SIZE_OR_NOT_AVAILABLE`
- New size: `REPLACE_WITH_SIZE`
- Difference: `REPLACE_WITH_DELTA`
- Same toolchain and profile: `[ ] yes  [ ] no`
- Explanation for unrelated size changes: `REPLACE_WITH_EXPLANATION`

Example commands:

```sh
wc -c previous/grainhack_escrow.wasm
wc -c target/wasm32-unknown-unknown/release/grainhack_escrow.wasm
shasum -a 256 target/wasm32-unknown-unknown/release/grainhack_escrow.wasm
```

## Scope review

The metadata issue is intentionally non-functional. Confirm that the change
does not modify any of the following:

- [ ] public function names or argument types;
- [ ] storage key definitions or storage layout;
- [ ] contract error numbers;
- [ ] authorization requirements;
- [ ] token transfer amounts or destinations;
- [ ] Merkle leaf field ordering or hash prefixes;
- [ ] root publication rules;
- [ ] claim replay protection;
- [ ] cancellation or sweep state transitions;
- [ ] deployment or signing configuration.

If any box cannot be checked, stop and explain the additional runtime change.
Metadata should identify an artifact, not become a back door for changing its
behavior.

## Public-data review

The metadata is visible to every observer. Confirm that no public field contains
private or deployment-specific information:

- [ ] no wallet address;
- [ ] no secret key or token;
- [ ] no GitHub login or claimant identity;
- [ ] no per-event salt;
- [ ] no internal incident or customer identifier;
- [ ] no environment variable value;
- [ ] no staging or production hostname that is not canonical;
- [ ] no claim about behavior the contract does not implement.

The Merkle identity hash remains the correct place for the salted off-chain
identity commitment. It must not be copied into metadata for convenience.

## Reproducibility notes

Describe anything that could cause another operator to build a different wasm:

- compiler or target installation changes:
- Cargo lockfile changes:
- Soroban CLI version changes:
- package feature flags:
- environment variables:
- generated files:
- local patches:
- network-dependent steps:

The metadata inspection itself is read-only with respect to the network. It
does not deploy a contract, invoke a function, sign a transaction, or alter
ledger state.

## Reviewer sign-off

- Source macro reviewed: `[ ]`
- Manifest version compared: `[ ]`
- README description compared: `[ ]`
- Compiled artifact inspected: `[ ]`
- Size recorded: `[ ]`
- Digest recorded: `[ ]`
- Tests recorded: `[ ]`
- No runtime scope expansion: `[ ]`
- Reviewer: `REPLACE_WITH_REVIEWER`
- Date: `REPLACE_WITH_DATE`

## Failure record

If the helper fails, preserve the failure rather than deleting it from the
review. Include the command, exit status, raw error, and resolution:

```text
Command:
Exit status:
Raw output:
Root cause:
Resolution:
Rerun result:
```

Typical causes include a missing wasm target, a stale release artifact, a
missing Stellar CLI, an unsupported CLI output format, or a source version
literal that no longer matches `Cargo.toml`. The helper should fail closed in
each case.
