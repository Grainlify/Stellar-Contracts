#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, vec, Address, Bytes, BytesN, Env,
};


/// The contract panics with `panic_with_error!`, so a `try_*` call surfaces a
/// `soroban_sdk::Error` carrying the contract-error code rather than the enum
/// itself. This converts one for comparison, so tests still read in terms of
/// the named error.
fn contract_err(e: Error) -> Result<soroban_sdk::Error, soroban_sdk::InvokeError> {
    Ok(soroban_sdk::Error::from_contract_error(e as u32))
}

fn sha(env: &Env, parts: &[&[u8]]) -> BytesN<32> {
    let mut buf = Bytes::new(env);
    for p in parts {
        buf.append(&Bytes::from_slice(env, p));
    }
    env.crypto().sha256(&buf).into()
}

/// Build an internal node the same way the contract does: node prefix,
/// lexicographically sorted pair.
fn node(env: &Env, a: &BytesN<32>, b: &BytesN<32>) -> BytesN<32> {
    let (x, y) = (a.to_array(), b.to_array());
    if x <= y {
        sha(env, &[&[NODE_PREFIX], &x, &y])
    } else {
        sha(env, &[&[NODE_PREFIX], &y, &x])
    }
}

struct Fixture {
    env: Env,
    contract: Address,
    client: GrainhackEscrowClient<'static>,
    admin: Address,
    sponsor: Address,
    sweep_dest: Address,
    token: Address,
    token_admin: token::StellarAssetClient<'static>,
}

fn setup(sweep_delay: u64) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sponsor = Address::generate(&env);
    let sweep_dest = Address::generate(&env);

    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_addr = sac.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_addr);

    let contract = env.register_contract(None, GrainhackEscrow);
    let client = GrainhackEscrowClient::new(&env, &contract);
    client.initialise(&admin, &token_addr, &sweep_dest, &sweep_delay);

    token_admin.mint(&sponsor, &1_000_000_000i128);

    Fixture {
        env,
        contract,
        client,
        admin,
        sponsor,
        sweep_dest,
        token: token_addr,
        token_admin,
    }
}

// ---------------------------------------------------------------------------
// The separate-pool guarantee
// ---------------------------------------------------------------------------

/// §5.1: it must be structurally impossible for a maintainer claim to draw on
/// contributor funds. Separate balances, not one balance with accounting.
#[test]
fn pools_are_separate_balances() {
    let f = setup(0);
    f.client.fund(&Pool::Contributor, &f.sponsor, &600i128);
    f.client.fund(&Pool::Maintainer, &f.sponsor, &400i128);

    assert_eq!(f.client.balance(&Pool::Contributor), 600);
    assert_eq!(f.client.balance(&Pool::Maintainer), 400);

    // A root may not exceed its *own* pool, even though the contract holds
    // 1000 in total. This is the assertion that would fail if the two were
    // ever collapsed into one balance.
    let root = BytesN::from_array(&f.env, &[7u8; 32]);
    let res = f.client.try_publish_root(&Pool::Maintainer, &root, &500i128);
    assert_eq!(res, Err(contract_err(Error::InsufficientEscrow)));
}

/// A contributor leaf must not verify against the maintainer root. The pool is
/// part of the leaf, so the same person and amount hash differently per pool.
#[test]
fn a_leaf_from_one_pool_cannot_claim_from_the_other() {
    let f = setup(0);
    let alice = Address::generate(&f.env);
    let id = BytesN::from_array(&f.env, &[1u8; 32]);

    let contributor_leaf = f.client.leaf(&Pool::Contributor, &alice, &id, &100i128);
    let maintainer_leaf = f.client.leaf(&Pool::Maintainer, &alice, &id, &100i128);
    assert_ne!(contributor_leaf, maintainer_leaf);

    f.client.fund(&Pool::Contributor, &f.sponsor, &1_000i128);
    f.client.fund(&Pool::Maintainer, &f.sponsor, &1_000i128);
    // Publish the *contributor* leaf as the maintainer root.
    f.client.publish_root(&Pool::Maintainer, &contributor_leaf, &100i128);

    // Claiming from the maintainer pool builds a maintainer leaf, which does
    // not match the root that was published from a contributor leaf.
    let res = f.client.try_claim(
        &Pool::Maintainer,
        &alice,
        &id,
        &100i128,
        &vec![&f.env],
    );
    assert_eq!(res, Err(contract_err(Error::InvalidProof)));
}

