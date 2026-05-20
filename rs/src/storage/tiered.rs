// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Tiered storage: all backends active simultaneously.
//!
//! Routing:
//!   write  → HOT always; write-through to WARM for durability (not EPHEMERAL)
//!   read   → HOT → WARM → COLD → NETWORK (cascade, no promotion on read)
//!   commit → HOT + WARM flushed per block; COLD at archival checkpoints
//!   evict  → called by soma when focus drops; moves HOT entry to WARM
//!
//! Promotion (WARM/COLD → HOT) is explicit, driven by soma prefetch,
//! not lazy on read — keeping get(&self) borrow-checker clean.

use nebu::Goldilocks;

use super::{dim, NetworkStore, ShardStore};
use crate::types::Particle;

pub struct TieredStore {
    /// L1: current polynomial evaluation tables (memory or unimem)
    hot:     Box<dyn ShardStore>,
    /// L2: recent state, durability copy (fjall/ssd)
    warm:    Option<Box<dyn ShardStore>>,
    /// L4: archival history (redb/hdd)
    cold:    Option<Box<dyn ShardStore>>,
    /// L3: content retrieval on miss (injected by cybergraph)
    network: Option<Box<dyn NetworkStore>>,
}

impl TieredStore {
    pub fn new(hot: Box<dyn ShardStore>) -> Self {
        Self { hot, warm: None, cold: None, network: None }
    }

    pub fn with_warm(mut self, warm: Box<dyn ShardStore>) -> Self {
        self.warm = Some(warm);
        self
    }

    pub fn with_cold(mut self, cold: Box<dyn ShardStore>) -> Self {
        self.cold = Some(cold);
        self
    }

    pub fn with_network(mut self, net: Box<dyn NetworkStore>) -> Self {
        self.network = Some(net);
        self
    }

    /// Promote a (dim, key) from WARM or COLD into HOT.
    /// Called by soma prefetch or focus-driven caching.
    pub fn promote(&mut self, dimension: u8, key: &[u8; 32]) -> bool {
        // Try warm first, then cold.
        let found = self.warm.as_ref()
            .and_then(|w| w.get(dimension, key))
            .or_else(|| self.cold.as_ref().and_then(|c| c.get(dimension, key)));

        if let Some(slice) = found {
            let owned = slice.to_vec();
            self.hot.put(dimension, *key, owned);
            true
        } else {
            false
        }
    }

    /// Evict a (dim, key) from HOT, ensuring it is persisted in WARM.
    /// Called by soma when focus drops below eviction threshold.
    pub fn evict(&mut self, dimension: u8, key: &[u8; 32]) {
        if let Some(slice) = self.hot.get(dimension, key) {
            let owned = slice.to_vec();
            if let Some(warm) = &mut self.warm {
                warm.put(dimension, *key, owned);
            }
        }
        self.hot.remove(dimension, key);
    }

    /// Fetch raw content bytes for a particle from the network tier.
    pub fn fetch_content(&self, particle: &Particle) -> Option<Vec<u8>> {
        self.network.as_ref()?.fetch(particle)
    }

    /// Flush COLD tier explicitly (called at archival checkpoints, not per block).
    pub fn archive(&mut self) -> Option<[u8; 32]> {
        self.cold.as_mut().map(|c| c.commit())
    }
}

impl ShardStore for TieredStore {
    /// Cascade: HOT → WARM → COLD. No lazy promotion; use promote() explicitly.
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        if let Some(v) = self.hot.get(dimension, key) {
            return Some(v);
        }
        if let Some(warm) = &self.warm {
            if let Some(v) = warm.get(dimension, key) {
                return Some(v);
            }
        }
        if let Some(cold) = &self.cold {
            if let Some(v) = cold.get(dimension, key) {
                return Some(v);
            }
        }
        None
    }

    /// Write-through: HOT always, WARM for durability. EPHEMERAL stays in HOT only.
    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) {
        if dimension != dim::EPHEMERAL {
            if let Some(warm) = &mut self.warm {
                warm.put(dimension, key, value.clone());
            }
        }
        self.hot.put(dimension, key, value);
    }

    /// Dirty entries are tracked by HOT.
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        self.hot.dirty_entries()
    }

    /// Per-block commit: flush HOT + WARM. COLD is archival-only (see archive()).
    fn commit(&mut self) -> [u8; 32] {
        let sub_root = self.hot.commit();
        if let Some(warm) = &mut self.warm {
            let _ = warm.commit();
        }
        sub_root
    }

    fn get_mut(&mut self, dimension: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        self.hot.get_mut(dimension, key)
    }

    fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]) {
        self.hot.mark_dirty(dimension, key);
    }

    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>> {
        // Materialize from cascade before mutating any tier.
        let val = self.get(dimension, key).map(|s| s.to_vec())?;
        self.hot.remove(dimension, key);
        if let Some(warm) = &mut self.warm {
            warm.remove(dimension, key);
        }
        if let Some(cold) = &mut self.cold {
            cold.remove(dimension, key);
        }
        Some(val)
    }

    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        self.hot.iter(dimension)
    }
}

