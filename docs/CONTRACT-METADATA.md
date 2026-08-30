# Contract metadata release policy

The `grainhack-escrow` wasm includes a small metadata section produced by
Soroban's `contractmeta!` macro. This section is part of the artifact, not a
comment in the source tree. A person who has only a deployed contract or a
downloaded `.wasm` can identify the contract and the source release without
rebuilding every candidate revision.

## Why the metadata exists

An on-chain contract is normally identified by an address and a wasm hash.
Those values are useful for integrity, but they do not tell an operator what
the artifact does or which package version produced it. Metadata supplies the
human-readable context while leaving the executable interface, storage, error
codes, and authorization rules unchanged.

The metadata is intentionally short. Soroban stores it in the wasm, and every
extra byte is carried by every release. The current entries are:

| Key | Value | Stability rule |
| --- | --- | --- |
| `name` | `GrainHack escrow` | Keep stable for explorers and inventories. |
| `version` | Cargo package version | Change only with a package release. |
| `description` | Holds GrainHack prize pools, publishes commitments, and honours pull-based Merkle claims. | Must remain faithful to the README. |
| `license` | `MIT` | Matches the repository license policy. |
| `repository` | `https://github.com/Grainlify/Stellar-Contracts` | Use the canonical upstream URL. |
| `contract` | `grainhack-escrow` | Matches the Cargo package name. |

## Source of truth

The macro calls live at module level near the imports in
`contracts/grainhack-escrow/src/lib.rs`. Soroban requires metadata values to be
string literals, so the version literal is kept equal to the package manifest.
The artifact inspection script reads Cargo's version and fails before CLI
inspection if the source literal and manifest disagree. The package manifest
is therefore the source of truth while a stale source literal is caught in CI.
The description is duplicated in this document and the README so reviewers can
spot a user-facing documentation change, but the artifact inspection script
checks the exact expected sentence.

Do not put wallet addresses, event identifiers, GitHub login names, salts,
secrets, API keys, or environment-specific deployment data in metadata. The
metadata is public and copied into every artifact. A claimant's identity is
intentionally represented by a salted hash in the Merkle leaf and must never
be placed in a public descriptive field.

## Building the artifact

The repository uses a size-focused release profile. Build the exact package
and target used for deployment:

```sh
cargo build --package grainhack-escrow \
  --target wasm32-unknown-unknown --release
```

The resulting path is normally:

```text
target/wasm32-unknown-unknown/release/grainhack_escrow.wasm
```

The command compiles the contract. It does not deploy it and does not sign a
transaction. A build passing is necessary but it does not prove that metadata
was embedded; the release artifact must be inspected as a separate step.

## Inspecting the embedded section

Run the repository helper from the root:

```sh
scripts/inspect-contract-metadata.sh
```

The helper builds the wasm, invokes `stellar contract info meta`, checks every
required entry, and prints the artifact size and SHA-256 digest. To inspect a
previously built artifact without changing it:

```sh
scripts/inspect-contract-metadata.sh --no-build
scripts/inspect-contract-metadata.sh --wasm path/to/release.wasm
```

For CI or another automated release tool, use the JSON form:

```sh
scripts/inspect-contract-metadata.sh --json
```

The JSON report contains `ok`, `wasm`, `size_bytes`, `sha256`, `name`, and
`version`. The command exits non-zero when the CLI is unavailable, the wasm is
missing, the package version cannot be read, or any required entry is absent.
This makes it safe to use as a release gate rather than as a best-effort log.

## Stellar CLI versions

The exact formatting of `stellar contract info meta` has changed across CLI
versions. The helper intentionally searches the CLI output for the required
values instead of depending on one table layout. This accepts equivalent
human-readable output while still requiring all six values.

If a future CLI changes the command name, update the helper and this document
together. Do not replace inspection with `cargo build` alone. Record the CLI
version in a release report when a published artifact is reviewed:

```sh
stellar version
scripts/inspect-contract-metadata.sh --json
```

## Release review checklist

Before attaching a wasm to a release, the maintainer should:

1. Start from a clean checkout of the intended commit.
2. Confirm `cargo test` passes for the workspace.
3. Confirm the wasm target is installed.
4. Build the package with the release profile.
5. Run the metadata helper against the generated artifact.
6. Compare the reported version with the release tag and package manifest.
7. Read the reported description and confirm it matches the README.
8. Record the byte size and digest in the release notes.
9. Verify no wallet, identity, or environment-specific values appear.
10. Keep the inspected file and commit together in the release archive.

The size comparison belongs in the pull request because adding metadata has a
real artifact cost. Compare the pre-change and post-change release wasm from
the same toolchain:

```sh
wc -c target/wasm32-unknown-unknown/release/grainhack_escrow.wasm
```

Do not compare a debug build with a release build. Do not compare artifacts
produced by different Rust or Soroban SDK versions and attribute the complete
difference to metadata. The PR should state the measured sizes and the
environment used for both measurements.

## Compatibility and upgrade expectations

Metadata is descriptive. Changing it must not change callable function
signatures, storage keys, error numbers, Merkle leaf construction, token
transfers, or authorization. A metadata-only PR should have no runtime logic
diff beyond the macro declarations.

Adding a new key is normally backwards-compatible for tooling that ignores
unknown keys. Renaming `name`, `contract`, or `repository` is not a harmless
cosmetic change because inventories may use them as stable identifiers. If a
value must change, explain the reason in release notes and keep the old
identity discoverable through the release documentation.

The version is informational and does not act as an on-chain upgrade guard.
Operators must continue to verify the wasm hash and deployment address using
their normal change-control process. Metadata cannot substitute for an audit,
code review, reproducible build, or signed release.

## Troubleshooting

### `stellar` is not installed

Install the Stellar CLI using the official installation instructions, then
rerun the helper. Do not mark the issue complete based only on a source review.

### The wasm path is missing

Check that `wasm32-unknown-unknown` is installed and run the package-specific
Cargo build command. A workspace build can succeed while producing a different
package's artifact.

### A value is absent from CLI output

Inspect the raw wasm with the same CLI version used by the helper. Check that
the macro is at module level, that the package was rebuilt after the source
change, and that the helper points at the intended file. Delete only the
package's stale release artifact if necessary and rebuild.

### The description is disputed

Use the README's description of the contract's actual responsibility. Do not
turn metadata into a marketing claim or describe off-chain judging and payout
arithmetic as on-chain behavior. The chain holds funds, publishes commitments,
and honours pull-based Merkle claims; it does not decide winners.

## Maintainer notes

Metadata review is deliberately mechanical. Reviewers should be able to see
the six entries in source, run one command, and compare the printed output to
the PR. Keeping the policy in the repository prevents a release process from
quietly becoming dependent on an individual operator's memory.