// ---------------------------------------------------------------------------
// Claims
// ---------------------------------------------------------------------------

#[test]
fn claim_pays_once_and_only_once() {
    let f = setup(0);
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    let id_a = BytesN::from_array(&f.env, &[0xAA; 32]);
    let id_b = BytesN::from_array(&f.env, &[0xBB; 32]);

    f.client.fund(&Pool::Contributor, &f.sponsor, &300i128);

    let leaf_a = f.client.leaf(&Pool::Contributor, &alice, &id_a, &100i128);
    let leaf_b = f.client.leaf(&Pool::Contributor, &bob, &id_b, &200i128);
    let root = node(&f.env, &leaf_a, &leaf_b);
    f.client.publish_root(&Pool::Contributor, &root, &300i128);

    let token_client = token::Client::new(&f.env, &f.token);

    f.client.claim(&Pool::Contributor, &alice, &id_a, &100i128, &vec![&f.env, leaf_b.clone()]);
    assert_eq!(token_client.balance(&alice), 100);
    assert_eq!(f.client.balance(&Pool::Contributor), 200);
    assert!(f.client.is_claimed(&leaf_a));

    // A second attempt is refused rather than paying twice.
    let res = f.client.try_claim(
        &Pool::Contributor,
        &alice,
        &id_a,
        &100i128,
        &vec![&f.env, leaf_b.clone()],
    );
    assert_eq!(res, Err(contract_err(Error::AlreadyClaimed)));
    assert_eq!(token_client.balance(&alice), 100);

    f.client.claim(&Pool::Contributor, &bob, &id_b, &200i128, &vec![&f.env, leaf_a]);
    assert_eq!(token_client.balance(&bob), 200);
    assert_eq!(f.client.balance(&Pool::Contributor), 0);
}

/// A valid proof for one contributor must not pay a different address. The
/// leaf binds the claiming address, so a stolen proof verifies for nobody
/// else.
#[test]
fn a_proof_cannot_be_replayed_by_another_address() {
    let f = setup(0);
    let alice = Address::generate(&f.env);
    let thief = Address::generate(&f.env);
    let id_a = BytesN::from_array(&f.env, &[0xAA; 32]);
    let leaf_b = BytesN::from_array(&f.env, &[0xCC; 32]);

    f.client.fund(&Pool::Contributor, &f.sponsor, &300i128);
    let leaf_a = f.client.leaf(&Pool::Contributor, &alice, &id_a, &100i128);
    let root = node(&f.env, &leaf_a, &leaf_b);
    f.client.publish_root(&Pool::Contributor, &root, &100i128);

    let res = f.client.try_claim(
        &Pool::Contributor,
        &thief,
        &id_a,
        &100i128,
        &vec![&f.env, leaf_b],
    );
    assert_eq!(res, Err(contract_err(Error::InvalidProof)));
}

/// Claiming more than the leaf commits to must fail: the amount is inside the
/// hash, so any other amount is a different leaf.
#[test]
fn the_amount_is_bound_into_the_leaf() {
    let f = setup(0);
    let alice = Address::generate(&f.env);
    let id = BytesN::from_array(&f.env, &[0xAA; 32]);
    let sibling = BytesN::from_array(&f.env, &[0xDD; 32]);

    f.client.fund(&Pool::Contributor, &f.sponsor, &500i128);
    let leaf = f.client.leaf(&Pool::Contributor, &alice, &id, &100i128);
    let root = node(&f.env, &leaf, &sibling);
    f.client.publish_root(&Pool::Contributor, &root, &100i128);

    let res = f.client.try_claim(
        &Pool::Contributor,
        &alice,
        &id,
        &500i128, // more than the leaf says
        &vec![&f.env, sibling],
    );
    assert_eq!(res, Err(contract_err(Error::InvalidProof)));
}

