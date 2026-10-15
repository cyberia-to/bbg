// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Per-dimension record layouts for [`BbgState`].
//!
//! One layout per dimension, used by both root versions: the v2 leaf of a
//! record is `trie::leaf_digest(key, leaf_vals)` ([`LeafValue`]), the v1
//! polynomial is the sorted concatenation of `[key(4) | leaf_vals]` entries
//! committed via [`commit_dim`] and cached per dimension until it mutates.
//! Adjacency lists are the one exception: v1 serializes the ordered list,
//! v2 commits the set (see [`crate::keyed::AxonIndex`]).

use std::collections::BTreeMap;

use lens::Commitment;
use nebu::Goldilocks;

use crate::dim::{commit_dim, goldilocks_from_bytes32, goldilocks_from_u64};
use crate::keyed::{LeafValue, TrieKey};
use crate::types::{
    CardRecord, CoinRecord, FileRecord, LocationRecord, Particle, ParticleRecord, NeuronRecord,
    SignalRecord,
};

use super::BbgState;

impl LeafValue for ParticleRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        vec![
            goldilocks_from_u64(self.energy),
            goldilocks_from_u64(self.pi_star),
            goldilocks_from_u64(self.weight),
            goldilocks_from_u64(self.s_yes),
            goldilocks_from_u64(self.s_no),
            goldilocks_from_u64(self.meta_score),
        ]
    }
}

impl LeafValue for NeuronRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        vec![
            goldilocks_from_u64(self.focus),
            goldilocks_from_u64(self.karma),
            goldilocks_from_u64(self.stake),
        ]
    }
}

impl LeafValue for LocationRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        // i32 stored as u64 via bit-cast
        vec![
            goldilocks_from_u64(self.lat as u32 as u64),
            goldilocks_from_u64(self.lon as u32 as u64),
        ]
    }
}

impl LeafValue for CoinRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        vec![goldilocks_from_u64(self.total_supply)]
    }
}

impl LeafValue for CardRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        let mut vals = goldilocks_from_bytes32(&self.owner).to_vec();
        vals.extend_from_slice(&goldilocks_from_bytes32(&self.particle));
        vals
    }
}

impl LeafValue for FileRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        vec![
            goldilocks_from_u64(self.available as u64),
            goldilocks_from_u64(self.chunk_count as u64),
        ]
    }
}

impl LeafValue for SignalRecord {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        let mut vals = goldilocks_from_bytes32(&self.neuron).to_vec();
        vals.extend_from_slice(&goldilocks_from_bytes32(&self.network));
        vals.push(goldilocks_from_u64(self.link_count as u64));
        vals.push(goldilocks_from_u64(self.block_height));
        vals.extend_from_slice(&goldilocks_from_bytes32(&self.proof_hash));
        vals
    }
}

/// v1: the sorted `[key(4) | leaf_vals]` polynomial of a keyed map.
fn commit_map<K: TrieKey, V: LeafValue>(map: &BTreeMap<K, V>) -> Commitment {
    let entries: Vec<(Particle, Vec<Goldilocks>)> =
        map.iter().map(|(k, v)| (k.key_bytes(), v.leaf_vals())).collect();
    commit_dim(&entries)
}

/// v1: adjacency lists as `[count, particle…]` in list order.
fn commit_lists(map: &BTreeMap<Particle, Vec<Particle>>) -> Commitment {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = map
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

impl BbgState {
    pub(super) fn commit_particles(&self) -> Commitment {
        self.particles.legacy_commitment(commit_map)
    }

    pub(super) fn commit_axons_out(&self) -> Commitment {
        self.axons_out.legacy_commitment(commit_lists)
    }

    pub(super) fn commit_axons_in(&self) -> Commitment {
        self.axons_in.legacy_commitment(commit_lists)
    }

    pub(super) fn commit_neurons(&self) -> Commitment {
        self.neurons.legacy_commitment(commit_map)
    }

    pub(super) fn commit_locations(&self) -> Commitment {
        self.locations.legacy_commitment(commit_map)
    }

    pub(super) fn commit_coins(&self) -> Commitment {
        self.coins.legacy_commitment(commit_map)
    }

    pub(super) fn commit_cards(&self) -> Commitment {
        self.cards.legacy_commitment(commit_map)
    }

    pub(super) fn commit_files(&self) -> Commitment {
        self.files.legacy_commitment(commit_map)
    }

    pub(super) fn commit_time(&self) -> Commitment {
        self.time.legacy_commitment(commit_map)
    }

    pub(super) fn commit_signals(&self) -> Commitment {
        self.signals.legacy_commitment(commit_map)
    }

    pub(super) fn commit_balances(&self) -> Commitment {
        self.balances.legacy_commitment(commit_map)
    }

    pub(super) fn commit_a(&self) -> Commitment {
        self.commitments.legacy_commitment(commit_map)
    }

    pub(super) fn commit_n(&self) -> Commitment {
        // N(x): every nullifier carries the value 1.
        self.nullifiers.legacy_commitment(|set| {
            let entries: Vec<(Particle, Vec<Goldilocks>)> =
                set.keys().map(|k| (*k, vec![goldilocks_from_u64(1)])).collect();
            commit_dim(&entries)
        })
    }
}
