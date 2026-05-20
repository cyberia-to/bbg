// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Apple Silicon zero-copy shard store backed by honeycrisp::unimem.
//!
//! Each value lives in its own IOSurface-pinned Block by default. When a
//! dimension pool is reserved via `reserve_pool()`, subsequent puts into that
//! dimension allocate cells from one contiguous Block instead of per-entry
//! Blocks. This gives stable GPU/ANE descriptor addresses at frame rate.
//!
//! Fallback: if Block::open fails (e.g., non-Apple-Silicon CI), the entry
//! is stored in a heap Vec and get() returns normally.

use std::collections::HashMap;

use nebu::Goldilocks;
use unimem::{Block, MemError};

use super::{dim, hash_dirty, ShardStore};

// Verify layout assumptions at compile time.
// Goldilocks is repr(transparent) over u64 — size 8, align 8.
const _: () = {
    assert!(core::mem::size_of::<Goldilocks>() == 8);
    assert!(core::mem::align_of::<Goldilocks>() == 8);
};

enum Entry {
    /// IOSurface-pinned Block. Zero-copy reads by all compute units.
    Pinned { block: Block, count: usize },
    /// Heap fallback when IOSurface allocation fails.
    Heap(Vec<Goldilocks>),
}

/// Contiguous IOSurface pool for a single dimension.
/// All entries share one Block; each slot is `bytes_per_entry` bytes.
struct Pool {
    block:           Block,
    bytes_per_entry: usize,
    capacity:        usize,
    next_slot:       usize,
    /// key → (slot_index, element_count)
    slots:           HashMap<[u8; 32], (usize, usize)>,
    free:            Vec<usize>,
}

impl Pool {
    fn alloc_slot(&mut self, key: [u8; 32]) -> usize {
        if let Some(&(idx, _)) = self.slots.get(&key) {
            return idx;  // reuse existing slot (overwrite)
        }
        if let Some(idx) = self.free.pop() {
            return idx;
        }
        let idx = self.next_slot;
        self.next_slot += 1;
        idx
    }
}

pub struct UnimemStore {
    entries: HashMap<(u8, [u8; 32]), Entry>,
    pools:   HashMap<u8, Pool>,
    dirty:   Vec<(u8, [u8; 32], Vec<Goldilocks>)>,
}

impl UnimemStore {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), pools: HashMap::new(), dirty: Vec::new() }
    }
}

impl Default for UnimemStore {
    fn default() -> Self { Self::new() }
}

