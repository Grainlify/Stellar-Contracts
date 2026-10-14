# Cross-Implementation Merkle & Leaf Vectors (`leaf_vector.json`)

This directory contains the machine-readable golden test vectors (`leaf_vector.json`) that pin the canonical GrainHack Merkle leaf and tree construction rules (spec §13.1) across the Soroban contract (`contracts/grainhack-escrow`), the Go backend (`Grainlify-Backend/internal/chain`), and sibling chain implementations.

---

## 1. Binary & Multi-Byte Encoding Rules

Every binary digest and hash input in `leaf_vector.json` follows one uniform convention:

| Quantity | Encoding | Byte Order / Width |
|---|---|---|
| 32-byte digests (`identity_hash_hex`, `expected_leaf_hex`, `leaves_hex`, `expected_root_hex`, `siblings`) | 64-character lowercase hexadecimal (no `0x` prefix) | Raw 32-byte array (`[u8; 32]`) |
| `pool_byte` | Single unsigned byte (`u8`) | `0x00` for `contributor`, `0x01` for `maintainer` |
| `claim_address` (Soroban) | Canonical Stellar strkey UTF-8 string (e.g. `GA5Z...`) | Preceded by a 2-byte **unsigned big-endian** length prefix (`u16_be`, `0x0038` for a 56-byte strkey) |
| `amount_minor` | Integer minor units (`i128` / `uint256`) | Serialized into the hash preimage as a **32-byte big-endian** unsigned integer (`amount_be32`). Non-positive values serialize as 32 zero bytes |
| Synthetic leaf index `i` | Unsigned 16-bit integer (`u16`) | Serialized as **2-byte big-endian** (`uint16_be(i)`) |

### Hash Domain Separation Prefixes

- `0x00` — **Leaf prefix** (`LEAF_PREFIX`):
  $$\text{leaf} = \text{SHA-256}(0\text{x}00 \parallel \text{pool\_u8} \parallel \text{uint16\_be}(\text{len}(\text{address})) \parallel \text{address} \parallel \text{identity\_hash\_32} \parallel \text{amount\_be32})$$
- `0x01` — **Internal node prefix** (`NODE_PREFIX`):
  $$\text{node}(a, b) = \text{SHA-256}(0\text{x}01 \parallel \min(a, b) \parallel \max(a, b))$$
  where $\min(a, b)$ and $\max(a, b)$ compare the two 32-byte child digests lexicographically.
- `0xFF` — **Synthetic tree-vector leaf prefix**:
  $$\text{synth\_leaf}(i) = \text{SHA-256}(0\text{xFF} \parallel \text{uint16\_be}(i)) \quad \text{for } i \in \{0, \dots, n - 1\}$$

---

## 2. Top-Level Fixture Schema (`leaf_vector.json`)

### `encoding`
Documents the canonical binary encoding (`lowercase_hex`) and big-endian integer widths (`u16_big_endian`, `u256_big_endian_32_bytes`).

### `inputs`, `leaf_contributor`, `leaf_maintainer`
The primary cross-repository pin shared with `Grainlify-Backend/internal/chain/testdata/leaf_vector.json`:
- `inputs.claim_address`: Canonical Stellar strkey (`"GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"`).
- `inputs.identity_hash_hex`: 32-byte identity hash as 64 lowercase hex chars.
- `inputs.amount_minor`: Exact integer amount in minor units (`1500000`).
- `leaf_contributor`: Expected 32-byte SHA-256 leaf digest when `pool = contributor` (`0x00`).
- `leaf_maintainer`: Expected 32-byte SHA-256 leaf digest when `pool = maintainer` (`0x01`).

### `leaf_vectors` (Array of Self-Contained Leaf Test Cases)
Each element provides all inputs needed to reproduce a leaf digest from scratch. Adding a new object to `leaf_vectors` automatically exercises it in `leaf_matches_the_pinned_cross_implementation_vector` without any Rust code changes:
- `name`: Human-readable identifier for failure messages.
- `pool`: `"contributor"` (`0x00`) or `"maintainer"` (`0x01`).
- `pool_byte`: Explicit numeric pool tag (`0` or `1`).
- `claim_address`: Canonical Stellar strkey string.
- `identity_hash_hex`: 64-character lowercase hex string (32 bytes).
- `amount_minor`: Exact integer amount in minor units.
- `expected_leaf_hex`: Expected 64-character lowercase hex SHA-256 leaf digest.

### `tree_vectors`
Pins the three Merkle tree construction rules across implementations:
1. **Ascending leaf sort**: Leaves are sorted lexicographically ascending by their 32-byte digest before level 0 is paired.
2. **Sorted-pair internal hashing**: Sibling pairs are ordered lexicographically and hashed with prefix `0x01`.
3. **Odd-node promotion**: When a tree level has an odd number of nodes, the final node is promoted unchanged to the next level (never duplicated).

Fields inside `tree_vectors`:
- `leaves_sample`: Map of generator index `"0"`, `"1"`, `"2"` to expected `synth_leaf(i)` hex digests so implementations can verify their synthetic leaf generator before testing tree construction.
- `roots`: Map of leaf count `"n"` (`"1"`, `"2"`, `"3"`, `"5"`, `"6"`, `"7"`, `"38"`) to expected 64-character lowercase hex root digest for `[synth_leaf(0), ..., synth_leaf(n - 1)]`.
- `cases`: Array of self-contained tree test vectors. Each entry contains:
  - `leaf_count`: Number of leaves `n`.
  - `leaves_hex`: The full ordered input list `[synth_leaf(0), ..., synth_leaf(n - 1)]` in generator order (**not** pre-sorted, so the builder's ascending leaf sort is exercised).
  - `expected_root_hex`: Expected 32-byte Merkle root in lowercase hex.
  Adding a new entry to `cases` (or `roots`) requires no Rust changes; `tree_roots_match_the_pinned_cross_implementation_vectors` iterates over all entries automatically.
- `proof`: Pinned inclusion proof for `n = 7` (the deepest small odd tree, crossing a promoted node):
  - `leaf_count`: `7`
  - `leaf`: The first leaf in sorted digest order (`synth_leaf(3)`).
  - `siblings`: Ordered array of 32-byte sibling digests from leaf to root.
  - `expected_root`: Expected root digest (`roots["7"]`).
