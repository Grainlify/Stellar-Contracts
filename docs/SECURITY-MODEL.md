# GrainHack escrow security model

Status: working security model for maintainer review. This document is an
inventory of assumptions, enforced properties, and explicit exclusions. It is
not an audit, a security guarantee, or a statement that the contract is safe
to deploy with real funds.

## System boundary

The repository contains one Soroban contract, `grainhack-escrow`. The contract
holds two token pools, stores operator commitments, publishes one Merkle root
per pool, and pays pull-based claims. The contract does not judge work,
assign contributors, calculate awards, close appeals, manage signing keys, or
deploy to a network. These boundaries are stated in the repository README and
in the module-level documentation in
[`contracts/grainhack-escrow/src/lib.rs`](../contracts/grainhack-escrow/src/lib.rs).

The following inventory is based on the implementation on the default branch
and should be updated when the contract or its off-chain companion changes.
Every implementation claim below names the relevant source function or
document so a reviewer can verify it directly.

## Assets and authority

| Asset or authority | What can affect it | Evidence in this repository | Security consequence |
| --- | --- | --- | --- |
| Contributor token balance | `fund`, `claim`, and `sweep` using `Pool::Contributor` | `src/lib.rs`: `Pool`, `Key::Balance`, `fund`, `claim`, `sweep` | The balance is keyed separately from the maintainer pool. The contract still relies on the configured token contract behaving as an expected Soroban token. |
| Maintainer token balance | `fund` and `sweep` using `Pool::Maintainer` | `src/lib.rs`: `Pool`, `Key::Balance`, `fund`, `sweep` | A root for one pool is checked against that pool's balance; aggregate contract balance is not used for this check. |
| Published entitlement root and total | `publish_root` after admin authorization | `src/lib.rs`: `publish_root`, `Key::Root`, `Key::RootTotal` | A root is write-once and must not exceed the selected pool balance. The admin can still choose the root and total. |
| Commitments | `commit` after admin authorization | `src/lib.rs`: `commit`, `Key::Commitment` | Each `(kind, subject)` value is write-once. The contract does not interpret or reveal a commitment protocol. |
| Unclaimed funds | `claim` or time-locked `sweep` | `src/lib.rs`: `claim`, `cancel`, `sweep`, `Key::SweepAfter` | Sweep is limited to settled/cancelled states, requires admin authorization, and pays the fixed initialization destination. |
| Contract configuration | `initialise` exactly once | `src/lib.rs`: `initialise`, `Key::Config` | Admin, token, sweep destination, and delay cannot be replaced through another initialization call. The initial transaction and account remain trust-critical. |

## Privileged roles and enforcement

There is one configured `Config.admin` address. `initialise`, `commit`,
`publish_root`, `cancel`, and `sweep` require authorization from that address;
`fund` requires authorization from its `from` address; and `claim` requires
authorization from its `claimant` address. These checks are visible in the
corresponding functions in `src/lib.rs` through `require_auth`. Soroban account
multisig is expected to be configured at the account layer, as the `Config`
field documentation explains; this contract does not implement a second
multisignature scheme.

The admin can therefore:

* select the token, admin, fixed sweep destination, and sweep delay at
  initialization (`initialise`);
* fund either pool from an authorized source (`fund`);
* publish an arbitrary 32-byte commitment, or a positive root total within the
  selected pool's balance (`commit`, `publish_root`); and
* cancel before settlement or sweep after the stored delay (`cancel`,
  `sweep`).

The admin cannot republish a root after `Key::Root` exists, cancel after the
state is `Settled`, or sweep before the stored `SweepAfter` timestamp. Those
guards are implemented in `publish_root`, `cancel`, and `sweep`. They do not
protect against a malicious or compromised admin choosing a bad root before
settlement.

## State and fund-flow invariants

The state machine is `Open -> Funded -> Settled` for a normal event, with
`Open`/`Funded -> Cancelled` available through `cancel`. `claim` requires
`Settled`; `fund` requires `Open` or `Funded`; `publish_root` requires `Funded`
or `Settled`; and `sweep` requires `Settled` or `Cancelled`. These are the
state checks in the named entrypoints and the `State` enum in `src/lib.rs`.

The implementation stores a separate `Balance(Pool)` value for each pool.
`publish_root` compares a root's total only with the selected pool, and
`claim` subtracts only from that pool. The separation is exercised by
`pools_are_separate_balances` and
`a_leaf_from_one_pool_cannot_claim_from_the_other` in `src/test.rs`.

`claim` verifies the root proof, checks the claimed marker, checks the selected
pool balance, sets the marker and decrements the balance before invoking the
token transfer. This ordering is implemented in `claim`; the intended
single-payment property is exercised by `claim_pays_once_and_only_once` in
`src/test.rs`. A failed transaction is expected to roll back Soroban storage
and the transfer together; the token contract's own behavior is outside this
repository's control.

## Entitlement and Merkle assumptions

