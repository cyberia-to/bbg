// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Storage interface for BBG polynomial state.
//!
//! ShardStore is independent of the polynomial commitment layer — the store
//! does not know about polynomials; the polynomial does not know about storage.
//! Three backends are planned; only MemStore is implemented here.

use std::collections::HashMap;

use hemera::hash as hemera_hash;
use nebu::Goldilocks;

/// Dimension identifiers — maps to the 10 BBG_poly dimensions + 2 private polynomials.
pub mod dim {
    pub const PARTICLES:    u8 = 0;
    pub const AXONS_OUT:    u8 = 1;
    pub const AXONS_IN:     u8 = 2;
    pub const NEURONS:      u8 = 3;
    pub const LOCATIONS:    u8 = 4;
    pub const COINS:        u8 = 5;
    pub const CARDS:        u8 = 6;
    pub const FILES:        u8 = 7;
    pub const TIME:         u8 = 8;
    pub const SIGNALS:      u8 = 9;
    /// A(x) — private commitment polynomial (NOT a BBG_poly dimension)
    pub const COMMITMENTS:  u8 = 10;
    /// N(x) — private nullifier polynomial (NOT a BBG_poly dimension)
    pub const NULLIFIERS:   u8 = 11;
}

/// Storage interface for a polynomial evaluation shard.
///
/// The store holds raw field-element evaluation tables. Authentication is
/// provided by the Lens commitment layer above — the store has no opinion
/// on correctness. `commit()` returns a shard sub-root for change tracking.
pub trait ShardStore: Send + Sync {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]>;
    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>);
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)];
    fn commit(&mut self) -> [u8; 32];
}

/// In-memory shard store. Optimal for hot state (≤ 64 GB, 50 ns read).
pub struct MemStore {
    data:  HashMap<(u8, [u8; 32]), Vec<Goldilocks>>,
    dirty: Vec<(u8, [u8; 32], Vec<Goldilocks>)>,
}

impl MemStore {
    pub fn new() -> Self {
        Self { data: HashMap::new(), dirty: Vec::new() }
    }
}

impl Default for MemStore {
    fn default() -> Self { Self::new() }
}

impl ShardStore for MemStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        self.data.get(&(dimension, *key)).map(Vec::as_slice)
    }

    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) {
        self.dirty.push((dimension, key, value.clone()));
        self.data.insert((dimension, key), value);
    }

    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &self.dirty
    }

    /// Flush dirty entries and return a sub-commitment of changed keys.
    ///
    /// The sub-commitment is a hemera hash of the serialised dirty key list.
    /// The authoritative commitment is computed by the Lens layer in `dim.rs`;
    /// this value is only used for change-set tracking and replication.
    fn commit(&mut self) -> [u8; 32] {
        let mut buf: Vec<u8> = Vec::with_capacity(self.dirty.len() * 33);
        for (d, k, _) in &self.dirty {
            buf.push(*d);
            buf.extend_from_slice(k);
        }
        self.dirty.clear();
        let h = hemera_hash(&buf);
        let b = h.as_bytes();
        let mut out = [0u8; 32];
        out[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
        out
    }
}
