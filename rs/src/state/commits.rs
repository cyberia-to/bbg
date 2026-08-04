// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Per-dimension polynomial commitment serializers for [`BbgState`].
//!
//! Each method serializes one dimension into `(key, values)` entries and
//! commits it via [`commit_dim`]. Folded into `BBG_root` by
//! [`BbgState::compute_root`].

use lens::Commitment;
use nebu::Goldilocks;

use crate::dim::{commit_dim, goldilocks_from_bytes32, goldilocks_from_u64};
use crate::types::Particle;

use super::BbgState;

impl BbgState {
    pub(super) fn commit_particles(&self) -> Commitment {
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

    pub(super) fn commit_axons_out(&self) -> Commitment {
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

    pub(super) fn commit_axons_in(&self) -> Commitment {
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

    pub(super) fn commit_neurons(&self) -> Commitment {
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

    pub(super) fn commit_locations(&self) -> Commitment {
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

    pub(super) fn commit_coins(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .coins
            .iter()
            .map(|(k, v)| ((*k), vec![goldilocks_from_u64(v.total_supply)]))
            .collect();
        commit_dim(&entries)
    }

    pub(super) fn commit_cards(&self) -> Commitment {
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

    pub(super) fn commit_files(&self) -> Commitment {
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

    pub(super) fn commit_time(&self) -> Commitment {
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

    pub(super) fn commit_signals(&self) -> Commitment {
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

    pub(super) fn commit_balances(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .balances
            .iter()
            .map(|(k, v)| (*k, vec![goldilocks_from_u64(*v)]))
            .collect();
        commit_dim(&entries)
    }

    pub(super) fn commit_a(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .commitments
            .iter()
            .map(|(k, v)| (*k, vec![*v]))
            .collect();
        commit_dim(&entries)
    }

    pub(super) fn commit_n(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .nullifiers
            .iter()
            .map(|k| (*k, vec![goldilocks_from_u64(1)]))
            .collect();
        commit_dim(&entries)
    }
}
