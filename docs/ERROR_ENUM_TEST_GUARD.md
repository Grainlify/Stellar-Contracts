# Contract error inventory guard

The escrow contract exposes a typed `Error` enum. Its numeric discriminants
are part of the Soroban interface: clients compare contract errors by code,
and an error that exists in the contract but is not understood by the test
suite can silently become an untested state-machine branch.

## The guard

`contracts/grainhack-escrow/src/test.rs` contains two deliberately redundant
pieces of inventory:

1. `error_test_decision` matches every `Error` variant without a wildcard.
2. `error_inventory` lists the same variants in a reviewer-facing table.

The exhaustive match is the compile-time mechanism. If a contributor adds a
variant to `Error` and does not add a corresponding arm, Rust reports a
non-exhaustive-pattern error in the test target. The table and the length
assertion make the intended one-for-one relationship visible in review and
ensure an added variant cannot be hidden by an accidentally broad iterator.

This is intentionally a test-target guard rather than a production macro or a
runtime registry. It adds no wasm code, no storage, and no public entry point.
The contract's `#[contracterror]` derive remains the single source of truth for
the wire-level numeric codes.

## Variant decisions

Each current variant has one of three decisions:

| Variant | Decision | Evidence |
| --- | --- | --- |
| `AlreadyInitialised` | asserted | repeated `initialise` test |
| `NotInitialised` | asserted by setup/read contract paths | initialization contract |
| `NotAuthorised` | provided by Soroban auth machinery | auth requirements on admin/funder/claimant |
| `PoolMismatch` | structurally prevented | separate pool keys and pool-isolation tests |
| `InsufficientEscrow` | asserted | pool and oversized-root tests |
| `WrongState` | asserted | pre-root claim, running sweep, and settled cancel tests |
| `RootNotPublished` | documented unreachable today | `claim` checks `Settled` before root lookup |
| `RootAlreadyPublished` | asserted | root write-once test |
| `InvalidProof` | asserted | cross-pool, replay, and amount-binding tests |
| `AlreadyClaimed` | asserted | claim replay test |
| `CommitmentExists` | asserted | commitment write-once test |
| `CommitmentMissing` | documented reserved read state | optional commitment returns `None` |
| `TimelockActive` | asserted | sweep timelock test |
| `InvalidAmount` | asserted | zero and negative funding tests |

The table does not claim that every variant needs a separate artificial test
if the platform already owns the behavior. It does require a deliberate
decision and an explanation for every variant. A new variant must appear in
the match, the table, and either a focused assertion or an explicit
reachability/state-machine note.

## `RootNotPublished` reachability

The `claim` entry point executes these checks in order:

```text
load config
require claimant auth
require State::Settled
require amount > 0
load Root(pool)
```

`RootNotPublished` is raised only by the `unwrap_or_else` around `Root(pool)`.
The only state that permits a claim is `Settled`. Publishing a root is the
operation that transitions the instance to `Settled`, and that operation writes
the root before writing the state. Therefore a normal caller cannot observe a
settled instance with a missing root: before publication the state check
returns `WrongState` first.

The existing `claiming_before_a_root_is_published_fails` test pins this exact
precedence by funding an event, not publishing a root, and asserting
`WrongState`. It would be incorrect to change that test to expect
`RootNotPublished`; such a test would describe a branch the public state
machine cannot reach and would weaken the contract's lifecycle guarantee.

If future storage migration or a new admin operation can create `Settled`
without a root, that is a behavior change. At that point the test should be
updated to construct that state legitimately and assert `RootNotPublished`
exactly, rather than deleting the enum variant or broadening an error match.

## Demonstrating the guard

The guard should be demonstrated locally whenever the enum changes:

1. Add a temporary variant such as `TemporaryInventoryProbe = 99`.
2. Run `cargo test -p grainhack-escrow`.
3. Observe the compiler's non-exhaustive-pattern error pointing at
   `error_test_decision`.
4. Add the temporary variant to neither the inventory nor a production path;
   remove it immediately after the failure is recorded.
5. Run the test suite again and confirm the original inventory passes.

The temporary edit is not committed. The important evidence is the compiler
failure, not a runtime test that happens to enumerate a list. A runtime-only
list can drift by omission; an exhaustive match cannot compile while it is
missing a case.

## Review checklist

When reviewing an error change, check the following in order:

- Is the numeric discriminant preserved unless a deliberate protocol migration
  is documented?
- Does the exhaustive match compile only because every variant is present?
- Does the inventory array contain the same number of variants as the enum?
- Is the new variant tested at the public entry point that can raise it?
- If it is unreachable, does the explanation name the earlier state check and
  prove the write ordering that makes that check pre-emptive?
- Are exact contract error codes asserted instead of generic panic text?
- Does the change avoid a wildcard arm or a blanket `allow` that would defeat
  the guard?

