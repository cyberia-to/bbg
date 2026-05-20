// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Pruning policy for BBG state.
//!
//! Three mechanisms in priority order:
//!   1. rank floor  — protect entries above φ* percentile threshold
//!   2. half-life   — candidacy for edges not refreshed within the window
//!   3. gravity     — density-ordered removal under size pressure
//!
//! Policy state (PruneConfig, PruneState) is separate from BBG_poly records.
//! No decay parameters or timestamps are stored in NeuronRecord or ParticleRecord.

use std::collections::BTreeMap;

use crate::state::BbgState;
use crate::types::Particle;

/// Pruning configuration. Three parameters, no hardcoded constants.
pub struct PruneConfig {
    /// Maximum estimated BBG state size in bytes before gravity kicks in.
    pub max_bytes: u64,
    /// Top-N percentile of φ* that is protected from pruning (0–100).
    /// e.g. 20 = top 20% of particles by pi_star are never pruned.
    pub rank_floor_pct: u8,
    /// Number of epochs without a signal before an edge is stale.
    pub half_life_epochs: u64,
}

impl Default for PruneConfig {
    fn default() -> Self {
        Self {
            max_bytes: 1 << 30,      // 1 GiB
            rank_floor_pct: 20,
            half_life_epochs: 1_000,
        }
    }
}

/// Runtime pruning state. Lives outside BBG_poly — not part of the state root.
pub struct PruneState {
    /// Epoch of the last signal that touched each axon.
    pub last_touched: BTreeMap<Particle, u64>,
}

impl Default for PruneState {
    fn default() -> Self {
        Self { last_touched: BTreeMap::new() }
    }
}

impl PruneState {
    /// Record that `axon_id` was touched at `epoch`.
    pub fn touch(&mut self, axon_id: Particle, epoch: u64) {
        self.last_touched.insert(axon_id, epoch);
    }
}

/// Run one pruning cycle at the end of `epoch`.
///
/// Called by `Bbg::finalize_block` at every epoch boundary, after the time
/// snapshot is written. Mutates `state` and `ps`; recomputes the root after.
pub fn prune(state: &mut BbgState, ps: &mut PruneState, config: &PruneConfig, epoch: u64) {
    let floor = rank_floor(state, config.rank_floor_pct);
    let over_budget = estimate_bytes(state) > config.max_bytes;

    // Collect candidate axon ids and their weights (proxy for density).
    let candidates: Vec<(Particle, u64)> = if over_budget {
        // Gravity: all unprotected entries compete.
        state
            .particles
            .iter()
            .filter(|(_, p)| p.pi_star < floor)
            .map(|(id, p)| (*id, p.weight))
            .collect()
    } else {
        // Half-life: only stale unprotected entries.
        ps.last_touched
            .iter()
            .filter(|(id, last)| {
                let pi = state.particles.get(*id).map_or(0, |p| p.pi_star);
                let stale = epoch.saturating_sub(**last) > config.half_life_epochs;
                pi < floor && stale
            })
            .map(|(id, _)| {
                let weight = state.particles.get(id).map_or(0, |p| p.weight);
                (*id, weight)
            })
            .collect()
    };

    // Sort ascending by weight (lowest density = lowest weight = prune first).
    let mut sorted = candidates;
    sorted.sort_by_key(|(_, w)| *w);

    for (axon_id, _) in sorted {
        if over_budget && estimate_bytes(state) <= config.max_bytes {
            break;
        }
        remove_axon(state, ps, &axon_id);
    }

    if !state.particles.is_empty() {
        state.root = state.compute_root();
    }
}

// ── internals ─────────────────────────────────────────────────────────────────

/// The φ* threshold below which entries are eligible for pruning.
///
/// pct=20 → protect top 20%: floor = value at the 80th percentile.
///          Entries with pi_star >= floor are protected.
/// pct=0  → protect nothing: floor = u64::MAX (pi_star < MAX covers everything).
/// pct=100 → protect everything: floor = 0 (pi_star >= 0 always true).
fn rank_floor(state: &BbgState, pct: u8) -> u64 {
    if pct == 0 {
        return u64::MAX;
    }
    let mut scores: Vec<u64> = state.particles.values().map(|p| p.pi_star).collect();
    if scores.is_empty() {
        return 0;
    }
    scores.sort_unstable();
    // boundary = first index that belongs to the protected top-pct%.
    let protected_count = (scores.len() * pct as usize + 99) / 100; // ceiling
    let boundary = scores.len().saturating_sub(protected_count);
    scores[boundary]
}

/// Rough storage estimate in bytes for the graph dimensions of BBG state.
pub fn estimate_bytes(state: &BbgState) -> u64 {
    let particles = state.particles.len() as u64 * 80;
    let axons_out: u64 = state.axons_out.values().map(|v| 32 + v.len() as u64 * 32).sum();
    let axons_in: u64 = state.axons_in.values().map(|v| 32 + v.len() as u64 * 32).sum();
    let neurons = state.neurons.len() as u64 * 56;
    particles + axons_out + axons_in + neurons
}