/// For tests and validators that don't need persistence: memory-only store.
impl Default for TieredStore {
    fn default() -> Self {
        Self::new(Box::new(super::mem::MemStore::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::mem::MemStore;
    use nebu::Goldilocks;

    fn g(v: u64) -> Goldilocks { Goldilocks::new(v) }
    fn key(b: u8) -> [u8; 32] { [b; 32] }

    #[test]
    fn write_through_to_warm() {
        let hot  = Box::new(MemStore::new());
        let warm = Box::new(MemStore::new());
        let warm_ptr = &*warm as *const MemStore as usize;
        let mut store = TieredStore::new(hot).with_warm(warm);

        store.put(0, key(1), vec![g(42)]);

        // Value readable from hot
        assert_eq!(store.hot.get(0, &key(1)), Some([g(42)].as_slice()));
        // Value also in warm (write-through)
        assert_eq!(store.warm.as_ref().unwrap().get(0, &key(1)), Some([g(42)].as_slice()));
        let _ = warm_ptr; // suppress warning
    }

    #[test]
    fn read_cascades_hot_then_warm() {
        let hot  = Box::new(MemStore::new());
        let mut warm = Box::new(MemStore::new());
        warm.put(0, key(2), vec![g(99)]);
        // hot does NOT have key(2)
        let _ = hot.get(0, &key(2));

        let store = TieredStore::new(hot).with_warm(warm);
        assert_eq!(store.get(0, &key(2)), Some([g(99)].as_slice()));
    }

    #[test]
    fn hot_hit_shadows_warm() {
        let mut hot  = Box::new(MemStore::new());
        let mut warm = Box::new(MemStore::new());
        hot.put(0, key(3), vec![g(1)]);
        warm.put(0, key(3), vec![g(2)]);  // different value in warm

        let store = TieredStore::new(hot).with_warm(warm);
        // HOT wins
        assert_eq!(store.get(0, &key(3)), Some([g(1)].as_slice()));
    }

    #[test]
    fn promote_moves_warm_to_hot() {
        let hot  = Box::new(MemStore::new());
        let mut warm = Box::new(MemStore::new());
        warm.put(0, key(4), vec![g(77)]);

        let mut store = TieredStore::new(hot).with_warm(warm);
        assert!(store.hot.get(0, &key(4)).is_none());

        let promoted = store.promote(0, &key(4));
        assert!(promoted);
        assert_eq!(store.hot.get(0, &key(4)), Some([g(77)].as_slice()));
    }

    #[test]
    fn ephemeral_not_written_to_warm() {
        let hot  = Box::new(MemStore::new());
        let warm = Box::new(MemStore::new());
        let mut store = TieredStore::new(hot).with_warm(warm);

        store.put(dim::EPHEMERAL, key(5), vec![g(123)]);

        assert_eq!(store.hot.get(dim::EPHEMERAL, &key(5)), Some([g(123)].as_slice()));
        assert!(store.warm.as_ref().unwrap().get(dim::EPHEMERAL, &key(5)).is_none(),
            "EPHEMERAL must not be written to warm tier");
    }

    #[test]
    fn ephemeral_not_in_dirty_after_commit() {
        let mut store = TieredStore::default();
        store.put(dim::EPHEMERAL, key(6), vec![g(7)]);
        assert!(store.dirty_entries().is_empty(), "EPHEMERAL must not appear in dirty");
    }

    #[test]
    fn remove_clears_from_all_tiers() {
        let hot  = Box::new(MemStore::new());
        let warm = Box::new(MemStore::new());
        let mut store = TieredStore::new(hot).with_warm(warm);

        store.put(0, key(7), vec![g(55)]);
        let removed = store.remove(0, &key(7));

        assert_eq!(removed, Some(vec![g(55)]));
        assert!(store.hot.get(0, &key(7)).is_none());
        assert!(store.warm.as_ref().unwrap().get(0, &key(7)).is_none());
    }

    #[test]
    fn get_mut_and_mark_dirty_roundtrip() {
        let mut store = TieredStore::default();
        store.put(0, key(8), vec![g(10), g(20)]);
        store.commit(); // clear dirty

        {
            let slice = store.get_mut(0, &key(8)).unwrap();
            slice[0] = g(99);
        }
        store.mark_dirty(0, key(8));

        let dirty = store.dirty_entries();
        assert_eq!(dirty.len(), 1);
        assert_eq!(dirty[0].2[0], g(99));
    }

    #[test]
    fn iter_returns_dimension_entries() {
        let mut store = TieredStore::default();
        store.put(0, key(1), vec![g(1)]);
        store.put(0, key(2), vec![g(2)]);
        store.put(1, key(3), vec![g(3)]);

        let dim0: Vec<_> = store.iter(0).collect();
        assert_eq!(dim0.len(), 2);
        let dim1: Vec<_> = store.iter(1).collect();
        assert_eq!(dim1.len(), 1);
    }
}
