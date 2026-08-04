// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! BBG state: the 11-dimensional authenticated state of the cybergraph.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

mod commits;

/// Number of blocks per epoch. Pruning runs at epoch boundaries.
pub const EPOCH_BLOCKS: u64 = 100;

use hemera::hash as hemera_hash;
use nebu::Goldilocks;

use crate::signal::{InsertError, Signal};
use crate::stats::{GraphStats, STAT_RELATIONS};
use crate::types::{
    CardRecord, Particle, CoinRecord, FileRecord, IntentRecord, LocationRecord, NeuronId,
    NeuronRecord, ParticleRecord, SignalRecord,
};

/// Compute the axon-particle id: H(from || to).
pub fn axon_id(from: &Particle, to: &Particle) -> Particle {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(from);
    buf[32..].copy_from_slice(to);
    let h = hemera_hash(&buf);
    let b = h.as_bytes();
    let mut out = [0u8; 32];
    out[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
    out
}

/// Compute the balance map key: H(owner_id || token_id).
pub fn balance_key(owner: &[u8; 32], token: &[u8; 32]) -> [u8; 32] {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(owner);
    buf[32..].copy_from_slice(token);
    let h = hemera_hash(&buf);
    let b = h.as_bytes();
    let mut out = [0u8; 32];
    out[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
    out
}

/// The full BBG state: 11 dimensions + private commitment sets.
pub struct BbgState {
    pub particles: BTreeMap<Particle, ParticleRecord>,
    pub axons_out: BTreeMap<Particle, Vec<Particle>>,
    pub axons_in: BTreeMap<Particle, Vec<Particle>>,
    pub neurons: BTreeMap<NeuronId, NeuronRecord>,
    pub locations: BTreeMap<Particle, LocationRecord>,
    pub coins: BTreeMap<Particle, CoinRecord>,
    pub cards: BTreeMap<Particle, CardRecord>,
    pub files: BTreeMap<Particle, FileRecord>,
    /// height → BBG_root snapshot
    pub time: BTreeMap<u64, Particle>,
    /// step → signal record
    pub signals: BTreeMap<u64, SignalRecord>,
    /// A(x): commit_point → value (private polynomial commitments)
    pub commitments: BTreeMap<[u8; 32], Goldilocks>,
    /// N(x): spent nullifiers
    pub nullifiers: BTreeSet<[u8; 32]>,
    /// balances: H(owner_id || token_id) → u64  (public opt-in balances)
    pub balances: BTreeMap<[u8; 32], u64>,
    /// intents: H(ν || h0 || scope_hash) → IntentRecord (unsealed declarations)
    pub intents: BTreeMap<Particle, IntentRecord>,
    /// Reverse map: axon_id → (from, to). Not committed; used for pruning.
    pub axon_edges: BTreeMap<Particle, (Particle, Particle)>,
    /// Tighter diameter bound installed by tru. None → use the trivial
    /// node_count−1 bound. Always an upper bound on the true diameter.
    pub diameter_override: Option<u64>,
    pub height: u64,
    /// Cached BBG_root. Empty ⇒ dirty: recomputed on the next [`Self::root`]
    /// read. `OnceLock` (not `Cell`) so `&BbgState` stays `Sync` — the
    /// `ProofLookProvider` in [[query]] requires it.
    root_cache: OnceLock<Particle>,
}

/// A pre-filled root cache holding `root`.
fn filled_cache(root: Particle) -> OnceLock<Particle> {
    let cache = OnceLock::new();
    let _ = cache.set(root);
    cache
}

impl BbgState {
    /// Create empty state. The root computes lazily through the same
    /// [`Self::compute_root`] path as every other state — no special constant.
    pub fn new() -> Self {
        Self {
            particles: BTreeMap::new(),
            axons_out: BTreeMap::new(),
            axons_in: BTreeMap::new(),
            neurons: BTreeMap::new(),
            locations: BTreeMap::new(),
            coins: BTreeMap::new(),
            cards: BTreeMap::new(),
            files: BTreeMap::new(),
            time: BTreeMap::new(),
            signals: BTreeMap::new(),
            commitments: BTreeMap::new(),
            nullifiers: BTreeSet::new(),
            balances: BTreeMap::new(),
            intents: BTreeMap::new(),
            axon_edges: BTreeMap::new(),
            diameter_override: None,
            height: 0,
            root_cache: OnceLock::new(),
        }
    }

    /// The current BBG_root.
    ///
    /// Lazily recomputed on the first read after a mutation, then cached
    /// until the next mutation. N inserts followed by one read cost one
    /// `compute_root`, and the value equals the root the eager per-insert
    /// path would have produced.
    pub fn root(&self) -> Particle {
        *self.root_cache.get_or_init(|| self.compute_root())
    }

    /// Recompute the root now and cache it. Eager equivalent of marking the
    /// cache dirty and immediately reading [`Self::root`].
    pub fn refresh_root(&mut self) -> Particle {
        let root = self.compute_root();
        self.root_cache = filled_cache(root);
        root
    }

    /// Invalidate the cached root after a mutation of a committed dimension.
    fn mark_root_dirty(&mut self) {
        self.root_cache = OnceLock::new();
    }

    /// The 14 leaves of the root preimage: the 11 dimension commitments (in
    /// `Dim` order), A (private commitments), N (nullifiers), stats.
    ///
    /// This is the exact structure zheng's look argument recomputes in-circuit
    /// (`zheng::root_from_leaves`), so a look opening can bind its dimension
    /// commitment to the root a nox program declares.
    pub fn root_leaves(&self) -> zheng::RootLeaves {
        let limbs = |c: &lens::Commitment| -> [Goldilocks; 4] {
            let mut b = [0u8; 32];
            let cb = c.as_bytes();
            let len = cb.len().min(32);
            b[..len].copy_from_slice(&cb[..len]);
            crate::dim::goldilocks_from_bytes32(&b)
        };
        zheng::RootLeaves {
            dims: [
                limbs(&self.commit_particles()),
                limbs(&self.commit_axons_out()),
                limbs(&self.commit_axons_in()),
                limbs(&self.commit_neurons()),
                limbs(&self.commit_locations()),
                limbs(&self.commit_coins()),
                limbs(&self.commit_cards()),
                limbs(&self.commit_files()),
                limbs(&self.commit_time()),
                limbs(&self.commit_signals()),
                limbs(&self.commit_balances()),
            ],
            a: limbs(&self.commit_a()),
            n: limbs(&self.commit_n()),
            stats: crate::dim::goldilocks_from_bytes32(&self.statistics().commit()),
        }
    }

    /// Compute BBG_root: the Poseidon2 compression chain over [`Self::root_leaves`].
    ///
    /// Field-native — the same limbs, permutation, and order as the in-circuit
    /// replay, so the root a program declares is the root the proof recomputes.
    pub fn compute_root(&self) -> Particle {
        let root = zheng::root_from_leaves(&self.root_leaves());
        let mut out = [0u8; 32];
        for (i, limb) in root.iter().enumerate() {
            out[i * 8..(i + 1) * 8].copy_from_slice(&limb.as_u64().to_le_bytes());
        }
        out
    }

    // ── committed graph statistics (bbg → inf interface) ──────────

    /// Compute the committed graph statistics from current state.
    ///
    /// node_count, relation_sizes, and max_degree are exact. diameter_bound is
    /// a sound upper bound: `diameter_override` if tru installed one, else the
    /// trivial connected-graph worst case `node_count − 1`.
    pub fn statistics(&self) -> GraphStats {
        let node_count = self.particles.len() as u64;

        let relation_sizes: [u64; STAT_RELATIONS] = [
            self.particles.len()  as u64,
            self.axons_out.len()  as u64,
            self.axons_in.len()   as u64,
            self.neurons.len()    as u64,
            self.locations.len()  as u64,
            self.coins.len()      as u64,
            self.cards.len()      as u64,
            self.files.len()      as u64,
            self.time.len()       as u64,
            self.signals.len()    as u64,
            self.balances.len()   as u64,
        ];

        let max_out = self.axons_out.values().map(|v| v.len()).max().unwrap_or(0);
        let max_in  = self.axons_in.values().map(|v| v.len()).max().unwrap_or(0);
        let max_degree = max_out.max(max_in) as u64;

        let diameter_bound = self
            .diameter_override
            .unwrap_or_else(|| node_count.saturating_sub(1));

        GraphStats { node_count, relation_sizes, max_degree, diameter_bound }
    }

    /// Install a tighter diameter bound (computed and proven by tru).
    ///
    /// Must be a sound upper bound on the true diameter — recursion in inf is
    /// only guaranteed to terminate correctly if this holds. Takes effect on
    /// the next `compute_root`.
    pub fn set_diameter_bound(&mut self, bound: u64) {
        self.diameter_override = Some(bound);
        self.mark_root_dirty();
    }

    // Dimension serializers live in [`commits`] (state/commits.rs).

    // ── signal insertion ─────────────────────────────────────────

    /// Insert a pre-validated signal into BBG state.
    ///
    /// cybergraph is responsible for all semantic validation (A1–A3, focus
    /// sufficiency, box ownership, conservation, VDF). BBG only enforces
    /// the structural double-spend invariant via N(x).
    pub fn insert(&mut self, signal: &Signal) -> Result<(), InsertError> {
        // Structural check: N(nullifier) = 0 → reject
        for mv in &signal.box_moves {
            if self.nullifiers.contains(&mv.nullifier) {
                return Err(InsertError::DoubleSpend);
            }
        }

        // Apply box movements
        for mv in &signal.box_moves {
            self.nullifiers.insert(mv.nullifier);
            if let Some((point, value)) = mv.commitment {
                self.commitments.insert(point, Goldilocks::new(value));
            }
        }

        // Apply each cyberlink ℓ = (p, q, τ, a, v)
        for link in &signal.links {
            let axon_id = axon_id(&link.from, &link.to);

            // particles[H(p,q)]: weight += a
            {
                let new_weight = self.particles.get(&axon_id)
                    .map_or(link.amount, |p| p.weight.saturating_add(link.amount));
                self.particles.entry(axon_id).or_insert(ParticleRecord::zero()).weight = new_weight;
            }

            // particles[q]: energy += a
            {
                let new_energy = self.particles.get(&link.to)
                    .map_or(link.amount, |p| p.energy.saturating_add(link.amount));
                self.particles.entry(link.to).or_insert(ParticleRecord::zero()).energy = new_energy;
            }

            // axons_out[p]: insert H(p,q)
            let out_list = self.axons_out.entry(link.from).or_default();
            if !out_list.contains(&axon_id) {
                out_list.push(axon_id);
            }

            // axons_in[q]: insert H(p,q)
            let in_list = self.axons_in.entry(link.to).or_default();
            if !in_list.contains(&axon_id) {
                in_list.push(axon_id);
            }

            // record reverse mapping for pruning
            self.axon_edges.entry(axon_id).or_insert((link.from, link.to));

            // neurons[ν]: focus -= cost (cost = amount; cybergraph already verified sufficiency)
            if let Some(nr) = self.neurons.get_mut(&signal.neuron) {
                nr.focus = nr.focus.saturating_sub(link.amount);
            }

            // balances[H(to || token)] += a  (public output)
            let to_key = balance_key(&link.to, &link.token);
            *self.balances.entry(to_key).or_insert(0) += link.amount;

            // balances[H(from || token)] -= a  (public input)
            let from_key = balance_key(&link.from, &link.token);
            let bal = self.balances.entry(from_key).or_insert(0);
            *bal = bal.saturating_sub(link.amount);
        }

        self.mark_root_dirty();
        Ok(())
    }

    // ── intent persistence ───────────────────────────────────────

    /// Persist an unsealed intent record at its inception height.
    ///
    /// Sync is responsible for validating the identity proof before calling.
    /// The record is keyed by H(ν ‖ h0 ‖ scope_hash) so identical intents
    /// dedupe and abandonment is observable.
    pub fn apply_intent(&mut self, intent: &IntentRecord) -> Particle {
        let key = intent_key(&intent.neuron, intent.h0, &intent.scope_hash);
        self.intents.insert(key, IntentRecord {
            neuron:     intent.neuron,
            h0:         intent.h0,
            scope_hash: intent.scope_hash,
            signature:  intent.signature,
        });
        key
    }

    /// Persist a signal header-only record (no cyberlinks applied).
    ///
    /// Used when the signal has already been validated and ordered by sync
    /// but the cyberlink batch is being applied separately (e.g., for sealing
    /// a previously-declared intent).
    pub fn apply_signal_record(&mut self, step: u64, record: SignalRecord) {
        self.signals.insert(step, record);
        self.mark_root_dirty();
    }

}

/// Compute the intent key = H(ν ‖ h0 ‖ scope_hash).
fn intent_key(neuron: &NeuronId, h0: u64, scope_hash: &Particle) -> Particle {
    let mut buf = [0u8; 32 + 8 + 32];
    buf[..32].copy_from_slice(neuron);
    buf[32..40].copy_from_slice(&h0.to_le_bytes());
    buf[40..].copy_from_slice(scope_hash);
    let h = hemera_hash(&buf);
    let b = h.as_bytes();
    let mut out = [0u8; 32];
    out[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
    out
}

impl Default for BbgState {
    fn default() -> Self {
        Self::new()
    }
}