impl ShardStore for UnimemStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        // Pool path.
        if let Some(pool) = self.pools.get(&dimension) {
            if let Some(&(slot_idx, count)) = pool.slots.get(key) {
                let offset = slot_idx * pool.bytes_per_entry;
                return Some(unsafe {
                    core::slice::from_raw_parts(
                        pool.block.as_bytes()[offset..].as_ptr() as *const Goldilocks,
                        count,
                    )
                });
            }
        }
        // Per-entry path.
        match self.entries.get(&(dimension, *key))? {
            Entry::Heap(v) => Some(v.as_slice()),
            Entry::Pinned { block, count } => {
                // SAFETY:
                // - Goldilocks is repr(transparent) over u64 (verified above).
                // - We wrote exactly `count` Goldilocks values as LE u64 bytes.
                // - Block memory is IOSurface-pinned with a stable address for
                //   the Block's lifetime. Block is owned by self.entries, so
                //   the pointer is valid for the lifetime of &self.
                // - Apple Silicon is little-endian, matching LE serialisation.
                Some(unsafe {
                    core::slice::from_raw_parts(
                        block.as_bytes().as_ptr() as *const Goldilocks,
                        *count,
                    )
                })
            }
        }
    }

    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) {
        if dimension != dim::EPHEMERAL {
            self.dirty.push((dimension, key, value.clone()));
        }

        // Pool path: route to pool cell instead of per-entry Block.
        if let Some(pool) = self.pools.get_mut(&dimension) {
            let slot_idx = pool.alloc_slot(key);
            let count    = value.len().min(pool.bytes_per_entry / 8);
            let offset   = slot_idx * pool.bytes_per_entry;
            let dst = &mut pool.block.as_bytes_mut()[offset..offset + pool.bytes_per_entry];
            dst.fill(0);
            for (i, g) in value.iter().take(count).enumerate() {
                dst[i * 8..(i + 1) * 8].copy_from_slice(&g.as_u64().to_le_bytes());
            }
            pool.slots.insert(key, (slot_idx, value.len()));
            return;
        }

        // Per-entry Block path.
        let count    = value.len();
        let byte_len = (count * 8).max(8);
        let entry = match Block::open(byte_len) {
            Ok(block) => {
                let dst = block.as_bytes_mut();
                for (i, g) in value.iter().enumerate() {
                    dst[i * 8..(i + 1) * 8].copy_from_slice(&g.as_u64().to_le_bytes());
                }
                Entry::Pinned { block, count }
            }
            Err(_) => Entry::Heap(value.clone()),
        };
        self.entries.insert((dimension, key), entry);
    }

    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &self.dirty
    }

    fn commit(&mut self) -> [u8; 32] {
        let out = hash_dirty(&self.dirty);
        self.dirty.clear();
        out
    }

    fn get_mut(&mut self, dimension: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        // Pool path.
        if let Some(pool) = self.pools.get_mut(&dimension) {
            if let Some(&(slot_idx, count)) = pool.slots.get(key) {
                let offset = slot_idx * pool.bytes_per_entry;
                // SAFETY: same as get(), but mutable. Block address is stable.
                return Some(unsafe {
                    core::slice::from_raw_parts_mut(
                        pool.block.as_bytes_mut()[offset..].as_mut_ptr() as *mut Goldilocks,
                        count,
                    )
                });
            }
        }
        // Per-entry path.
        match self.entries.get_mut(&(dimension, *key))? {
            Entry::Heap(v) => Some(v.as_mut_slice()),
            Entry::Pinned { block, count } => {
                let count = *count;
                // SAFETY: same as get(), but mutable.
                Some(unsafe {
                    core::slice::from_raw_parts_mut(
                        block.as_bytes_mut().as_mut_ptr() as *mut Goldilocks,
                        count,
                    )
                })
            }
        }
    }

    fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]) {
        if dimension == dim::EPHEMERAL { return; }
        // Materialize the current value to release the shared borrow before
        // pushing to dirty.
        let val = self.get(dimension, &key).map(|s| s.to_vec());
        if let Some(v) = val {
            self.dirty.push((dimension, key, v));
        }
    }

    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>> {
        // Materialize value before removing.
        let val = self.get(dimension, key).map(|s| s.to_vec())?;

        if let Some(pool) = self.pools.get_mut(&dimension) {
            if let Some((slot_idx, _)) = pool.slots.remove(key) {
                pool.free.push(slot_idx);
            }
        } else {
            self.entries.remove(&(dimension, *key));
        }

        self.dirty.retain(|(d, k, _)| !(*d == dimension && k == key));
        Some(val)
    }

    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        let mut items: Vec<(&[u8; 32], &[Goldilocks])> = Vec::new();

        if let Some(pool) = self.pools.get(&dimension) {
            for (key, &(slot_idx, count)) in &pool.slots {
                let offset = slot_idx * pool.bytes_per_entry;
                let slice = unsafe {
                    core::slice::from_raw_parts(
                        pool.block.as_bytes()[offset..].as_ptr() as *const Goldilocks,
                        count,
                    )
                };
                items.push((key, slice));
            }
        }

        for (k, e) in &self.entries {
            if k.0 == dimension {
                let slice = match e {
                    Entry::Heap(v) => v.as_slice(),
                    Entry::Pinned { block, count } => unsafe {
                        core::slice::from_raw_parts(
                            block.as_bytes().as_ptr() as *const Goldilocks,
                            *count,
                        )
                    },
                };
                items.push((&k.1, slice));
            }
        }

        Box::new(items.into_iter())
    }
}

impl UnimemStore {
    /// Returns the raw Block for a per-entry key if it is IOSurface-pinned.
    /// Used by aruminium/rane to bind the buffer directly to GPU/ANE.
    pub fn block(&self, dimension: u8, key: &[u8; 32]) -> Option<&Block> {
        match self.entries.get(&(dimension, *key))? {
            Entry::Pinned { block, .. } => Some(block),
            Entry::Heap(_) => None,
        }
    }

