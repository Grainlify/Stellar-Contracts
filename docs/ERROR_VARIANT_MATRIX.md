# Error variant test matrix

This matrix is the review companion to the exhaustive error match in the
Soroban test target. It records whether a variant is observed through a public
entry point, enforced by host authentication, or intentionally unreachable
under the current state machine. It is not a replacement for the match: the
match is what makes omissions fail at compile time.

## How to read the matrix

Each row contains four questions:

1. What operation owns the error?
2. What exact condition raises it or prevents it?
3. Which test or invariant provides evidence?
4. What would require revisiting the decision?

The numeric code is included because Soroban clients receive the discriminant,
not the Rust identifier. Renumbering an existing variant is a protocol change
even when the human-readable name remains the same.

| Code | Variant | Decision | Evidence | Revisit when |
| ---: | --- | --- | --- | --- |
| 1 | `AlreadyInitialised` | public assertion | second initialise call | setup becomes upgradeable |
| 2 | `NotInitialised` | public-state guard | config/state reads | bootstrap moves outside contract |
| 3 | `NotAuthorised` | host-auth guard | `require_auth` sites | auth model changes |
| 4 | `PoolMismatch` | structural invariant | distinct pool storage keys | pool accounting is redesigned |
| 5 | `InsufficientEscrow` | public assertion | oversized root and pool tests | escrow becomes credit-backed |
| 6 | `WrongState` | public assertion | pre-root claim and lifecycle tests | state graph changes |
| 7 | `RootNotPublished` | unreachable today | state check precedes root read | settled state can exist without root |
| 8 | `RootAlreadyPublished` | public assertion | write-once root test | roots become versioned |
| 9 | `InvalidProof` | public assertion | replay, amount, and pool tests | proof protocol changes |
| 10 | `AlreadyClaimed` | public assertion | repeated claim test | claim marker is replaced |
| 11 | `CommitmentExists` | public assertion | write-once commitment test | commits become mutable by version |
| 12 | `CommitmentMissing` | optional-read decision | `None` read semantics | missing commits become fatal |
| 13 | `TimelockActive` | public assertion | pre-deadline sweep test | sweep timing is redesigned |
| 14 | `InvalidAmount` | public assertion | zero/negative funding test | amount type gains a new domain |

## Public error rows

### `AlreadyInitialised` (1)

`initialise` stores the configuration and state exactly once. A second call
must not replace the admin, token, destination, or delay. The existing test
uses a second generated address to make an accidental update visible, then
compares the exact contract error code. A future migration operation should
introduce a separate named path and error rather than weakening this guard.

### `NotInitialised` (2)

The internal `config` and `state` reads use this error when required instance
storage is absent. Test setup normally initializes before accessing the public
client, so this branch is a protection for undeployed or partially migrated
instances. If a future constructor guarantees initialization outside the
contract, retain the variant until the deployed protocol has migrated all
callers and update the decision deliberately.

### `NotAuthorised` (3)

The contract delegates authorization to Soroban's `require_auth` mechanism on
the configured admin, funder, and claimant addresses. The test harness uses
`mock_all_auths` for business behavior tests; it must not be interpreted as
proof that authorization is unnecessary. A dedicated authorization test is
appropriate if the harness begins testing real auth trees or if a new callable
operation moves funds.

### `PoolMismatch` (4)

Pool separation is intentionally structural. Contributor and maintainer
balances are stored under different typed keys, roots carry the pool, and the
leaf includes the pool discriminator. There is no ordinary branch that can
raise `PoolMismatch`; it is the named failure reserved for a future guard if a
cross-pool operation is introduced. The pool-isolation tests are the evidence
that the invariant is currently enforced before such a branch is needed.

### `InsufficientEscrow` (5)

Root publication and claim settlement cannot promise more than the balance of
the selected pool. The test funds one pool, tries to publish a larger root,
and asserts the exact error. A separate claim path is also protected by the
same balance check. Any future credit or delayed funding model must preserve a
clear distinction between funded and promised value.

### `WrongState` (6)

State checks protect the lifecycle: funding is allowed only while open or
funded, claim only after settlement, sweep only after settlement or cancel,
and cancel is rejected after settlement. The test suite exercises multiple
edges because one generic wrong-state assertion would not prove that every
operation is placed in the intended graph edge.