/// §9 second-preimage resistance: an internal node must not be presentable as
/// a leaf. With domain separation the digest of two leaves is in a different
/// hash space from any leaf, so a one-level-short proof cannot verify.
#[test]
fn an_internal_node_cannot_be_claimed_as_a_leaf() {
    let f = setup(0);
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    let id_a = BytesN::from_array(&f.env, &[0xAA; 32]);
    let id_b = BytesN::from_array(&f.env, &[0xBB; 32]);

    f.client.fund(&Pool::Contributor, &f.sponsor, &1_000i128);

    let leaf_a = f.client.leaf(&Pool::Contributor, &alice, &id_a, &100i128);
    let leaf_b = f.client.leaf(&Pool::Contributor, &bob, &id_b, &200i128);
    let inner = node(&f.env, &leaf_a, &leaf_b);
    let other = BytesN::from_array(&f.env, &[0xEE; 32]);
    let root = node(&f.env, &inner, &other);
    f.client.publish_root(&Pool::Contributor, &root, &300i128);

    // An attacker who knows the tree tries to claim using the internal node's
    // preimage as though it were their leaf. It cannot be: leaves carry the
    // leaf prefix and nodes carry the node prefix, so no leaf ever hashes to
    // an internal node.
    let res = f.client.try_claim(
        &Pool::Contributor,
        &alice,
        &id_a,
        &300i128,
        &vec![&f.env, other],
    );
    assert_eq!(res, Err(contract_err(Error::InvalidProof)));
}

#[test]
fn claiming_before_a_root_is_published_fails() {
    let f = setup(0);
    let alice = Address::generate(&f.env);
    let id = BytesN::from_array(&f.env, &[1u8; 32]);
    f.client.fund(&Pool::Contributor, &f.sponsor, &100i128);

    let res = f.client.try_claim(&Pool::Contributor, &alice, &id, &100i128, &vec![&f.env]);
    assert_eq!(res, Err(contract_err(Error::WrongState)));
}

// ---------------------------------------------------------------------------
// Commitments
// ---------------------------------------------------------------------------

/// A draw-seed commit that could be overwritten proves nothing: an operator
/// could commit, watch the applicants arrive, then replace it with a seed that
/// produces a preferred winner.
#[test]
fn commitments_are_write_once() {
    let f = setup(0);
    let kind = symbol_short!("drawcmt");
    let subject = Bytes::from_slice(&f.env, b"issue-42");
    let first = BytesN::from_array(&f.env, &[1u8; 32]);
    let second = BytesN::from_array(&f.env, &[2u8; 32]);

    f.client.commit(&kind, &subject, &first);
    assert_eq!(f.client.get_commitment(&kind, &subject), Some(first.clone()));

    let res = f.client.try_commit(&kind, &subject, &second);
    assert_eq!(res, Err(contract_err(Error::CommitmentExists)));
    // And the original is untouched.
    assert_eq!(f.client.get_commitment(&kind, &subject), Some(first));
}

/// Absent must be distinguishable from committed-to-zero, or a verifier
/// cannot tell "never committed" from "committed a zero seed".
#[test]
fn an_absent_commitment_reads_as_none() {
    let f = setup(0);
    let kind = symbol_short!("drawcmt");
    let subject = Bytes::from_slice(&f.env, b"never-committed");
    assert_eq!(f.client.get_commitment(&kind, &subject), None);

    let zero = BytesN::from_array(&f.env, &[0u8; 32]);
    let other = Bytes::from_slice(&f.env, b"issue-1");
    f.client.commit(&kind, &other, &zero);
    assert_eq!(f.client.get_commitment(&kind, &other), Some(zero));
}

// ---------------------------------------------------------------------------
// Roots
// ---------------------------------------------------------------------------

#[test]
fn a_root_cannot_be_republished() {
    let f = setup(0);
    f.client.fund(&Pool::Contributor, &f.sponsor, &500i128);
    let a = BytesN::from_array(&f.env, &[1u8; 32]);
    let b = BytesN::from_array(&f.env, &[2u8; 32]);

    f.client.publish_root(&Pool::Contributor, &a, &100i128);
    let res = f.client.try_publish_root(&Pool::Contributor, &b, &100i128);
    assert_eq!(res, Err(contract_err(Error::RootAlreadyPublished)));
    assert_eq!(f.client.get_root(&Pool::Contributor), Some(a));
}

