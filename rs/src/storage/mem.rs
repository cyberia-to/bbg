// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! In-memory shard store. Optimal for hot state (≤ 64 GB, 50 ns read).

use std::collections::HashMap;

use nebu::Goldilocks;

use super::{dim, hash_dirty, ShardStore};

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
        if dimension != dim::EPHEMERAL {
            self.dirty.push((dimension, key, value.clone()));
        }
        self.data.insert((dimension, key), value);
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
        self.data.get_mut(&(dimension, *key)).map(Vec::as_mut_slice)
    }

    fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]) {
        if dimension == dim::EPHEMERAL { return; }
        let val = self.data.get(&(dimension, key)).map(|v| v.clone());
        if let Some(v) = val {
            self.dirty.push((dimension, key, v));
        }
    }

    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>> {
        let val = self.data.remove(&(dimension, *key))?;
        self.dirty.retain(|(d, k, _)| !(*d == dimension && k == key));
        Some(val)
    }

    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        Box::new(
            self.data.iter()
                .filter(move |(k, _)| k.0 == dimension)
                .map(|(k, v)| (&k.1, v.as_slice()))
        )
    }
}
