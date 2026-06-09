// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! BBG state: the 11-dimensional authenticated state of the cybergraph.

use std::collections::{BTreeMap, BTreeSet};

/// Number of blocks per epoch. Pruning runs at epoch boundaries.
pub const EPOCH_BLOCKS: u64 = 100;

use hemera::hash as hemera_hash;
use lens::Commitment;
use nebu::Goldilocks;

use crate::dim::{
    bbg_poly_commit, commit_dim, goldilocks_from_bytes32, goldilocks_from_u64,
};
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
    pub root: Particle,
}

impl BbgState {
    /// Create empty state. Root is hemera hash of empty commitments.
    pub fn new() -> Self {
        let empty_root = *hemera_hash(b"bbg-empty-state")
            .as_bytes()
            .first_chunk::<32>()
            .unwrap_or(&[0u8; 32]);
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
            root: empty_root,
        }
    }

    /// Compute BBG_root = H(commit(BBG_poly) ‖ commit(A) ‖ commit(N) ‖ commit(stats)).
    ///
    /// The graph statistics commitment is folded in so inf's cost model and
    /// recursion bounds rest on proven values (see [[stats]]).
    pub fn compute_root(&self) -> Particle {
        let dim_commits = [
            self.commit_particles(),
            self.commit_axons_out(),
            self.commit_axons_in(),
            self.commit_neurons(),
            self.commit_locations(),
            self.commit_coins(),
            self.commit_cards(),
            self.commit_files(),
            self.commit_time(),
            self.commit_signals(),
            self.commit_balances(),
        ];
        let bbg_poly_particle = bbg_poly_commit(&dim_commits);
        let a_commit = self.commit_a();
        let n_commit = self.commit_n();
        let stats_commit = self.statistics().commit();

        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(&bbg_poly_particle);
        buf.extend_from_slice(a_commit.as_bytes());
        buf.extend_from_slice(n_commit.as_bytes());
        buf.extend_from_slice(&stats_commit);

        let hash = hemera_hash(&buf);
        let hash_bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        let len = hash_bytes.len().min(32);
        out[..len].copy_from_slice(&hash_bytes[..len]);
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
    }

    // ── dimension serializers ─────────────────────────────────────

    fn commit_particles(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .particles
            .iter()
            .map(|(k, v)| {
                let vals = vec![
                    goldilocks_from_u64(v.energy),
                    goldilocks_from_u64(v.pi_star),
                    goldilocks_from_u64(v.weight),
                    goldilocks_from_u64(v.s_yes),
                    goldilocks_from_u64(v.s_no),
                    goldilocks_from_u64(v.meta_score),
                ];
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_axons_out(&self) -> Commitment {
        // Serialize each adjacency list: key + count + each particle (4 field elems each)
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .axons_out
            .iter()
            .map(|(k, v)| {
                let mut vals = vec![goldilocks_from_u64(v.len() as u64)];
                for particle in v {
                    vals.extend_from_slice(&goldilocks_from_bytes32(particle));
                }
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_axons_in(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .axons_in
            .iter()
            .map(|(k, v)| {
                let mut vals = vec![goldilocks_from_u64(v.len() as u64)];
                for particle in v {
                    vals.extend_from_slice(&goldilocks_from_bytes32(particle));
                }
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_neurons(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .neurons
            .iter()
            .map(|(k, v)| {
                let vals = vec![
                    goldilocks_from_u64(v.focus),
                    goldilocks_from_u64(v.karma),
                    goldilocks_from_u64(v.stake),
                ];
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_locations(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .locations
            .iter()
            .map(|(k, v)| {
                // i32 stored as u64 via bit-cast
                let vals = vec![
                    goldilocks_from_u64(v.lat as u32 as u64),
                    goldilocks_from_u64(v.lon as u32 as u64),
                ];
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_coins(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .coins
            .iter()
            .map(|(k, v)| ((*k), vec![goldilocks_from_u64(v.total_supply)]))
            .collect();
        commit_dim(&entries)
    }

    fn commit_cards(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .cards
            .iter()
            .map(|(k, v)| {
                let mut vals: Vec<Goldilocks> = goldilocks_from_bytes32(&v.owner).to_vec();
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.particle));
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_files(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .files
            .iter()
            .map(|(k, v)| {
                let vals = vec![
                    goldilocks_from_u64(v.available as u64),
                    goldilocks_from_u64(v.chunk_count as u64),
                ];
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_time(&self) -> Commitment {
        // Key: height as 32-byte key (8 bytes LE padded)
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .time
            .iter()
            .map(|(h, particle)| {
                let mut key = [0u8; 32];
                key[..8].copy_from_slice(&h.to_le_bytes());
                let vals: Vec<Goldilocks> = goldilocks_from_bytes32(particle).to_vec();
                (key, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_signals(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .signals
            .iter()
            .map(|(step, v)| {
                let mut key = [0u8; 32];
                key[..8].copy_from_slice(&step.to_le_bytes());
                let mut vals: Vec<Goldilocks> = goldilocks_from_bytes32(&v.neuron).to_vec();
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.network));
                vals.push(goldilocks_from_u64(v.link_count as u64));
                vals.push(goldilocks_from_u64(v.block_height));
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.proof_hash));
                (key, vals)
            })
            .collect();
        commit_dim(&entries)
    }

    fn commit_balances(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .balances
            .iter()
            .map(|(k, v)| (*k, vec![goldilocks_from_u64(*v)]))
            .collect();
        commit_dim(&entries)
    }

    fn commit_a(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .commitments
            .iter()
            .map(|(k, v)| (*k, vec![*v]))
            .collect();
        commit_dim(&entries)
    }

    fn commit_n(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .nullifiers
            .iter()
            .map(|k| (*k, vec![goldilocks_from_u64(1)]))
            .collect();
        commit_dim(&entries)
    }

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

        self.root = self.compute_root();
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
