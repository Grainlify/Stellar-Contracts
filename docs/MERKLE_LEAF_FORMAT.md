# Merkle leaf and tree format

This document is the self-contained client reference for the claim hash used
by `grainhack-escrow`. It describes the bytes the contract hashes, the bytes a
client must provide, the internal-node rule, and the supported build and test
commands. The implementation in
`contracts/grainhack-escrow/src/lib.rs` is authoritative if this document and
an older integration disagree.

## Scope

The contract does not calculate awards, identify contributors, or build a
Merkle tree. An off-chain publisher calculates an entitlement, constructs a
leaf, builds a tree, and publishes one root for a pool. A claimant supplies the
same entitlement inputs and a sibling path. The contract hashes those inputs,
checks the path against the stored root, and transfers the requested amount.

This document covers:

- the leaf preimage and its exact field order;
- canonical claimant-address encoding;
- identity-hash inputs and privacy boundaries;
- positive-amount encoding;
- internal-node hashing and sibling ordering;
- odd-level promotion and proof construction;
- cross-language implementation guidance;
- supported build commands; and
- review and verification checklists.

This document does not define an off-chain judging policy, a GitHub identity
policy, token issuance, deployment, or a different hash algorithm. It also does
not authorize changing the contract to fit an old README example.

## Source of truth

The production implementation is the private `leaf_hash` function and the
private `verify_proof` function in
`contracts/grainhack-escrow/src/lib.rs`. The public `leaf` entry point exposes
the leaf construction so a client can compare bytes and digest values against
the deployed contract during integration testing.

The public `claim` entry point performs the following sequence:

1. require authorization from the claimant;
2. require the selected pool to be settled;
3. reject a non-positive amount;
4. load the root for the selected pool;
5. hash the supplied pool, address, identity hash, and amount;
6. reject an already-claimed leaf;
7. verify the sorted sibling path;
8. check the selected pool balance;
9. mark the leaf claimed and decrement that pool; and
10. transfer the token to the claimant.

The hash format is therefore part of the claim protocol, not a display
convention. A client that differs by one prefix byte, length byte, address
encoding byte, or amount byte will produce a different leaf and fail proof
verification.

## Leaf equation

The complete equation is:

```text
leaf = SHA256(
    0x00
  || pool_byte
  || address_length_be16
  || claimant_address_utf8
  || identity_hash_32
  || amount_be32
)
```

The preimage is not a text string and must not contain separators, spaces,
hexadecimal formatting, or a JSON representation. Every `||` above means raw
byte concatenation. SHA-256 is applied once to the complete preimage. The
result is exactly 32 bytes.

## Field table

| Order | Field | Encoding | Width |
|---:|---|---|---:|
| 1 | Leaf domain | Literal `0x00` | 1 byte |
| 2 | Pool | `Contributor = 0x00`; `Maintainer = 0x01` | 1 byte |
| 3 | Address length | Unsigned big-endian integer | 2 bytes |
| 4 | Claimant address | Canonical `Address::to_string()` UTF-8 bytes | 0–64 bytes in implementation buffer |
| 5 | Identity hash | Raw `BytesN<32>` bytes | 32 bytes |
| 6 | Amount | Positive `i128` encoded as a 32-byte big-endian unsigned magnitude | 32 bytes |

The address field is the only variable-width field. Its length prefix is part
of the preimage and covers only the address bytes. The prefix itself is not
included in the length. The implementation allocates a 64-byte temporary
buffer for the canonical address representation; standard account addresses
fit this bound.

For an address of length `L`, the preimage length is:

```text
1 + 1 + 2 + L + 32 + 32 = 68 + L bytes
```

The digest length is always 32 bytes regardless of `L`.

## Domain prefix

The first byte is always `0x00`. It separates a leaf preimage from an internal
node preimage. Internal nodes use `0x01`, so a digest created for an internal
node cannot be interpreted as a leaf preimage by removing a tree level.

Do not encode the prefix as the ASCII character `"0"`, the two-character hex
string `"00"`, a four-byte integer, or a serialized enum. It is one raw byte
with numeric value zero.

## Pool encoding

The second byte binds the claim to one of the contract's two independent
balances:

```text
Pool::Contributor -> 0x00
Pool::Maintainer  -> 0x01
```

The pool byte is not a string and is not the Soroban XDR encoding of the enum.
It is exactly one byte. A proof built for a contributor root cannot be replayed
against a maintainer root by changing only the claim call's pool argument,
because the resulting leaf digest changes.

The backend must select the pool before hashing and must use the same pool in
the root publication and claim request. A pool mismatch is a protocol input
error, not a reason to try both roots.