The leaf construction is `SHA256(0x00 || pool || address length || canonical
address string || identity hash || 32-byte big-endian amount)`. The exact
encoding is implemented in `leaf_hash` and exposed by `leaf`; the module
documentation and `leaf_construction_is_deterministic` plus
`leaf_matches_the_pinned_cross_implementation_vector` in `src/test.rs` are the
local evidence for this format.

The claimant address, pool, identity hash, and amount are all bound into the
leaf. `claim` requires the claimant to authorize the call, so a proof made for
one address is not intended to be replayable by another address. This is
tested by `a_proof_cannot_be_replayed_by_another_address`; it is a property of
the hash construction, not an assertion that the identity behind a salted
hash is unknowable in every possible context.

Merkle internal nodes use `SHA256(0x01 || sorted(left,right))`, while leaves
use the distinct `0x00` prefix. The implementation and tests
`an_internal_node_cannot_be_claimed_as_a_leaf` and
`a_claim_verifies_through_a_promoted_node_in_an_odd_tree` define the supported
proof convention. The contract does not build trees, check the off-chain
claim list, or determine whether the published root is socially correct.

## Trust assumptions

This contract depends on the following assumptions. They are assumptions,
not guarantees supplied by this repository:

1. The configured admin account is controlled by the intended maintainers,
   uses appropriate multisig controls where required by the project design,
   and keeps its signing keys secure. Source: `Config.admin` documentation and
   `require_auth` calls in the privileged entrypoints.
2. The initializer and deployment process chooses the intended token,
   administrator, destination, and delay. Source: `initialise` and `Config`.
3. The token contract follows the expected transfer semantics and does not
   maliciously re-enter or otherwise violate the assumptions of a Soroban
   token client. Source: token client calls in `fund`, `claim`, and `sweep`.
4. The off-chain backend computes the intended awards, closes the appropriate
   review/appeal process, builds the exact leaf format, and publishes the
   corresponding root through the authorized admin. Source: README's
   off-chain boundary, `publish_root` comments, and `leaf_hash`.
5. Claimants keep the identity salt and their claim inputs private enough for
   the project's intended privacy property. The contract receives an
   `identity_hash`, not the GitHub login, but public transaction data and
   external information can still permit correlation. Source: `claim` and
   `leaf_hash` documentation.
6. The Soroban runtime, ledger timestamp, cryptographic primitives, and
   authorization model behave according to the SDK/runtime version used by
   this workspace. Source: workspace `Cargo.toml` and all contract operations.

## Explicitly out of scope

The following are intentionally not security claims or responsibilities of
this contract:

* judging submissions, contributor identity verification, assignment,
  appeals, payout arithmetic, or backend database integrity;
* correctness or availability of the off-chain Merkle builder, reconciliation
  job, deployment scripts, RPC provider, indexer, or frontend;
* admin key custody, multisig membership/recovery, token issuance, token
  freeze/ clawback policy, treasury operations, or network deployment;
* privacy against an observer who can correlate addresses, amounts, timing,
  public roots, and off-chain information;
* upgradeability, emergency pause, admin rotation, or recovery after an
  incorrectly initialized deployment; no such mechanism is implemented in
  the current `Config`, `State`, or public entrypoints;
* denial-of-service, ledger fee markets, Soroban resource limits, or failures
  of external infrastructure; and
* formal verification, independent audit, economic modeling, or a claim that
  the current implementation is safe for real funds.

The README explicitly says deployment, funding, key handling, and mainnet
operations are human tasks and that the contract has not been audited. This
document preserves that limitation rather than replacing it with a stronger
claim.

## Code/README alignment notes

The README describes the same high-level model implemented by the contract:
two separately keyed pools, write-once roots and commitments, pull-based
claims, and a fixed-destination timelocked sweep. The implementation evidence
for those statements is listed in the tables and sections above (`Pool`,
`Key`, `fund`, `commit`, `publish_root`, `claim`, and `sweep`).

The README also names `Grainily_onchain_spec.md`, an off-chain backend Merkle
builder, and a golden fixture. Those companion artifacts are not present in
this repository, so this document records their existence as a dependency but
does not independently verify their contents. The contract-side leaf format
is instead checked by the local tests named in the entitlement section.
Likewise, the README's statement that there is "one event" is an operating
model; the contract has no stored event identifier and cannot enforce that a
deployment is used for only one event. Deployment discipline is therefore an
explicit trust assumption, not an on-chain invariant.

## Open questions for maintainer review

These questions need an explicit project decision before an audit or a funded
deployment:

1. What exact operational process proves that judging and appeals are complete
   before `publish_root` is authorized?
2. Is a single immutable admin and fixed sweep destination sufficient, or is a
   rotation/recovery mechanism required before deployment?
3. What token implementations and transfer failure behaviors are supported,
   and how will the configured token address be verified?
4. What identity-salt lifecycle and disclosure policy preserves the intended
   privacy property while still allowing reconciliation?
5. What deployment/reproducibility evidence, multisig threshold, monitoring,
   and incident-response process are required for a real event?
6. Should the contract reject additional funding after a root is published, or
   is the current `Funded`/`Settled` transition and per-pool accounting the
   intended policy?

Maintainers should resolve these questions and review every source reference
before treating this document as an approved security specification.
