// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Committed graph statistics — the bbg → inf cost/recursion interface.
//!
//! `GraphStats` is folded into `BBG_root` (see `state::compute_root`). This is
//! the cross-repo contract with [[inf]]: inf's cost model and recursion bounds
//! rest on values that are proven by the root, not trusted inputs.
//!
//!   inf cost = reads (bbg read_cost) + combine (inf coefficients)
//!            + recursion (bound × per-iter)
//!
//! The recursion `bound` consumes `diameter_bound`; the read and combine terms
//! consume `relation_sizes` / `node_count` / `max_degree`. The same committed
//! stats that *bound* recursion also *cost* it — one interface serves both.
//!
//! All four statistics are deterministic functions of committed state:
//!   - node_count, relation_sizes, max_degree are computed exactly from the
//!     BTreeMaps at commit time (no estimation).
//!   - diameter_bound is a sound upper bound: either the trivial connected-graph
//!     worst case (node_count − 1) or a tighter bound installed by tru via
//!     `set_diameter_bound`. Either way the committed value is an upper bound,
//!     so any inf recursion bounded by it terminates correctly.

use hemera::hash as hemera_hash;

use crate::types::Particle;

/// Number of committed relations whose sizes are tracked, in the canonical
/// order used by `BbgState::compute_root`.
pub const STAT_RELATIONS: usize = 11;

/// Canonical index of each relation within `GraphStats::relation_sizes`.
/// Matches the dim order folded into `compute_root`, with balances last.
pub mod rel {
    pub const PARTICLES: usize = 0;
    pub const AXONS_OUT: usize = 1;
    pub const AXONS_IN:  usize = 2;
    pub const NEURONS:   usize = 3;
    pub const LOCATIONS: usize = 4;
    pub const COINS:     usize = 5;
    pub const CARDS:     usize = 6;
    pub const FILES:     usize = 7;
    pub const TIME:      usize = 8;
    pub const SIGNALS:   usize = 9;
    pub const BALANCES:  usize = 10;
}

/// Committed graph statistics. Read by inf via `Bbg::statistics()`; authenticated
/// by inclusion in `BBG_root`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphStats {
    /// Number of particles (graph nodes).
    pub node_count: u64,
    /// Per-relation entry counts, indexed by `rel::*`.
    pub relation_sizes: [u64; STAT_RELATIONS],
    /// Maximum out/in adjacency-list length across all particles.
    pub max_degree: u64,
    /// Sound upper bound on graph diameter — bounds inf recursion depth.
    /// Always ≥ the true diameter, so recursion bounded by it is complete.
    pub diameter_bound: u64,
}

impl GraphStats {
    /// Canonical little-endian serialization: node_count ‖ relation_sizes ‖
    /// max_degree ‖ diameter_bound. Single valid encoding per value.
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 * (1 + STAT_RELATIONS + 2));
        buf.extend_from_slice(&self.node_count.to_le_bytes());
        for s in &self.relation_sizes {
            buf.extend_from_slice(&s.to_le_bytes());
        }
        buf.extend_from_slice(&self.max_degree.to_le_bytes());
        buf.extend_from_slice(&self.diameter_bound.to_le_bytes());
        buf
    }

    /// Commitment to these statistics: hemera hash of the canonical serialization.
    /// This 32-byte value is folded into `BBG_root`.
    pub fn commit(&self) -> Particle {
        let h = hemera_hash(&self.serialize());
        let b = h.as_bytes();
        let mut out = [0u8; 32];
        let len = b.len().min(32);
        out[..len].copy_from_slice(&b[..len]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(node_count: u64, diameter_bound: u64) -> GraphStats {
        GraphStats {
            node_count,
            relation_sizes: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            max_degree: 4,
            diameter_bound,
        }
    }

    #[test]
    fn serialize_length_is_fixed() {
        // 1 (node_count) + 11 (relations) + 1 (max_degree) + 1 (diameter) = 14 u64s
        assert_eq!(stats(100, 99).serialize().len(), 14 * 8);
    }

    #[test]
    fn commit_is_deterministic() {
        assert_eq!(stats(100, 99).commit(), stats(100, 99).commit());
    }

    #[test]
    fn different_node_count_changes_commitment() {
        assert_ne!(stats(100, 99).commit(), stats(101, 99).commit());
    }

    #[test]
    fn different_diameter_changes_commitment() {
        assert_ne!(stats(100, 99).commit(), stats(100, 50).commit());
    }

    #[test]
    fn relation_index_constants_cover_all_slots() {
        // Highest index must be exactly STAT_RELATIONS - 1.
        assert_eq!(rel::BALANCES, STAT_RELATIONS - 1);
    }
}