## Address encoding

The claimant is encoded as the UTF-8 bytes of the canonical string returned by
Soroban's `Address::to_string()`. The contract does not hash XDR, an account
public-key byte array, a raw contract identifier, or a display string supplied
by a user without canonicalization.

The address encoding procedure is:

1. parse or obtain the Soroban `Address`;
2. call the platform's canonical address-to-string operation;
3. encode that string as UTF-8 bytes without a terminating NUL;
4. calculate the byte length, not the number of Unicode code points;
5. encode that length as a two-byte unsigned big-endian integer; and
6. append the length followed by the address bytes.

The length prefix is:

```text
length_hi = (length >> 8) & 0xff
length_lo = length & 0xff
```

The two bytes are appended as `length_hi`, then `length_lo`. There is no
padding after the address. The identity hash begins immediately after the last
address byte.

Soroban account addresses are normally printable ASCII StrKeys, so their UTF-8
byte length equals their visible character length. Clients must nevertheless
measure the encoded byte array because the protocol is defined in bytes.

## Identity hash

`identity_hash` is supplied as a 32-byte value. The current off-chain convention
is:

```text
identity_hash = SHA256(github_login_lower || per_event_salt)
```

The exact off-chain salt framing belongs to the companion backend protocol. It
is not re-derived by this contract. The contract receives only the resulting
32 raw bytes and places them into the leaf.

The salt must not be published as part of the claim preimage, an event, a
contract metadata field, a README example, or a debug log. The hash is a
commitment, not encryption. A public salt or a small candidate identity set can
make correlation easier. The address and amount are public transaction inputs
when a claim is submitted, so this format does not promise complete privacy.

Clients must append the 32 digest bytes directly. They must not append a
hexadecimal string, a base64 string, a length prefix, or the original login.

## Amount encoding

The claim amount must be positive before hashing. The contract encodes a
positive `i128` magnitude into 32 bytes, right-aligned and big-endian:

```text
amount_be32 = 16 zero bytes || amount.to_be_bytes()
```

For example, the value `1` ends in `01` and has 31 preceding zero bytes. The
value `256` ends in `01 00`. The amount field is always 32 bytes even though
the contract input is an `i128`; this leaves the high half zero for valid
values and makes the wire format fixed-width.

The contract rejects zero and negative claim amounts before proof verification.
The leaf helper renders a non-positive amount as all zeros as a defensive
internal rule, but that representation is not a valid claim entitlement and
must not be published by a client.

Do not use a signed two's-complement 32-byte extension, decimal text, floating
point, a variable-length integer, or little-endian bytes. Do not round an
amount during encoding. The backend and the claimant must use the same integer
unit selected by the token and payout policy.

## Worked layout

For a hypothetical contributor claim with an address of `L` bytes, the bytes
are laid out as follows:

```text
offset 0       00                                      leaf domain
offset 1       00                                      contributor pool
offset 2..3    L >> 8, L & 0xff                       address length
offset 4..     address UTF-8 bytes                    claimant
after address  32 raw bytes                           identity hash
last 32 bytes  16 zero bytes + amount.to_be_bytes()    amount
```

The hash input is the concatenation of those rows. There are no delimiters.
There is no newline at the end. There is no JSON object wrapper. There is no
separate event identifier field in the leaf because one deployed escrow is
designed for one event on one chain and the root is stored in that escrow.

The output of SHA-256 is treated as a 32-byte digest. When compared or sorted,
digest bytes use ordinary lexicographic ascending order from byte zero to byte
31.

## Internal nodes

An internal node is:

```text
node = SHA256(0x01 || lower_digest || higher_digest)
```

Both child digests are exactly 32 bytes. Compare them lexicographically as raw
bytes. Append the smaller digest first and the larger digest second. The proof
does not carry a left/right bit because sorting makes the operation
commutative for this tree protocol.

The node prefix is one raw byte `0x01`. It is not the ASCII character `"1"`.
The children are not hex strings, and the pair is not sorted by a stringified
representation.

For every proof sibling, a verifier performs:

1. convert the current digest and sibling to 32 raw bytes;
2. compare the two byte arrays lexicographically;
3. append `0x01`, the lower array, and the higher array;
4. SHA-256 the resulting 65-byte preimage; and
5. use the resulting 32-byte digest as the next current value.

After all siblings are consumed, the current digest must equal the stored root
byte-for-byte.

## Tree construction

The publisher should construct a canonical tree as follows:

