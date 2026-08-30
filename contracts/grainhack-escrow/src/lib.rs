#![no_std]
//! GrainHack escrow, commitments and Merkle claims for one event on one chain.
//!
//! Companion to `Grainily_onchain_spec.md`. Everything here layers on top of
//! the off-chain system; none of it replaces judging, assignment or appeals.
//!
//! Two rules govern every decision in this file:
//!
//! **The chain never decides anything.** Judging, assignment, appeals and the
//! payout arithmetic stay off-chain and authoritative. This contract holds
//! funds and publishes commitments. It contains no business logic that could
//! disagree with the backend - it cannot compute a bucket, a unit value or a
//! payout, only honour a root the backend published.
//!
//! **Claims are pull-based.** Grainlify publishes a Merkle root; contributors
//! claim against it and pay their own gas. Nothing is ever pushed to a list of
//! addresses: a failed push mid-loop leaves an event half-paid with no clean
//! recovery, and pushing requires holding everyone's address.

use soroban_sdk::{
    contract, contracterror, contractimpl, contractmeta, contracttype, panic_with_error,
    symbol_short, token, Address, Bytes, BytesN, Env, Symbol, Vec,
};

// Embed a small, stable identity in every release artifact. The description
// deliberately matches the README. Soroban metadata values must be literals;
// the release helper compares this version with Cargo before inspection so a
// rebuilt wasm cannot silently advertise a stale release number.
contractmeta!(key = "name", val = "GrainHack escrow");
contractmeta!(key = "version", val = "0.1.0");
contractmeta!(
    key = "description",
    val =
        "Holds GrainHack prize pools, publishes commitments, and honours pull-based Merkle claims."
);
contractmeta!(
    key = "repository",
    val = "https://github.com/Grainlify/Stellar-Contracts"
);
contractmeta!(key = "contract", val = "grainhack-escrow");

// ---------------------------------------------------------------------------
// Domain separation
// ---------------------------------------------------------------------------

/// Prefix byte for a Merkle **leaf** hash.
///
/// Second-preimage resistance (§9). Without distinct prefixes for leaves and
/// internal nodes, a crafted internal node can be presented as a leaf: an
/// attacker who knows two sibling leaves can hash them, submit that digest as
/// though it were a leaf, and claim against a proof one level short. The
/// prefix makes the two hash domains disjoint, so an internal node can never
/// be interpreted as a leaf.
const LEAF_PREFIX: u8 = 0x00;

/// Prefix byte for an internal node hash.
const NODE_PREFIX: u8 = 0x01;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialised = 1,
    NotInitialised = 2,
    NotAuthorised = 3,
    /// A maintainer operation tried to draw on contributor funds, or vice
    /// versa. Structurally impossible by design; this exists so the attempt
    /// is a loud failure rather than an accounting drift.
    PoolMismatch = 4,
    InsufficientEscrow = 5,
    /// The event is not in a state where this operation is legitimate.
    WrongState = 6,
    RootNotPublished = 7,
    RootAlreadyPublished = 8,
    InvalidProof = 9,
    AlreadyClaimed = 10,
    /// A commitment for this key already exists. Draw-seed commits are
    /// write-once: a second one would make "which seed was committed?"
    /// unanswerable, which is the entire guarantee.
    CommitmentExists = 11,
    CommitmentMissing = 12,
    /// Timelock has not elapsed.
    TimelockActive = 13,
    InvalidAmount = 14,
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Which pool a balance belongs to. Two separate balances, never one balance
/// with accounting (§5.1) - a maintainer claim must be *structurally* unable
/// to reach contributor funds, mirroring the off-chain separate-budget test.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Pool {
    Contributor,
    Maintainer,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum State {
    /// Created, not yet funded.
    Open,
    /// Funded and running.
    Funded,
    /// Claim roots published; contributors may claim.
    Settled,
    /// Abandoned before settlement. Only the refund path is reachable.
    Cancelled,
}

#[contracttype]
pub enum Key {
    Config,
    State,
    /// Balance per pool, so the two can never be added together by accident.
    Balance(Pool),
    /// Published Merkle root per pool.
    Root(Pool),
    /// Total the root commits to, per pool.
    RootTotal(Pool),
    /// Arbitrary commitment: config hash, draw commit, draw reveal.
    Commitment(Symbol, Bytes),
    /// Claimed marker per leaf.
    Claimed(BytesN<32>),
    /// Ledger timestamp after which a sweep is permitted.
    SweepAfter,
}

