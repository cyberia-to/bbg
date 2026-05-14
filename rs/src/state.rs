// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! BBG state: the 10-dimensional authenticated state of the cybergraph.

use std::collections::{BTreeMap, BTreeSet};

/// Number of blocks per epoch. Decay and pruning run at epoch boundaries.
pub const EPOCH_BLOCKS: u64 = 100;
/// Decay numerator: w_eff = w × (DECAY_NUM / DECAY_DEN) per epoch.
pub const DECAY_NUM: u64 = 99;
/// Decay denominator.
pub const DECAY_DEN: u64 = 100;
/// Minimum axon weight before pruning. Below this the axon is removed.
pub const PRUNE_MIN_WEIGHT: u64 = 1_000;

use hemera::hash as hemera_hash;
use lens::Commitment;
use nebu::Goldilocks;

use crate::dim::{
    bbg_poly_commit, commit_dim, goldilocks_from_bytes32, goldilocks_from_u64,
};
use crate::signal::{InsertError, Signal};
use crate::types::{
    CardRecord, Particle, CoinRecord, FileRecord, LocationRecord, NeuronId, NeuronRecord,
    ParticleRecord, SignalRecord,
};

/// The full BBG state: 10 dimensions + private commitment sets.
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
    /// Reverse map: axon_id → (from, to). Not committed; used for pruning.
    pub axon_edges: BTreeMap<Particle, (Particle, Particle)>,
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
            axon_edges: BTreeMap::new(),
            height: 0,
            root: empty_root,
        }
    }

    /// Compute BBG_root = H(commit(BBG_poly) ‖ commit(A) ‖ commit(N)).
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
        ];
        let bbg_poly_particle = bbg_poly_commit(&dim_commits);
        let a_commit = self.commit_a();
        let n_commit = self.commit_n();

        let mut buf = Vec::with_capacity(96);
        buf.extend_from_slice(&bbg_poly_particle);
        buf.extend_from_slice(a_commit.as_bytes());
        buf.extend_from_slice(n_commit.as_bytes());

        let hash = hemera_hash(&buf);
        let hash_bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        let len = hash_bytes.len().min(32);
        out[..len].copy_from_slice(&hash_bytes[..len]);
        out
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
                vals.push(goldilocks_from_u64(v.link_count as u64));
                vals.push(goldilocks_from_u64(v.block_height));
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.proof_hash));
                (key, vals)
            })
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
    /// sufficiency, UTXO ownership, conservation, VDF). BBG only enforces
    /// the structural double-spend invariant via N(x).
    pub fn insert(&mut self, signal: &Signal) -> Result<(), InsertError> {
        // Structural check: N(nullifier) = 0 → reject
        for mv in &signal.utxo_moves {
            if self.nullifiers.contains(&mv.nullifier) {
                return Err(InsertError::DoubleSpend);
            }
        }

        // Apply UTXO movements
        for mv in &signal.utxo_moves {
            self.nullifiers.insert(mv.nullifier);
            if let Some((point, value)) = mv.commitment {
                self.commitments.insert(point, Goldilocks::new(value));
            }
        }

        // Apply each cyberlink ℓ = (p, q, τ, a, v)
        for link in &signal.links {
            // axon-particle = H(from ‖ to)
            let axon_id = {
                let mut buf = [0u8; 64];
                buf[..32].copy_from_slice(&link.from);
                buf[32..].copy_from_slice(&link.to);
                let h = hemera_hash(&buf);
                let mut out = [0u8; 32];
                let b = h.as_bytes();
                out[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
                out
            };

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
        }

        self.root = self.compute_root();
        Ok(())
    }

    /// Decay axon weights and prune those below PRUNE_MIN_WEIGHT.
    /// Called at epoch boundaries (every EPOCH_BLOCKS blocks).
    pub fn apply_decay_and_prune(&mut self) {
        let mut to_prune: Vec<Particle> = Vec::new();
        for (axon_id, particle) in &mut self.particles {
            if particle.weight > 0 {
                particle.weight = particle.weight.saturating_mul(DECAY_NUM) / DECAY_DEN;
                if particle.weight < PRUNE_MIN_WEIGHT {
                    to_prune.push(*axon_id);
                }
            }
        }
        for axon_id in to_prune {
            self.particles.remove(&axon_id);
            if let Some((from, to)) = self.axon_edges.remove(&axon_id) {
                if let Some(list) = self.axons_out.get_mut(&from) {
                    list.retain(|id| id != &axon_id);
                    if list.is_empty() {
                        self.axons_out.remove(&from);
                    }
                }
                if let Some(list) = self.axons_in.get_mut(&to) {
                    list.retain(|id| id != &axon_id);
                    if list.is_empty() {
                        self.axons_in.remove(&to);
                    }
                }
            }
        }
        self.root = self.compute_root();
    }
}

impl Default for BbgState {
    fn default() -> Self {
        Self::new()
    }
}