1. compute every leaf digest using the leaf format above;
2. sort the leaf digests lexicographically;
3. pair adjacent digests;
4. hash each pair with the internal-node rule;
5. promote an unpaired final digest unchanged; and
6. repeat until one digest remains as the root.

An odd level does not duplicate its final digest. Promotion means the final
digest is copied to the next level without hashing at that level. If it later
has a sibling, the normal `0x01` sorted-pair rule applies.

The proof for a target leaf contains only siblings encountered while the target
rises. If the target is promoted at a level, no sibling is appended for that
level. The proof is therefore not necessarily `ceil(log2(number_of_leaves))`
items for a non-power-of-two tree.

Duplicate leaf digests should be rejected by the publisher because target
lookup becomes ambiguous. Binding claimant, pool, identity hash, and amount
normally makes accidental duplicates unlikely, but uniqueness remains an
off-chain input invariant.

## Pseudocode

The following language-neutral pseudocode describes the protocol:

```text
function leaf(pool, address, identity_hash, amount):
    require amount > 0
    p = bytes([0x00])
    p += bytes([pool == Contributor ? 0x00 : 0x01])
    a = canonical_address_to_string(address).utf8_bytes()
    require len(a) <= 65535
    p += uint16_be(len(a))
    p += a
    p += identity_hash[0..32]
    p += uint128_positive_right_aligned_be32(amount)
    return sha256(p)

function parent(left, right):
    if lexicographic(left, right) <= 0:
        ordered = left || right
    else:
        ordered = right || left
    return sha256(bytes([0x01]) || ordered)

function verify(root, target, siblings):
    current = target
    for sibling in siblings:
        current = parent(current, sibling)
    return current == root
```

The pseudocode's `||` operator always means byte concatenation. The `utf8`
operation produces bytes, not a string object that is serialized by a runtime.

## Cross-language implementation checklist

Before publishing a root from another language, verify:

- SHA-256 is the standard 32-byte digest algorithm;
- the leaf prefix is one byte `00`;
- the pool byte is one byte `00` or `01`;
- the address is canonical Soroban `to_string()` output;
- the address length counts bytes and is a two-byte big-endian integer;
- the address has no NUL terminator;
- the identity hash contributes 32 raw bytes;
- the amount contributes 32 big-endian bytes;
- positive amounts are rejected before hashing;
- no text separators or serialization envelope is inserted;
- internal nodes use one byte `01`;
- child digests are sorted as raw 32-byte arrays;
- odd final nodes are promoted, not duplicated;
- duplicate leaves are rejected; and
- the resulting digest matches the contract's `leaf` call.

A golden vector should include the input values, every preimage field, the
complete leaf preimage in hex, and the resulting digest in hex. Keeping only
the final digest makes it difficult to identify whether a mismatch came from
the address, the length, the identity commitment, or the amount.

## Golden-vector review

A useful vector set contains:

| Vector | What it proves |
|---|---|
| Contributor, small amount | Basic field order and pool byte |
| Maintainer, same inputs | Pool binding changes the digest |
| Address length below 256 | High length byte remains zero |
| Address length at a byte boundary | Both length bytes are tested |
| Amount `1` | Right alignment and zero padding |
| Amount `256` | Big-endian order is visible |
| Amount near `i128::MAX` | High 16 bytes remain zero and no truncation occurs |
| Two equal leaf inputs | Determinism |
| One changed identity byte | Full identity field is included |
| One changed amount byte | Full amount field is included |
| One changed pool byte | Pool separation |

For tree vectors, include one, two, three, and a non-power-of-two number of
leaves. Record the sorted leaf list, each level, promoted values, sibling path,
and final root. This catches a promotion error that a single balanced tree can
miss.

## Common incompatible formats

The following formats look plausible but are not this protocol:

```text
SHA256("0" || "Contributor" || address || hex(identity_hash) || "10")
SHA256(0x00 || XDR(address) || identity_hash || amount.to_le_bytes())
SHA256(0x00 || pool || address || identity_hash || variable_amount_bytes)
SHA256(0x00 || pool || address || identity_hash || amount_u256_text)
SHA256(0x00 || pool || address_length_le16 || address || identity_hash || amount)
```

They differ in one or more of domain encoding, enum encoding, address encoding,
length framing, identity representation, byte order, or amount width. A client
must implement the canonical format explicitly rather than adapting a generic
Merkle library's default serializer.

## Build and test commands

From the repository root, the supported local commands are:

```sh
cargo test
cargo build --target wasm32-unknown-unknown --release
stellar contract build
```