#[contracttype]
#[derive(Clone)]
pub struct Config {
    /// Multisig or admin account permitted to fund, publish and cancel.
    ///
    /// A single address here is a deliberate simplification: Soroban's
    /// `require_auth` composes with a multisig account, so multisig is
    /// configured on the *account*, not re-implemented in the contract.
    /// §8 requires multisig for anything moving funds outside the claim path.
    pub admin: Address,
    /// The token contract this escrow holds.
    pub token: Address,
    /// Where a sweep or refund sends funds. Fixed at initialisation so it is
    /// published in advance and cannot be redirected later (§5.4).
    pub sweep_dest: Address,
    /// Ledger seconds after settlement before a sweep is permitted.
    pub sweep_delay: u64,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct GrainhackEscrow;

#[contractimpl]
impl GrainhackEscrow {
    /// One-time setup. The sweep destination is fixed here so it is knowable
    /// before anyone commits work, rather than chosen once the money is idle.
    pub fn initialise(
        env: Env,
        admin: Address,
        token: Address,
        sweep_dest: Address,
        sweep_delay: u64,
    ) {
        if env.storage().instance().has(&Key::Config) {
            panic_with_error!(&env, Error::AlreadyInitialised);
        }
        admin.require_auth();

        env.storage().instance().set(
            &Key::Config,
            &Config {
                admin: admin.clone(),
                token: token.clone(),
                sweep_dest: sweep_dest.clone(),
                sweep_delay,
            },
        );
        env.storage().instance().set(&Key::State, &State::Open);
        env.storage()
            .instance()
            .set(&Key::Balance(Pool::Contributor), &0i128);
        env.storage()
            .instance()
            .set(&Key::Balance(Pool::Maintainer), &0i128);

        env.events().publish(
            (symbol_short!("init"),),
            (admin, token, sweep_dest, sweep_delay),
        );
    }

    /// Fund one pool. Called once per pool, or repeatedly to top up.
    ///
    /// The pool is an explicit argument and the balances are stored under
    /// separate keys, so there is no code path that could move value from one
    /// to the other - the separation is structural rather than a rule someone
    /// has to remember (§5.1).
    pub fn fund(env: Env, pool: Pool, from: Address, amount: i128) {
        let cfg = Self::config(&env);
        from.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        let state = Self::state(&env);
        if state != State::Open && state != State::Funded {
            panic_with_error!(&env, Error::WrongState);
        }

        token::Client::new(&env, &cfg.token).transfer(
            &from,
            &env.current_contract_address(),
            &amount,
        );

        let balance = Self::balance_of(&env, pool);
        env.storage()
            .instance()
            .set(&Key::Balance(pool), &(balance + amount));
        env.storage().instance().set(&Key::State, &State::Funded);

        env.events()
            .publish((symbol_short!("funded"), pool), amount);
    }

    /// Publish a commitment: config hash, draw-seed commit, or draw-seed
    /// reveal.
    ///
    /// **Write-once per key.** A draw-seed commit that could be overwritten
    /// proves nothing - an operator could commit, see the applicants, then
    /// replace the commit with one whose seed produces a preferred winner.
    /// The whole value of commit-reveal is that the commit is immutable once
    /// made, so a second write is an error rather than an update.
    pub fn commit(env: Env, kind: Symbol, subject: Bytes, value: BytesN<32>) {
        let cfg = Self::config(&env);
        cfg.admin.require_auth();

        let key = Key::Commitment(kind.clone(), subject.clone());
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, Error::CommitmentExists);
        }
        env.storage().persistent().set(&key, &value);