/// Remove an axon and all its index references. Updates PruneState too.
fn remove_axon(state: &mut BbgState, ps: &mut PruneState, axon_id: &Particle) {
    state.particles.remove(axon_id);
    ps.last_touched.remove(axon_id);
    if let Some((from, to)) = state.axon_edges.remove(axon_id) {
        if let Some(list) = state.axons_out.get_mut(&from) {
            list.retain(|id| id != axon_id);
            if list.is_empty() {
                state.axons_out.remove(&from);
            }
        }
        if let Some(list) = state.axons_in.get_mut(&to) {
            list.retain(|id| id != axon_id);
            if list.is_empty() {
                state.axons_in.remove(&to);
            }
        }
    }
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{Cyberlink, Signal};
    use crate::state::EPOCH_BLOCKS;
    use crate::types::NeuronRecord;

    fn particle(seed: u8) -> Particle { [seed; 32] }
    fn neuron_id(seed: u8) -> Particle { [seed; 32] }

    fn make_state() -> BbgState {
        let mut s = BbgState::new();
        s.neurons.insert(neuron_id(1), NeuronRecord { focus: 10_000, karma: 0, stake: 0 });
        s
    }

    fn insert_link(state: &mut BbgState, ps: &mut PruneState, from: Particle, to: Particle, amount: u64, epoch: u64) {
        let sig = Signal {
            neuron: neuron_id(1),
            links: vec![Cyberlink { from, to, token: particle(0), amount, valence: 1 }],
            box_moves: vec![],
            height: epoch * EPOCH_BLOCKS,
        };
        state.insert(&sig).unwrap();
        let aid = crate::state::axon_id(&from, &to);
        ps.touch(aid, epoch);
    }

    #[test]
    fn stale_low_weight_edge_is_pruned() {
        let mut state = make_state();
        let mut ps = PruneState::default();
        let config = PruneConfig { half_life_epochs: 10, rank_floor_pct: 0, max_bytes: u64::MAX };

        insert_link(&mut state, &mut ps, particle(1), particle(2), 100, 0);
        assert!(state.particles.contains_key(&crate::state::axon_id(&particle(1), &particle(2))));

        // advance past half-life, no refresh
        prune(&mut state, &mut ps, &config, 11);

        assert!(!state.particles.contains_key(&crate::state::axon_id(&particle(1), &particle(2))));
    }

    #[test]
    fn fresh_edge_survives_normal_pruning() {
        let mut state = make_state();
        let mut ps = PruneState::default();
        let config = PruneConfig { half_life_epochs: 100, rank_floor_pct: 0, max_bytes: u64::MAX };

        insert_link(&mut state, &mut ps, particle(1), particle(2), 100, 5);

        // epoch 10 — still within half-life
        prune(&mut state, &mut ps, &config, 10);

        assert!(state.particles.contains_key(&crate::state::axon_id(&particle(1), &particle(2))));
    }

    #[test]
    fn high_rank_edge_is_protected() {
        let mut state = make_state();
        let mut ps = PruneState::default();
        // half-life short, rank floor protects top 50%
        let config = PruneConfig { half_life_epochs: 1, rank_floor_pct: 50, max_bytes: u64::MAX };

        insert_link(&mut state, &mut ps, particle(1), particle(2), 100, 0);

        // manually set pi_star high so it falls in the protected percentile
        let aid = crate::state::axon_id(&particle(1), &particle(2));
        state.particles.get_mut(&aid).unwrap().pi_star = 9_999;

        prune(&mut state, &mut ps, &config, 100);

        assert!(state.particles.contains_key(&aid));
    }

    #[test]
    fn gravity_prunes_fresh_low_density_when_over_budget() {
        let mut state = make_state();
        let mut ps = PruneState::default();
        // budget so tight that even fresh edges must go
        let config = PruneConfig { half_life_epochs: 10_000, rank_floor_pct: 0, max_bytes: 0 };

        insert_link(&mut state, &mut ps, particle(1), particle(2), 50, 0);
        insert_link(&mut state, &mut ps, particle(3), particle(4), 200, 0);

        // both fresh — gravity should prune lower-weight first
        prune(&mut state, &mut ps, &config, 1);

        // low-weight (50) pruned, high-weight (200) survives (budget is 0 so both may go,
        // but the order is low-first)
        let low_aid = crate::state::axon_id(&particle(1), &particle(2));
        let hi_aid  = crate::state::axon_id(&particle(3), &particle(4));
        // low-weight must be gone before high-weight is considered
        if state.particles.contains_key(&hi_aid) {
            assert!(!state.particles.contains_key(&low_aid));
        }
    }

    #[test]
    fn refresh_resets_staleness() {
        let mut state = make_state();
        let mut ps = PruneState::default();
        let config = PruneConfig { half_life_epochs: 10, rank_floor_pct: 0, max_bytes: u64::MAX };

        insert_link(&mut state, &mut ps, particle(1), particle(2), 100, 0);

        // refresh at epoch 8 — resets last_touched
        let aid = crate::state::axon_id(&particle(1), &particle(2));
        ps.touch(aid, 8);

        // epoch 15 — only 7 epochs since refresh, within half-life
        prune(&mut state, &mut ps, &config, 15);

        assert!(state.particles.contains_key(&aid));
    }
}
