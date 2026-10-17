// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Per-dimension record layouts for [`BbgState`].
//!
//! v2 (keyed trie): the leaf of a record is `trie::leaf_digest(key, leaf_vals)`
//! with [`LeafValue`] giving the fields in the injective encoding (`u64` → two
//! 32-bit limbs, 32 bytes → eight) — the same fields `proof::dim_entries`
//! serializes for v1. v1 (flat Brakedown polynomial, table layout `dim::TABLE_VERSION`):
//! `commit_dim` over `dim_entries`, cached per dimension until it mutates.
//! Adjacency: v1 commits the ordered list, v2 the set ([`crate::keyed::AxonIndex`]).

use lens::Commitment;
use nebu::Goldilocks;

use crate::dim::{bytes32_limbs, commit_dim, u64_limbs};
use crate::keyed::LeafValue;
use crate::proof::dim_entries;
use crate::query::Dim;
use crate::types::{
    CardRecord, CoinRecord, FileRecord, LocationRecord, Particle, ParticleRecord, NeuronRecord,
    SignalRecord,
};

use super::BbgState;

impl LeafValue for ParticleRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        [self.energy, self.pi_star, self.weight, self.s_yes, self.s_no, self.meta_score]
            .iter()
            .flat_map(|&v| u64_limbs(v))
            .collect()
    }
}

impl LeafValue for NeuronRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        [self.focus, self.karma, self.stake, self.nonce].iter().flat_map(|&v| u64_limbs(v)).collect()
    }
}

impl LeafValue for LocationRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        // i32 stored as u64 via bit-cast
        [self.lat as u32 as u64, self.lon as u32 as u64].iter().flat_map(|&v| u64_limbs(v)).collect()
    }
}

impl LeafValue for CoinRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        u64_limbs(self.total_supply).to_vec()
    }
}

impl LeafValue for CardRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        let mut vals = bytes32_limbs(&self.owner).to_vec();
        vals.extend_from_slice(&bytes32_limbs(&self.particle));
        vals
    }
}

impl LeafValue for FileRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        [self.available as u64, self.chunk_count as u64].iter().flat_map(|&v| u64_limbs(v)).collect()
    }
}

impl LeafValue for SignalRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        let mut vals = bytes32_limbs(&self.neuron).to_vec();
        vals.extend_from_slice(&bytes32_limbs(&self.network));
        vals.extend_from_slice(&u64_limbs(self.link_count as u64));
        vals.extend_from_slice(&u64_limbs(self.block_height));
        vals.extend_from_slice(&bytes32_limbs(&self.proof_hash));
        vals.extend_from_slice(&u64_limbs(self.step));
        vals
    }
}

impl BbgState {
    pub(super) fn commit_particles(&self) -> Commitment {
        self.particles.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Particles)))
    }

    pub(super) fn commit_axons_out(&self) -> Commitment {
        self.axons_out.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::AxonsOut)))
    }

    pub(super) fn commit_axons_in(&self) -> Commitment {
        self.axons_in.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::AxonsIn)))
    }

    pub(super) fn commit_neurons(&self) -> Commitment {
        self.neurons.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Neurons)))
    }

    pub(super) fn commit_locations(&self) -> Commitment {
        self.locations.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Locations)))
    }

    pub(super) fn commit_coins(&self) -> Commitment {
        self.coins.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Coins)))
    }

    pub(super) fn commit_cards(&self) -> Commitment {
        self.cards.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Cards)))
    }

    pub(super) fn commit_files(&self) -> Commitment {
        self.files.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Files)))
    }

    pub(super) fn commit_time(&self) -> Commitment {
        self.time.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Time)))
    }

    pub(super) fn commit_signals(&self) -> Commitment {
        self.signals.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Signals)))
    }

    pub(super) fn commit_balances(&self) -> Commitment {
        self.balances.legacy_commitment(|_| commit_dim(&dim_entries(self, Dim::Balances)))
    }

    pub(super) fn commit_a(&self) -> Commitment {
        self.commitments.legacy_commitment(|m| {
            let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter().map(|(k, v)| (*k, vec![*v])).collect();
            commit_dim(&entries)
        })
    }

    pub(super) fn commit_n(&self) -> Commitment {
        self.nullifiers.legacy_commitment(|m| {
            let entries: Vec<(Particle, Vec<Goldilocks>)> =
                m.keys().map(|k| (*k, vec![Goldilocks::ONE])).collect();
            commit_dim(&entries)
        })
    }
}