The wasm target is pinned in `rust-toolchain.toml`. The release build is the
artifact check; a host build alone does not prove that the contract compiles to
the deployment target. `stellar contract build` is an additional supported
Stellar CLI route and requires the CLI to be installed.

The repository workflow also runs:

```sh
cargo fmt --all -- --check
cargo build --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p grainhack-escrow --target wasm32-unknown-unknown --release
```

The CI workflow does not construct a root, deploy a contract, or access a
production wallet. It verifies source and release compilation only.

## Claim-side debugging

When a valid-looking proof fails, debug in this order:

1. confirm the selected pool matches the published root;
2. compare the contract `leaf` result with the off-chain leaf digest;
3. compare the complete preimage bytes, not just rendered inputs;
4. confirm the address is canonical and its prefix is byte length;
5. confirm the identity hash is 32 raw bytes;
6. confirm the amount is positive and right-aligned big-endian;
7. compare the sorted child order at every proof level;
8. check whether an odd node was promoted rather than duplicated; and
9. verify the stored root and the proof's final digest byte-for-byte.

Do not fix a mismatch by trying both pool values, changing endianness during
verification, or accepting a root that differs by one byte. Those approaches
hide an integration bug and weaken the protocol boundary.

## Privacy review

The leaf includes a claimant address and amount by commitment. The claimant
address becomes public when the claimant calls `claim`; the identity hash and
root are also public inputs or outputs around the claim flow. The per-event salt
is intentionally not part of the public contract state.

The format provides binding and domain separation. It does not provide
anonymous claims, encrypted payouts, or protection against an observer with
side information. A client or backend must not describe the identity hash as a
zero-knowledge proof.

Do not place the salt in:

- the README;
- contract metadata;
- a root publication event;
- a claim event;
- a client-side URL;
- an exception message; or
- a committed source file used for production deployment.

## Compatibility policy

The field order and encodings are a deployed protocol. Changes require a new
contract or an explicit versioned root format; silently changing the README or
backend serializer is not a migration. The current contract has no leaf-format
version byte beyond the fixed domain prefix.

If a future format is needed, document a distinct domain prefix, preserve the
old verifier for already-published roots, and add vectors for both formats.
Never make a verifier accept multiple ambiguous encodings under one root
without a version discriminator.

The contract's one-event, one-chain design intentionally omits `chain_id` and
`event_id` from the leaf. Adding either field only in an off-chain builder would
break every proof. A future multi-event deployment should define those fields
as a new version rather than appending them ad hoc.

## Reviewer checklist

Reviewers of a client implementation should confirm:

- the code shows every field append in order;
- the address representation comes from a canonical SDK operation;
- the length prefix is visibly big-endian;
- the amount conversion is fixed-width and integer-only;
- negative and zero amounts cannot become published leaves;
- hashes are compared as bytes, not locale-sensitive strings;
- the tree handles one-leaf and odd-level cases;
- the proof excludes promoted-node placeholders;
- duplicate leaves are rejected;
- a contract `leaf` call is used in at least one integration test; and
- the test vectors include the preimage as well as the digest.

Reviewers of a README change should additionally confirm:

- every command is runnable from the repository root;
- `stellar contract build` is called out as a supported route;
- no link points to a file absent from this repository;
- the README does not claim the chain calculates awards;
- the README does not expose a salt or identity mapping;
- field widths and byte orders are explicit; and
- the README agrees with `leaf_hash` rather than changing contract behavior.

## Maintenance checklist

When the contract hash code changes:

1. update this document and the README together;
2. regenerate or review golden preimage vectors;
3. test every field boundary;
4. test one, two, three, and odd tree sizes;
5. update the off-chain builder in the same protocol change;
6. document migration and compatibility behavior; and
7. include a pull-request note explaining why the protocol changed.

When only prose changes, compare the prose against the source function and
public `leaf` behavior. Do not infer a format from a previous README example.
The contract's byte appends are the authority.

## Final reference

For quick implementation reference, the protocol is:

```text
leaf preimage =
  1 byte  0x00
  1 byte  pool (0x00 contributor, 0x01 maintainer)
  2 bytes address UTF-8 length, unsigned big-endian
  L bytes canonical Address::to_string() UTF-8
  32 bytes identity_hash
  32 bytes positive i128 magnitude, big-endian, right-aligned

leaf digest = SHA256(leaf preimage)

node digest = SHA256(0x01 || lexicographically_lower_child ||
                     lexicographically_higher_child)

odd final node = promoted unchanged to the next level
```

Any implementation that cannot account for every byte in this reference is
not ready to publish a root or submit a claim. Use the public `leaf` entry point
and the repository tests to resolve uncertainty before a production event.