#[test]
fn a_root_larger_than_the_escrow_is_refused() {
    let f = setup(0);
    f.client.fund(&Pool::Contributor, &f.sponsor, &100i128);
    let root = BytesN::from_array(&f.env, &[1u8; 32]);

    let res = f.client.try_publish_root(&Pool::Contributor, &root, &101i128);
    assert_eq!(res, Err(contract_err(Error::InsufficientEscrow)));
}

// ---------------------------------------------------------------------------
// Sweep and refund
// ---------------------------------------------------------------------------

/// §5.4: sweeping is time-locked. Before the delay elapses it must fail, or
/// the timelock is decorative.
#[test]
fn sweep_is_timelocked() {
    let f = setup(1_000);
    f.client.fund(&Pool::Contributor, &f.sponsor, &500i128);
    let root = BytesN::from_array(&f.env, &[1u8; 32]);
    f.client.publish_root(&Pool::Contributor, &root, &500i128);

    let res = f.client.try_sweep(&Pool::Contributor);
    assert_eq!(res, Err(contract_err(Error::TimelockActive)));

    f.env.ledger().with_mut(|l| l.timestamp += 1_001);
    let swept = f.client.sweep(&Pool::Contributor);
    assert_eq!(swept, 500);
    assert_eq!(f.client.balance(&Pool::Contributor), 0);
    assert_eq!(
        token::Client::new(&f.env, &f.token).balance(&f.sweep_dest),
        500
    );
}

/// §11-#7: a pool ending with no eligible claimants is refunded. It needs no
/// separate mechanism - it is simply a pool that is entirely unclaimed.
#[test]
fn a_pool_with_no_claimants_refunds_through_the_same_path() {
    let f = setup(0);
    f.client.fund(&Pool::Maintainer, &f.sponsor, &750i128);
    f.client.cancel();

    let swept = f.client.sweep(&Pool::Maintainer);
    assert_eq!(swept, 750);
    assert_eq!(
        token::Client::new(&f.env, &f.token).balance(&f.sweep_dest),
        750
    );
}

/// Cancelling after settlement would make an on-chain promise revocable.
#[test]
fn a_settled_event_cannot_be_cancelled() {
    let f = setup(0);
    f.client.fund(&Pool::Contributor, &f.sponsor, &100i128);
    let root = BytesN::from_array(&f.env, &[1u8; 32]);
    f.client.publish_root(&Pool::Contributor, &root, &100i128);

    let res = f.client.try_cancel();
    assert_eq!(res, Err(contract_err(Error::WrongState)));
}