The goal is not to force artificial coverage for impossible branches. The goal
is to ensure that impossibility is explicit, reviewable, and revisited when the
state machine changes.

## Verification

Run the focused package tests and then the repository checks:

```bash
cargo test -p grainhack-escrow
cargo test
cargo build --target wasm32-unknown-unknown --release
stellar contract build
```

The first command exercises the inventory directly. The final two build
commands confirm that the test-only guard has not changed the no-std contract
artifact or its release configuration.

## Scope and compatibility

This issue changes test-only code and documentation. It does not remove
`RootNotPublished`, renumber an error, change a contract function, or alter
storage. Existing clients continue to receive the same numeric error values.
The guard adds compile-time maintenance cost intentionally: adding a variant
now requires an explicit decision before CI can pass.

Future error variants should be added in one logical change with their
discriminant, public raise site, exact test, and inventory decision. Avoid
adding a catch-all conversion or using a string-only assertion; those recreate
the drift this guard is designed to prevent.

## CI failure interpretation

An error inventory failure should be treated as a design review request, not
as a nuisance compile failure. The failure normally belongs to one of three
categories:

### Non-exhaustive match

If Rust reports that a new enum variant is not covered by
`error_test_decision`, the author has added a wire-level error without making
the corresponding test decision. Inspect the new raise site first. Determine
whether a normal caller can reach it, then add the exact assertion or the
state-machine explanation. Only after that decision should the match arm be
added.

### Inventory length mismatch

If the array length assertion fails, compare the enum, match and inventory
side-by-side. A missing table row is a documentation defect; an extra table
row usually means a removed or renamed enum variant was not handled as a
protocol migration. Do not change the expected count blindly. Existing
numeric codes belong to deployed clients.

### Exact assertion regression

If an existing error test changes from its named contract error to another
error, investigate operation ordering and state transitions. In this contract,
`WrongState` intentionally pre-empts `RootNotPublished` for a claim before
settlement. Similar precedence can be security-relevant when an authorization
check must run before a storage or balance read.

## Local evidence record

For a change that adds or reclassifies an error, record the following in the
pull request:

```text
variant:                 Error::Example
numeric code:            15
public raise site:       function_name / condition
reachable:               yes or no
exact test:              test_name, or state-machine rationale
inventory updated:       yes
temporary probe:         compiler failure observed, then removed
workspace tests:         command and count
artifact delta:          unchanged or measured byte delta
```

This small record gives reviewers the same information regardless of whether
the variant is a user-facing validation error or an internal defensive guard.
It also prevents a “tests pass” statement from hiding that the new error was
never reached by a test or never examined for reachability.

## Why a runtime list is insufficient

A runtime list such as `const ERRORS = [Error::A, Error::B]` can prove only that
the listed values exist. It cannot prove that the list is complete: omitting a
new variant is a valid program. A wildcard match has the same weakness because
the compiler accepts any future variant through the wildcard arm. The
non-wildcard match is intentionally less convenient because the convenience
of a wildcard is exactly the source of silent drift.

The explicit array still has value for review, documentation and a count
assertion. It is paired with—not substituted for—the exhaustive match. This
two-layer design makes both compiler enforcement and human inspection clear.

## Relationship to coverage tools

Line coverage can report a match arm as unexecuted, but coverage thresholds do
not understand whether an enum variant was added. A newly added arm might
increase the denominator and still pass a percentage threshold. The guard is
orthogonal: it catches the missing arm at compile time, while coverage and
exact tests show which existing behavior has been executed. Keep both kinds of
evidence when the repository adds a coverage tool.

Mutation testing is also useful for the public assertions. Mutating an exact
error to `WrongState`, a generic panic, or a success path should fail a focused
test. It is not a reason to weaken the compile-time inventory or to add a
catch-all arm just to improve mutation-test ergonomics.

## Protocol migration notes

If an error must be removed, renamed, or renumbered, treat that as a protocol
migration:

- identify clients that compare the numeric code;
- reserve the old number rather than silently reusing it;
- document the compatibility window;
- add a migration test for old and new behavior;
- update the inventory with the historical decision;
- make the PR title and body call out the wire-level change.

This issue does not perform such a migration. It keeps all current variants
and codes unchanged while making future changes deliberate.

## Maintainer handoff

When handing the repository to a new maintainer, point them to the guard before
the contract implementation. The guard explains where the error model is
maintained, why one variant is currently unreachable, and what evidence is
expected in a review. A maintainer should be able to add a new operation and
know immediately whether it needs a new error, an existing state error, or a
documented impossible branch.

The desired outcome is a short, explicit review conversation at the point a
new error is introduced. It is much cheaper than discovering months later
that clients received an undocumented code or that a safety branch was never
exercised because the test asserted only that “some error” occurred.