        env.events().publish((symbol_short!("commit"), kind), value);
    }

    /// Read a commitment back. Returns `None` when absent, so a verifier can
    /// distinguish "never committed" from "committed to zero".
    pub fn get_commitment(env: Env, kind: Symbol, subject: Bytes) -> Option<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Key::Commitment(kind, subject))
    }

    /// Publish a pool's Merkle claim root.
    ///
    /// Preconditions mirror the off-chain ones and are enforced here
    /// *independently* (§5.4: chain guard and backend guard are separate, and
    /// both must hold). This contract cannot check that appeals closed - that
    /// is off-chain knowledge - but it can and does refuse a root the escrow
    /// cannot honour, which is the failure that would strand claimants.
    ///
    /// Write-once. A published root is what contributors claim against; a
    /// second one would either invalidate proofs already served or silently
    /// change what someone is owed, and an on-chain root cannot be quietly
    /// corrected.
    pub fn publish_root(env: Env, pool: Pool, root: BytesN<32>, total: i128) {
        let cfg = Self::config(&env);
        cfg.admin.require_auth();

        if total <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if env.storage().persistent().has(&Key::Root(pool)) {
            panic_with_error!(&env, Error::RootAlreadyPublished);
        }
        let state = Self::state(&env);
        if state != State::Funded && state != State::Settled {
            panic_with_error!(&env, Error::WrongState);
        }

        // The escrow must already hold what the root promises. Publishing a
        // root larger than the balance would mean the last claimants find
        // nothing left, having been told on-chain that they were owed it.
        let balance = Self::balance_of(&env, pool);
        if total > balance {
            panic_with_error!(&env, Error::InsufficientEscrow);
        }

        env.storage().persistent().set(&Key::Root(pool), &root);
        env.storage()
            .persistent()
            .set(&Key::RootTotal(pool), &total);
        env.storage().instance().set(&Key::State, &State::Settled);

        // The sweep clock starts at settlement, not at funding.
        let sweep_after = env.ledger().timestamp() + cfg.sweep_delay;
        env.storage().instance().set(&Key::SweepAfter, &sweep_after);

        env.events()
            .publish((symbol_short!("root"), pool), (root, total));
    }

    /// Claim against a published root.
    ///
    /// The contributor submits this themselves and pays their own gas, so
    /// their address appears on-chain only by their own action (§4: never
    /// publish `github_login -> wallet address`; the leaf commits to a salted
    /// hash of the identity instead).
    ///
    /// Ordering is the security property (§9): verify the proof, check the
    /// claimed flag, **set** the claimed flag, then transfer. Setting before
    /// the external call is what makes reentrancy uninteresting.
    pub fn claim(
        env: Env,
        pool: Pool,
        claimant: Address,
        identity_hash: BytesN<32>,
        amount: i128,
        proof: Vec<BytesN<32>>,
    ) {
        let cfg = Self::config(&env);
        claimant.require_auth();

        if Self::state(&env) != State::Settled {
            panic_with_error!(&env, Error::WrongState);
        }
        if amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }

        let root: BytesN<32> = env
            .storage()
            .persistent()
            .get(&Key::Root(pool))
            .unwrap_or_else(|| panic_with_error!(&env, Error::RootNotPublished));

        // The leaf binds the identity hash, the claiming address and the
        // amount together. Binding the address is what stops a valid proof
        // being replayed by someone else: the leaf only verifies for the
        // address it was built for.
        let leaf = Self::leaf_hash(&env, &pool, &claimant, &identity_hash, amount);

        if env.storage().persistent().has(&Key::Claimed(leaf.clone())) {
            panic_with_error!(&env, Error::AlreadyClaimed);
        }
        if !Self::verify_proof(&env, &root, &leaf, &proof) {
            panic_with_error!(&env, Error::InvalidProof);
        }

        let balance = Self::balance_of(&env, pool);
        if amount > balance {
            panic_with_error!(&env, Error::InsufficientEscrow);
        }

        // Mark before transferring.
        env.storage()
            .persistent()
            .set(&Key::Claimed(leaf.clone()), &true);
        env.storage()
            .instance()
            .set(&Key::Balance(pool), &(balance - amount));

        token::Client::new(&env, &cfg.token).transfer(
            &env.current_contract_address(),
            &claimant,
            &amount,
        );

        env.events()
            .publish((symbol_short!("claim"), pool), (claimant, amount));
    }

    /// Whether a leaf has been claimed. Public so the backend's reconciliation
    /// job can compare on-chain state against `claim_leaves` (§8).
    pub fn is_claimed(env: Env, leaf: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .get(&Key::Claimed(leaf))
            .unwrap_or(false)
    }

    /// Cancel before settlement, opening the refund path.
    ///
    /// Deliberately unreachable once settled: after a root is published,
    /// contributors have been told on-chain what they are owed, and a cancel
    /// that could still fire would make that promise revocable.
    pub fn cancel(env: Env) {
        let cfg = Self::config(&env);
        cfg.admin.require_auth();

        if Self::state(&env) == State::Settled {
            panic_with_error!(&env, Error::WrongState);
        }
        env.storage().instance().set(&Key::State, &State::Cancelled);

        let sweep_after = env.ledger().timestamp() + cfg.sweep_delay;
        env.storage().instance().set(&Key::SweepAfter, &sweep_after);

        env.events().publish((symbol_short!("cancel"),), ());
    }

    /// Sweep what remains in one pool to the fixed destination.
    ///
    /// This is the only path other than a claim that moves value out, and it
    /// is deliberately narrow: admin-authorised, time-locked, reachable only
    /// from `Settled` or `Cancelled`, and paying a destination fixed at
    /// initialisation. A general admin withdrawal would make the escrow
    /// meaningless - it would be a promise the holder could take back.
    ///
    /// Covers both §5.4's unclaimed sweep and §11-#7's zero-claimant refund:
    /// a pool with no eligible claimants is simply one that is entirely
    /// unclaimed, so it needs no separate mechanism.
    pub fn sweep(env: Env, pool: Pool) -> i128 {
        let cfg = Self::config(&env);
        cfg.admin.require_auth();

        let state = Self::state(&env);
        if state != State::Settled && state != State::Cancelled {
            panic_with_error!(&env, Error::WrongState);
        }

        let sweep_after: u64 = env
            .storage()
            .instance()
            .get(&Key::SweepAfter)
            .unwrap_or(u64::MAX);
        if env.ledger().timestamp() < sweep_after {
            panic_with_error!(&env, Error::TimelockActive);
        }

        let balance = Self::balance_of(&env, pool);
        if balance <= 0 {
            return 0;
        }

        env.storage().instance().set(&Key::Balance(pool), &0i128);
        token::Client::new(&env, &cfg.token).transfer(
            &env.current_contract_address(),
            &cfg.sweep_dest,
            &balance,
        );

        env.events()
            .publish((symbol_short!("sweep"), pool), balance);
        balance
    }

    // -----------------------------------------------------------------------
    // Reads
    // -----------------------------------------------------------------------

    /// Returns the escrowed balance for `pool`.
    ///
    /// Reads `Key::Balance(pool)` from instance storage. Unlike the other
    /// accessors, this does **not** panic if the contract is uninitialised —
    /// it delegates to `balance_of`, which uses `unwrap_or(0)` and returns `0`
    /// rather than `Error::NotInitialised`. A return value of `0` therefore
    /// does not distinguish "empty pool" from "not set up".
    pub fn balance(env: Env, pool: Pool) -> i128 {
        Self::balance_of(&env, pool)
    }

    /// Returns the current escrow `State`.
    ///
    /// Reads `Key::State` from instance storage. Panics with
    /// `Error::NotInitialised` if the contract has not been initialised.
    pub fn get_state(env: Env) -> State {
        Self::state(&env)
    }

    /// Returns the published Merkle root for `pool`, if any.
    ///
    /// Reads `Key::Root(pool)` from persistent storage. Returns `None` when
    /// no root has been published for the given pool.
    pub fn get_root(env: Env, pool: Pool) -> Option<BytesN<32>> {
        env.storage().persistent().get(&Key::Root(pool))
    }

    /// The leaf hash for a claim, exposed so the backend can build leaves with
    /// the identical construction.
    ///
    /// §1 requires every on-chain artefact to be reproducible off-chain. This
    /// being callable is what makes the backend's Merkle builder testable
    /// against the contract rather than against a second implementation of the
    /// same rules that can drift.
    pub fn leaf(
        env: Env,
        pool: Pool,
        claimant: Address,
        identity_hash: BytesN<32>,
        amount: i128,
    ) -> BytesN<32> {
        Self::leaf_hash(&env, &pool, &claimant, &identity_hash, amount)
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    fn config(env: &Env) -> Config {
        env.storage()
            .instance()
            .get(&Key::Config)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialised))
    }

    fn state(env: &Env) -> State {
        env.storage()
            .instance()
            .get(&Key::State)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialised))
    }

    fn balance_of(env: &Env, pool: Pool) -> i128 {
        env.storage()
            .instance()
            .get(&Key::Balance(pool))
            .unwrap_or(0)
    }

    /// `leaf = H(0x00 || pool || len(address) || address || identity_hash || amount_be32)`
    ///
    /// The canonical construction (spec §13.1), identical in field order and
    /// encoding to the backend's `ClaimLeaf.Hash`. It is pinned from both sides
    /// against a shared vector; the two drifted once, which is what the vector
    /// exists to prevent.
    ///
    /// Three things here are deliberate and were changed from the original:
    ///
    /// **The address is hashed as its canonical strkey string, not XDR.** XDR
    /// forced every off-chain builder to implement Stellar's encoding - the
    /// chain-specific leakage §3.1 exists to prevent - and it has no meaning at
    /// all on EVM, so the second chain would have had to invent something
    /// anyway. The string is what both sides already hold.
    ///
    /// **The address length is prefixed.** Concatenating variable-length fields
    /// without it leaves the boundaries unrecoverable, so two different
    /// entitlements can produce the same digest. The backend's construction
    /// demonstrably did.
    ///
    /// **The amount is 32-byte big-endian**, not 16. Wide enough for EVM's
    /// uint256 so no chain narrows what another can express. Fixed-width so
    /// "10" and "10.0" cannot differ (§9).
    ///
    /// The pool byte stays, and stays first: it is what makes a contributor
    /// leaf structurally unable to verify against the maintainer root.
    ///
    /// **chain_id and event_id are deliberately not here.** One deployed escrow
    /// is one event on one chain and the root lives inside it, so a leaf built
    /// elsewhere has nowhere to be replayed. Adding them would mean storing and
    /// canonicalising two strings this contract cannot otherwise verify, to
    /// re-derive scoping it already has structurally. Do not add them.
    fn leaf_hash(
        env: &Env,
        pool: &Pool,
        claimant: &Address,
        identity_hash: &BytesN<32>,
        amount: i128,
    ) -> BytesN<32> {
        let mut buf = Bytes::new(env);
        buf.push_back(LEAF_PREFIX);
        buf.push_back(match pool {
            Pool::Contributor => 0u8,
            Pool::Maintainer => 1u8,
        });

        // Canonical string form of the address, length-prefixed big-endian.
        let addr = claimant.to_string();
        let addr_len = addr.len() as usize;
        let mut addr_buf = [0u8; 64];
        addr.copy_into_slice(&mut addr_buf[..addr_len]);
        buf.push_back(((addr_len >> 8) & 0xff) as u8);
        buf.push_back((addr_len & 0xff) as u8);
        buf.append(&Bytes::from_slice(env, &addr_buf[..addr_len]));

        buf.append(&Bytes::from_slice(env, &identity_hash.to_array()));
        buf.append(&Bytes::from_slice(env, &Self::amount_be32(amount)));
        env.crypto().sha256(&buf).into()
    }

    /// An amount as 32-byte big-endian, right-aligned.
    ///
    /// A negative amount is rendered as zero rather than sign-extended. Every
    /// caller already rejects non-positive amounts before reaching here; what
    /// must never happen is a negative silently becoming an enormous positive.
    fn amount_be32(amount: i128) -> [u8; 32] {
        let mut out = [0u8; 32];
        if amount <= 0 {
            return out;
        }
        let be = amount.to_be_bytes();
        out[16..].copy_from_slice(&be);
        out
    }

    /// Verify a Merkle proof, hashing sorted pairs with the internal-node
    /// prefix.
    ///
    /// Sorting the pair removes the need to transmit left/right position with
    /// each step. The node prefix is what keeps that safe: without it, sorted
    /// pairs plus an undifferentiated hash lets an internal node masquerade as
    /// a leaf (§9).
    fn verify_proof(
        env: &Env,
        root: &BytesN<32>,
        leaf: &BytesN<32>,
        proof: &Vec<BytesN<32>>,
    ) -> bool {
        let mut computed = leaf.to_array();

        for sibling in proof.iter() {
            let sib = sibling.to_array();
            let mut buf = Bytes::new(env);
            buf.push_back(NODE_PREFIX);
            // Lexicographic ordering, so the prover need not say which side
            // each sibling was on.
            if computed <= sib {
                buf.append(&Bytes::from_slice(env, &computed));
                buf.append(&Bytes::from_slice(env, &sib));
            } else {
                buf.append(&Bytes::from_slice(env, &sib));
                buf.append(&Bytes::from_slice(env, &computed));
            }
            computed = env.crypto().sha256(&buf).to_array();
        }

        computed == root.to_array()
    }
}

mod test;