### `RootAlreadyPublished` (8)

Roots are write-once. Replacing a root after claim proofs have been distributed
would make an already-valid entitlement unverifiable and could redirect funds.
The test publishes a first root, attempts a second root, and checks that the
first value remains readable. A versioned-root protocol would require a new
storage key and explicit replay/snapshot semantics instead of changing this
error in place.

### `InvalidProof` (9)

The verifier protects the root promise. Current tests cover a wrong pool,
different claimant, different amount, and a leaf-shaped internal node. The
proof helper issue adds generated paths and corrupted-sibling tests. Each
assertion uses the exact contract error rather than a generic panic, so a
wrong-state failure cannot accidentally make an invalid proof appear covered.

### `AlreadyClaimed` (10)

The leaf digest is the replay key. The contract marks it before the token
transfer, and the test attempts the same claim twice while checking the token
balance remains unchanged. If claims become partially mutable or transferable,
the key must be redesigned together with the error decision; removing the
check would reintroduce a direct double-payment path.

### `CommitmentExists` (11)

Commit-reveal values are write-once. The test confirms that a second value
cannot replace the first and that reading still returns the original bytes.
This is especially important for draw seeds: an operator must not observe
applicants and then replace the seed with a favorable one.

### `TimelockActive` (13)

The sweep destination is fixed at initialization and the sweep deadline begins
when the event is settled or cancelled. The test attempts an early sweep,
asserts the exact error, advances the ledger, then confirms the transfer. A
change to time units or ledger timestamp handling should add boundary tests at
`deadline - 1`, `deadline`, and `deadline + 1`.

### `InvalidAmount` (14)

Zero and negative funding amounts cannot create useful accounting and are
rejected before token transfer. Root and claim amounts use the same positive
domain. The existing test covers both signs; a future amount type should keep
the validation before any external token call.

## Decision rows without a normal public failure

### `RootNotPublished` (7)

This variant remains in the enum because the root lookup is defensive and a
storage migration could expose the condition in the future. Under the current
state machine it is pre-empted:

```text
claim -> require Settled -> require amount > 0 -> read Root(pool)
publish_root -> write Root(pool) -> write Settled
```

The write ordering means a normally reachable `Settled` state has a root. A
pre-root claim returns `WrongState`, and the test pins that precedence. Do not
replace the test with a generic `is_err`; the distinction is the decision.

### `CommitmentMissing` (12)

The public `get_commitment` operation returns `Option<BytesN<32>>`, allowing a
caller to distinguish absent from committed-zero values. The variant is
reserved for a future operation that requires a commitment and chooses to
reject absence instead of returning `None`. Until such an operation exists,
the optional read is the deliberate behavior and the inventory records it.

## Adding a variant

When adding an enum variant, follow this sequence:

1. Choose and document a stable numeric discriminant.
2. Identify every public operation that can raise it.
3. Add a focused exact-error assertion, or write a state-machine proof that
   explains why the branch is unreachable.
4. Add the variant to `error_test_decision`. Let the compiler identify this
   omission if it is forgotten.
5. Add it to `error_inventory` and update the array length.
6. Update this matrix with evidence and a revisit condition.
7. Temporarily add a throwaway variant and run the test target to demonstrate
   that the exhaustive match fails; remove the throwaway edit before commit.
8. Run the package and workspace checks.

Do not add `_ => "unknown"`, convert errors to strings, or weaken exact error
assertions to satisfy the compiler. Those approaches make the enum compile but
restore the silent-drift failure mode.

## Review questions

Reviewers should be able to answer “yes” to each question below:

- Is the new numeric code unused and intentionally stable?
- Does the public behavior produce the intended exact contract error?
- Does the exhaustive match contain an arm with a meaningful decision?
- Does the inventory table contain the variant and the new total count?
- Is an unreachable decision justified by operation order and storage order?
- Are authentication and pool-separation claims backed by the appropriate
  host or structural tests rather than only comments?
- Were no unrelated tests deleted, skipped, or generalized?
- Does the PR state whether the contract artifact or snapshots changed?

The matrix is maintained as part of the contract's review surface. It should
grow when the error model grows, and it should become more precise—not less—if
the contract gains migrations, versioned roots, or new fund-moving operations.