/// Sweeping while the event is still running would empty an escrow that
/// contributors are actively working against.
#[test]
fn sweep_is_unreachable_while_the_event_is_running() {
    let f = setup(0);
    f.client.fund(&Pool::Contributor, &f.sponsor, &100i128);

    let res = f.client.try_sweep(&Pool::Contributor);
    assert_eq!(res, Err(contract_err(Error::WrongState)));
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

#[test]
fn initialise_is_once_only() {
    let f = setup(0);
    let other = Address::generate(&f.env);
    let res = f.client.try_initialise(&other, &f.token, &other, &0u64);
    assert_eq!(res, Err(contract_err(Error::AlreadyInitialised)));
}

#[test]
fn funding_zero_or_negative_is_refused() {
    let f = setup(0);
    assert_eq!(f.client.try_fund(&Pool::Contributor, &f.sponsor, &0i128), Err(contract_err(Error::InvalidAmount)));
    assert_eq!(f.client.try_fund(&Pool::Contributor, &f.sponsor, &-5i128), Err(contract_err(Error::InvalidAmount)));
}

/// The leaf construction must be stable: the backend builds leaves off-chain
/// and every artefact must be reproducible (§1). If this changes, roots
/// already published stop verifying.
#[test]
fn leaf_construction_is_deterministic() {
    let f = setup(0);
    let alice = Address::generate(&f.env);
    let id = BytesN::from_array(&f.env, &[0x11; 32]);

    let a = f.client.leaf(&Pool::Contributor, &alice, &id, &1_000i128);
    let b = f.client.leaf(&Pool::Contributor, &alice, &id, &1_000i128);
    assert_eq!(a, b);

    // Any input change produces a different leaf.
    assert_ne!(a, f.client.leaf(&Pool::Contributor, &alice, &id, &1_001i128));
    assert_ne!(
        a,
        f.client.leaf(&Pool::Contributor, &Address::generate(&f.env), &id, &1_000i128)
    );
    assert_ne!(
        a,
        f.client.leaf(
            &Pool::Contributor,
            &alice,
            &BytesN::from_array(&f.env, &[0x12; 32]),
            &1_000i128
        )
    );
}

// Silence unused-field warnings for fixture members kept for readability.
#[allow(dead_code)]
fn _fixture_fields_used(f: &Fixture) {
    let _ = (&f.contract, &f.admin, &f.token_admin);
}

// ---------------------------------------------------------------------------
// Cross-implementation leaf pin
// ---------------------------------------------------------------------------

/// The leaf digest for a fixed set of inputs, pinned as a golden value.
///
/// `leaf()` is exported so the backend's Merkle builder can be tested against
/// this contract rather than against a second implementation of the same rules
/// that can drift. That cross-check was never wired up and the two did drift -
/// the contract hashed an XDR address and a 16-byte amount, the backend hashed
/// a UTF-8 address, an 8-byte amount, a chain id and an event id, and a root
/// built by one could not be claimed against the other.
///
/// Both now compute the canonical construction of spec §13.1 and this vector
/// holds the agreed digests. If it fails, one side moved: fix the side that
/// moved rather than updating the vector to make it pass.
///
/// The identical vector lives at
/// `Grainlify-Backend/internal/chain/testdata/leaf_vector.json`. The two copies
/// must stay byte-identical.
#[test]
fn leaf_matches_the_pinned_cross_implementation_vector() {
    let env = Env::default();
    let contract = env.register_contract(None, GrainhackEscrow);
    let client = GrainhackEscrowClient::new(&env, &contract);

    // A fixed, valid Stellar address rather than a generated one, so the
    // digest is reproducible across runs and machines.
    let claimant = Address::from_string(&soroban_sdk::String::from_str(
        &env,
        "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
    ));
    let identity = BytesN::from_array(&env, &[0x11u8; 32]);
    let amount = 1_500_000i128;

    let contributor = client.leaf(&Pool::Contributor, &claimant, &identity, &amount);
    assert_eq!(
        contributor,
        BytesN::from_array(
            &env,
            &[
                0xa1, 0x2f, 0x90, 0xc9, 0xb7, 0x23, 0x38, 0x97, 0x26, 0xf6, 0x3f, 0x70, 0xb9, 0xdd, 0x6f, 0x81, 0x5b, 0x45, 0xc6, 0x70, 0x7b, 0x22, 0xb5, 0xfc, 0x6d, 0xa2, 0x2d, 0xef, 0x73, 0x80, 0x55, 0xfe,
            ]
        ),
        "contributor leaf digest changed; update the vector in BOTH repositories deliberately"
    );

    // The pool byte is inside the hash, so the same entitlement in the other
    // pool is a different leaf. This is the property the Go builder lacks.
    let maintainer = client.leaf(&Pool::Maintainer, &claimant, &identity, &amount);
    assert_ne!(contributor, maintainer);
    assert_eq!(
        maintainer,
        BytesN::from_array(
            &env,
            &[
                0x17, 0x29, 0xf8, 0xad, 0x8f, 0x47, 0xe8, 0xfc, 0xf0, 0x15, 0xb0, 0x77, 0x09, 0x31, 0x11, 0xf5, 0xe1, 0x4f, 0x20, 0xa6, 0x64, 0x37, 0x3b, 0xaf, 0x6d, 0x49, 0x04, 0x1f, 0x59, 0x67, 0xbc, 0xea,
            ]
        ),
        "maintainer leaf digest changed; update the vector in BOTH repositories deliberately"
    );
}
