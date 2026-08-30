# Merkle proof test helper

The escrow contract verifies proofs against a canonical binary Merkle tree.
The test suite now contains a proof generator so larger and randomly shaped
trees can be exercised without manually writing sibling paths.

## Canonical tree rules

The helper in `contracts/grainhack-escrow/src/test.rs` deliberately mirrors the
contract's verifier rather than introducing a second tree format:

1. The caller supplies leaf digests in any order.
2. Leaves are sorted in ascending lexicographic byte order.
3. Adjacent pairs are hashed as `SHA-256(0x01 || lower || higher)`.
4. When a level has an odd number of nodes, its final node is promoted without
   being duplicated or hashed with itself.
5. A proof contains only siblings encountered by the target as it rises. A
   promoted target has no sibling at that level, so the proof contains no
   placeholder for it.

The contract also uses a separate `0x00` domain for leaves. The helper accepts
already-built digests and therefore tests tree mechanics independently from
the leaf encoding, which is pinned by the cross-implementation vectors in the
same test module.

## Helper contract

`proof_for_leaf(env, digests, target)` takes a non-empty collection and a
digest that is present in that collection. It returns a standard Rust vector
of sibling digests in verifier order. The `sdk_proof` adapter converts that
vector into Soroban's `Vec` only at the contract call boundary.

The target is tracked by digest after the initial sort. At every level the
helper records the adjacent sibling when one exists, constructs the next
level using the same promotion rule as `root_from_digests`, and updates the
target to its parent. This makes the relationship between a proof and the
root explicit and keeps the algorithm useful for both direct verifier tests
and full `claim` calls.

## Coverage matrix

The generated-proof suite covers every leaf in every tree size from 1 through
64. That includes:

| Shape | Examples | Risk covered |
| --- | --- | --- |
| singleton | 1 | empty path and root-equals-leaf behavior |
| pair | 2, 4, 8, 16, 32, 64 | ordinary sibling folding |
| odd | 3, 5, 7,  nine | promoted nodes at one or more levels |
| non-power-of-two | 6, 10, 17, 24, 38 | promotion followed by later pairing |
| large claim | 24 real leaves | full client claim against a published root |

The retained three-leaf test keeps its hand-built proof and asserts that the
generated path is byte-for-byte identical. This is an important independent
check: a generator can agree with its own root builder while both are wrong in
the same way.

## Failure behavior

An empty tree and an unknown target are programmer errors in the test helper
and panic with a clear message. A production backend should validate its input
before constructing a root or proof and should never publish a root that does
not include the entitlement it intends to serve.

Proofs do not include left/right flags because the contract sorts every pair.
Adding positional flags would create a second protocol and would not improve
the current verifier. The helper must therefore continue to preserve digest
ordering at each node.

## Why the helper is tested against the verifier

Testing only `proof_for_leaf` against `root_from_digests` would leave a common
failure mode undetected: both helpers could share the same wrong promotion or
ordering rule. The matrix therefore passes each generated path into the
contract's actual private verifier. The large claim goes one step further and
uses the generated path through the generated Soroban client and token
transfer path.

The corruption tests are equally intentional. They replace each sibling in a
valid path and assert that the root no longer verifies, then remove a required
sibling and assert failure. These tests ensure the helper is not accidentally
returning a path that the verifier ignores, and protect against a future
verifier change that silently accepts incomplete proofs.

## Duplicate digest policy

The helper rejects duplicate leaves. A digest target is the lookup key used to
track a leaf through sorted levels; duplicates make that lookup ambiguous and
can cause a proof to be generated for a different occurrence than the caller
intended. Production claim data should also make leaf digests unique by
binding claimant, identity commitment, pool and amount into the leaf. The
test-only panic is preferable to publishing an ambiguous fixture.

## Choosing future fixtures

When a production bug is found, first reduce it to the smallest leaf count
that demonstrates it, then retain that count as a focused fixture. Keep a
second large generated case if the bug concerns allocation or deep paths. Use
fixed digests for protocol vectors and generated addresses only for end-to-end
claim behavior. Never update a root or proof golden value just because an
implementation changed: confirm whether the protocol or the implementation
moved before changing any vector.

The expected path length for a power-of-two tree is `log2(n)`. Non-power-of-two
trees can be shorter for leaves promoted through odd levels. The tests assert
verification rather than a single path length because the exact length is a
property of the target's position after canonical sorting.

## Compatibility and review checklist

This issue is test infrastructure only. It does not add a contract entry point,
change storage, change error codes, or alter the wire format of a claim. A
reviewer should verify:

- the helper sorts the input before locating the target;
- every non-promoted target records exactly one adjacent sibling;
- a promoted target records none for that level;
- the next target is the actual parent digest;
- generated paths are sent to the real verifier;
- at least one hand-written path remains as an independent cross-check;
- the 24-leaf claim reaches the real token transfer;
- no snapshot change is attributed to helper-only changes except the expected
  new test snapshot.

These checks keep future refactors reviewable. The helper may be optimized, but
the canonical sorting, node prefix, sorted-pair hashing and promotion rules
must remain visible in either the implementation or a directly linked test.

## Snapshot guidance

The 24-leaf test adds a normal Rust test but does not change contract behavior
or generated contract snapshots. If a future change adds a contract entry
point or renames an existing test, regenerate the Soroban snapshots and call
that out explicitly in the pull request. Changes to the helper alone should
not require snapshot updates.

## Verification

Run the focused contract tests while iterating:

```bash
cargo test -p grainhack-escrow
```

Then run the repository checks required by the issue:

```bash
cargo test
cargo build --target wasm32-unknown-unknown --release
stellar contract build
```

The proof generator is test-only code. It must not be copied into the
production contract because the on-chain verifier remains the authority for
claim acceptance and keeping proof construction off-chain keeps wasm small.
