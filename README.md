# Grainlify Stellar Contracts

Soroban contracts for GrainHack. The contract-side protocol is documented in
[`docs/MERKLE_LEAF_FORMAT.md`](docs/MERKLE_LEAF_FORMAT.md); that document is
self-contained and uses `contracts/grainhack-escrow/src/lib.rs` as its source
of truth.

## `grainhack-escrow`

One event, one chain. Holds the prize pools, publishes commitments, and
honours pull-based Merkle claims.

Two rules govern the design:

**The chain never decides anything.** Judging, assignment, appeals and payout
arithmetic stay off-chain and authoritative. This contract cannot compute a
bucket, a unit value or a payout — only honour a root the backend published.
There is deliberately no business logic here that could disagree with the
backend.

**Claims are pull-based.** Contributors claim against a published root and pay
their own gas. Nothing is pushed to a list of addresses: a failed push mid-loop
leaves an event half-paid with no clean recovery.

### What it enforces

| Guarantee | How |
|---|---|
| Maintainer funds cannot pay contributors, or vice versa | Two balances under separate storage keys, and the pool is inside the leaf hash |
| A root cannot promise more than the escrow holds | Checked at `publish_root` against that pool's balance |
| A draw-seed commit cannot be changed after the fact | Commitments are write-once; a second write errors |
| A published root cannot be swapped | Roots are write-once per pool |
| A proof cannot be replayed by another address | The claiming address is inside the leaf hash |
| An internal node cannot be claimed as a leaf | Domain separation: `0x00` for leaves, `0x01` for nodes |
| Funds cannot be withdrawn at will | The only non-claim exit is `sweep`: admin-authorised, time-locked, to a destination fixed at initialisation |
| A settled event cannot be cancelled | `cancel` is unreachable from `Settled` |

### Leaf format

```
leaf = SHA256(
  0x00 || pool_byte || address_length_be16 || claimant_address_utf8
  || identity_hash_32 || amount_be32
)
```

The fields are concatenated in exactly this order before SHA-256:

| Field | Encoding | Width |
|---|---|---:|
| Domain prefix | `0x00` | 1 byte |
| Pool | `Contributor = 0x00`, `Maintainer = 0x01` | 1 byte |
| Address length | Unsigned big-endian byte length | 2 bytes |
| Claimant | Canonical Soroban `Address::to_string()` bytes | length above |
| Identity hash | The 32 raw bytes of `identity_hash` | 32 bytes |
| Amount | Positive `i128`, zero-extended and right-aligned | 32 bytes |

The address length covers only the address bytes, not the two-byte prefix. The
amount is fixed-width big-endian, so a client must reject non-positive amounts
before encoding. `identity_hash` is `SHA256(github_login_lower ||
per_event_salt)`, computed off-chain. **The salt is never published** —
releasing it would let anyone with a list of GitHub logins match leaves to
addresses and reconstruct the `github_login → wallet` mapping the design
exists to prevent.

The backend must build identical leaves and use the same sorted-pair tree rules.
The contract exposes `leaf` so an integration can compare its implementation
against the contract-side construction. Internal nodes use
`SHA256(0x01 || lower_digest || higher_digest)` with the two 32-byte digests in
lexicographic order. See the complete [leaf and tree format guide](docs/MERKLE_LEAF_FORMAT.md).

## Build and test

```sh
cargo test
cargo build --target wasm32-unknown-unknown --release
stellar contract build
```

The repository pins Rust and the wasm target in `rust-toolchain.toml`. The
same commands, plus formatting and clippy with warnings denied, run in
`.github/workflows/ci.yml`. `stellar contract build` requires the Stellar CLI.

## Artifact metadata

The release wasm embeds its name, Cargo version, contract description,
canonical repository, and package name using Soroban's `contractmeta!` macro.
Inspect the compiled artifact—not only the source—with:

```sh
scripts/inspect-contract-metadata.sh
```

The helper prints the verified metadata, wasm size, and SHA-256 digest. It can
also inspect an existing artifact with `--wasm` or emit a CI-friendly report
with `--json`. See [the metadata release policy](docs/CONTRACT-METADATA.md)
for the release checklist and size-reporting guidance.

## Contributor task runner

Install [`just`](https://github.com/casey/just), then run `just` at the
repository root to list the verified commands. The common commands are
`just build`, `just test`, `just wasm-release`, `just stellar-build`,
`just format`, `just fmt-check`, `just lint`, and `just check`. `just ci` is an
alias for the complete local verification set. The runner only wraps the real
Cargo and Stellar CLI commands; see [the task-runner guide](docs/TASK-RUNNER.md)
for prerequisites and troubleshooting.

## Deploy to testnet

The repository includes a repeatable, testnet-only deployment command. It
builds the contract, uploads the wasm, creates an instance, calls
`initialise`, and writes the resulting IDs and configuration to a local JSON
record. The four initialise values are deliberately supplied by the caller:

```sh
scripts/deploy-testnet.sh \
  --network testnet \
  --source-account grainlify-deployer \
  --admin G... \
  --token C... \
  --sweep-dest G... \
  --sweep-delay 604800
```

The source identity must already be configured and funded on testnet. The
script refuses an unset or non-testnet network, validates the account before
submitting transactions, and refuses to reuse an existing deployment record
unless `--replace` is supplied. It never prints or writes a secret key.

The generated record is ignored by Git because testnet addresses are
environment-specific. Testnet is periodically reset, so a recorded contract
ID is a deployment receipt rather than a permanent address. Use `--help` for
equivalent environment-variable names and output-path options.

### Resource and wasm regression checks

The test suite records CPU instructions and memory bytes for every public
entry point. `claim` is measured at proof depths 1, 4, and 8 so the cost
growth is visible. The checked-in baseline also records the optimized wasm
size. Measurements may be up to 10% above baseline to allow for normal test
host noise while still catching meaningful regressions.

Run the full guard, including the wasm-size check, with:

```sh
scripts/check-resource-regressions.sh
```

If a deliberate change requires a new baseline, run
`scripts/update-resource-baseline.sh`, review the measured JSON diff, and
commit it separately from unrelated changes. The guard reports the entry
point, metric, baseline, observed value, tolerance, and regeneration command
when it fails. The current measurements are well below Soroban's documented
per-transaction CPU and memory budgets. At baseline, the depth-8 claim uses
1,971,762 CPU instructions against the current 400,000,000-instruction testnet
limit and 187,459 memory bytes against the 40 MiB limit (more than 99.5%
headroom in both dimensions). Network validators can change these limits, so
re-check them when reviewing a deployment or a large proof-path change.

## Not done here

Deployment, funding, key handling and mainnet operations are human tasks and
are deliberately absent from this repo. Per the spec: signing keys, treasury
custody and mainnet deploys are human operations.

**This contract has not been audited.** §8 of the spec makes an external audit
non-negotiable before it holds real money, and that is the reason for shipping
one chain first — one audit, not four.

See [`docs/SECURITY-MODEL.md`](docs/SECURITY-MODEL.md) for the current trust
assumptions, enforced invariants, out-of-scope items, and open questions for
maintainer review.