    /// Pre-allocate one contiguous IOSurface Block for `dimension` sized for
    /// `expected_count` entries of `bytes_per_entry` each.
    ///
    /// Subsequent `put()` into this dimension allocates cells from the pool
    /// instead of per-entry Blocks, giving stable descriptor addresses.
    pub fn reserve_pool(
        &mut self,
        dimension: u8,
        expected_count: usize,
        bytes_per_entry: usize,
    ) -> Result<(), MemError> {
        let total_bytes = (expected_count * bytes_per_entry).max(8);
        let block = Block::open(total_bytes)?;
        self.pools.insert(dimension, Pool {
            block,
            bytes_per_entry,
            capacity:  expected_count,
            next_slot: 0,
            slots:     HashMap::new(),
            free:      Vec::new(),
        });
        Ok(())
    }

    /// Pool Block and slot index for a key — used for direct GPU/ANE descriptor binding.
    /// The address `block.as_bytes()[slot * bytes_per_entry..]` is stable until `remove()`.
    pub fn cell(&self, dimension: u8, key: &[u8; 32]) -> Option<(&Block, usize)> {
        let pool = self.pools.get(&dimension)?;
        let &(slot_idx, _) = pool.slots.get(key)?;
        Some((&pool.block, slot_idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::dim;
    use nebu::Goldilocks;

    fn g(v: u64) -> Goldilocks { Goldilocks::new(v) }
    fn key(b: u8) -> [u8; 32] { [b; 32] }
    const BPE: usize = 64; // bytes_per_entry used in all pool tests

    #[test]
    fn pool_put_get_cell_roundtrip() {
        let mut store = UnimemStore::new();
        store.reserve_pool(0, 16, BPE).expect("reserve_pool");

        store.put(0, key(1), vec![g(10), g(20)]);
        store.put(0, key(2), vec![g(30)]);

        assert_eq!(store.get(0, &key(1)), Some([g(10), g(20)].as_slice()));
        assert_eq!(store.get(0, &key(2)), Some([g(30)].as_slice()));

        // Extract slot indices without holding the Block borrow.
        let slot1 = store.cell(0, &key(1)).expect("cell exists").1;
        let slot2 = store.cell(0, &key(2)).expect("cell exists").1;
        assert_ne!(slot1, slot2, "different keys must occupy different slots");

        // Overwrite key(1) — slot index must be stable.
        store.put(0, key(1), vec![g(99)]);
        let slot1b = store.cell(0, &key(1)).expect("cell still exists").1;
        assert_eq!(slot1, slot1b, "slot index stable on overwrite");

        // Verify value is in the Block at the expected byte offset.
        // Scoped so the Block borrow ends before any further store access.
        let val = {
            let (block, s) = store.cell(0, &key(1)).unwrap();
            let raw = &block.as_bytes()[s * BPE..s * BPE + 8];
            u64::from_le_bytes(raw.try_into().unwrap())
        };
        assert_eq!(val, 99, "block memory holds updated value");
    }

    #[test]
    fn pool_remove_frees_slot_for_reuse() {
        let mut store = UnimemStore::new();
        store.reserve_pool(0, 4, BPE).expect("reserve_pool");

        store.put(0, key(1), vec![g(1)]);
        store.put(0, key(2), vec![g(2)]);
        let (_, slot1) = store.cell(0, &key(1)).unwrap();

        assert_eq!(store.remove(0, &key(1)), Some(vec![g(1)]));
        assert!(store.get(0, &key(1)).is_none());
        assert!(store.cell(0, &key(1)).is_none());

        store.put(0, key(3), vec![g(3)]);
        let (_, slot3) = store.cell(0, &key(3)).unwrap();
        assert_eq!(slot1, slot3, "freed slot reused by next put");
    }

    #[test]
    fn pool_ephemeral_not_in_dirty() {
        let mut store = UnimemStore::new();
        store.reserve_pool(dim::EPHEMERAL, 8, BPE).expect("reserve_pool");
        store.put(dim::EPHEMERAL, key(1), vec![g(42)]);
        assert!(store.dirty_entries().is_empty(), "EPHEMERAL must not appear in dirty");
        assert_eq!(store.get(dim::EPHEMERAL, &key(1)), Some([g(42)].as_slice()));
    }
}
