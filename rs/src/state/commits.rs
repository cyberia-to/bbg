// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Per-dimension record layouts for [`BbgState`].
//!
//! v2 (keyed trie): the leaf of a record is `trie::leaf_digest(key, leaf_vals)`
//! with [`LeafValue`] giving the fields in the injective encoding (`u64` → two
//! 32-bit limbs, 32 bytes → eight). v1 (flat Brakedown polynomial): the sorted
//! concatenation of `[key(4) | fields]` in the historical one-limb-per-`u64`
//! layout, committed via [`commit_dim`] and cached per dimension until it
//! mutates. Adjacency lists: v1 commits the ordered list, v2 the set (see
//! [`crate::keyed::AxonIndex`]).

use lens::Commitment;
use nebu::Goldilocks;

use crate::dim::{bytes32_limbs, commit_dim, goldilocks_from_bytes32, goldilocks_from_u64, u64_limbs};
use crate::keyed::LeafValue;
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
        [self.focus, self.karma, self.stake].iter().flat_map(|&v| u64_limbs(v)).collect()
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
        vals
    }
}

impl BbgState {
    pub(super) fn commit_particles(&self) -> Commitment {
        self.particles.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
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
        })
    }

    pub(super) fn commit_axons_out(&self) -> Commitment {
        self.axons_out.legacy_commitment(|m| {
        // Serialize each adjacency list: key + count + each particle (4 field elems each)
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(k, v)| {
                let mut vals = vec![goldilocks_from_u64(v.len() as u64)];
                for particle in v {
                    vals.extend_from_slice(&goldilocks_from_bytes32(particle));
                }
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_axons_in(&self) -> Commitment {
        self.axons_in.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(k, v)| {
                let mut vals = vec![goldilocks_from_u64(v.len() as u64)];
                for particle in v {
                    vals.extend_from_slice(&goldilocks_from_bytes32(particle));
                }
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_neurons(&self) -> Commitment {
        self.neurons.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
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
        })
    }

    pub(super) fn commit_locations(&self) -> Commitment {
        self.locations.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
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
        })
    }

    pub(super) fn commit_coins(&self) -> Commitment {
        self.coins.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(k, v)| ((*k), vec![goldilocks_from_u64(v.total_supply)]))
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_cards(&self) -> Commitment {
        self.cards.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(k, v)| {
                let mut vals: Vec<Goldilocks> = goldilocks_from_bytes32(&v.owner).to_vec();
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.particle));
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_files(&self) -> Commitment {
        self.files.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(k, v)| {
                let vals = vec![
                    goldilocks_from_u64(v.available as u64),
                    goldilocks_from_u64(v.chunk_count as u64),
                ];
                (*k, vals)
            })
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_time(&self) -> Commitment {
        self.time.legacy_commitment(|m| {
        // Key: height as 32-byte key (8 bytes LE padded)
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(h, particle)| {
                let mut key = [0u8; 32];
                key[..8].copy_from_slice(&h.to_le_bytes());
                let vals: Vec<Goldilocks> = goldilocks_from_bytes32(particle).to_vec();
                (key, vals)
            })
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_signals(&self) -> Commitment {
        self.signals.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
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
        })
    }

    pub(super) fn commit_balances(&self) -> Commitment {
        self.balances.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(k, v)| (*k, vec![goldilocks_from_u64(*v)]))
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_a(&self) -> Commitment {
        self.commitments.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.iter()
            .map(|(k, v)| (*k, vec![*v]))
            .collect();
        commit_dim(&entries)
        })
    }

    pub(super) fn commit_n(&self) -> Commitment {
        self.nullifiers.legacy_commitment(|m| {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = m.keys()
            .map(|k| (*k, vec![goldilocks_from_u64(1)]))
            .collect();
        commit_dim(&entries)
        })
    }

}
