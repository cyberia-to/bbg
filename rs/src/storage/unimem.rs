// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Apple Silicon zero-copy shard store backed by honeycrisp::unimem.
//!
//! Each value lives in its own IOSurface-pinned Block. The Block's physical
//! pages are visible without copying to CPU, AMX (Brakedown matrix ops),
//! Metal GPU, and ANE. get() returns a Goldilocks slice backed directly by
//! the pinned IOSurface memory — no copies at any stage.
//!
//! Fallback: if Block::open fails (e.g., non-Apple-Silicon CI), the entry
//! is stored in a heap Vec and get() returns normally.

use std::collections::HashMap;

use nebu::Goldilocks;
use unimem::Block;

use super::{hash_dirty, ShardStore};

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

pub struct UnimemStore {
    entries: HashMap<(u8, [u8; 32]), Entry>,
    dirty:   Vec<(u8, [u8; 32], Vec<Goldilocks>)>,
}

impl UnimemStore {
    pub fn new() -> Self {
        Self { entries: HashMap::new(), dirty: Vec::new() }
    }
}

impl Default for UnimemStore {
    fn default() -> Self { Self::new() }
}

impl ShardStore for UnimemStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
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
        let count = value.len();
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
        self.dirty.push((dimension, key, value));
    }

    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &self.dirty
    }

    fn commit(&mut self) -> [u8; 32] {
        let out = hash_dirty(&self.dirty);
        self.dirty.clear();
        out
    }
}

impl UnimemStore {
    /// Returns the raw Block for a key if it is IOSurface-pinned.
    /// Used by aruminium/rane to bind the buffer directly to GPU/ANE.
    pub fn block(&self, dimension: u8, key: &[u8; 32]) -> Option<&Block> {
        match self.entries.get(&(dimension, *key))? {
            Entry::Pinned { block, .. } => Some(block),
            Entry::Heap(_) => None,
        }
    }
}
