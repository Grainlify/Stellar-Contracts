# Grainlify Stellar Contracts

Soroban contracts for GrainHack. Companion to `Grainily_onchain_spec.md` in the
main repo.

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
leaf = SHA256( 0x00 || pool || claimant_address || identity_hash || amount_be )
```

`identity_hash` is `SHA256(github_login_lower || per_event_salt)`, computed
off-chain. **The salt is never published** — releasing it would let anyone with
a list of GitHub logins match leaves to addresses and reconstruct the
`github_login → wallet` mapping the design exists to prevent. See §4 of the
spec.

The backend builds identical leaves in `internal/chain/merkle.go`, pinned by a
golden fixture so the two implementations cannot drift.

## Build and test

```sh
cargo test
cargo build --target wasm32-unknown-unknown --release
```

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
