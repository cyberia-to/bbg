// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! HDD shard store backed by redb (B-tree MVCC).
//!
//! Write-through: every put lands in both the in-memory cache and redb.
//! get() reads from the cache. Entries grouped by dimension at commit time
//! so each redb table is opened once per transaction.

use std::collections::HashMap;
use std::path::Path;

use redb::{Database, TableDefinition};
use nebu::Goldilocks;

use super::{deserialize_goldilocks, dim, hash_dirty, serialize_goldilocks, ShardStore};

const TABLES: [TableDefinition<&[u8], &[u8]>; 13] = [
    TableDefinition::new("particles"),
    TableDefinition::new("axons_out"),
    TableDefinition::new("axons_in"),
    TableDefinition::new("neurons"),
    TableDefinition::new("locations"),
    TableDefinition::new("coins"),
    TableDefinition::new("cards"),
    TableDefinition::new("files"),
    TableDefinition::new("time"),
    TableDefinition::new("signals"),
    TableDefinition::new("commitments"),
    TableDefinition::new("nullifiers"),
    TableDefinition::new("ephemeral"),
];

pub struct RedbStore {
    db:    Database,
    cache: HashMap<(u8, [u8; 32]), Vec<Goldilocks>>,
    dirty: Vec<(u8, [u8; 32], Vec<Goldilocks>)>,
}

impl RedbStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, redb::DatabaseError> {
        Ok(Self { db: Database::create(path)?, cache: HashMap::new(), dirty: Vec::new() })
    }
}

impl ShardStore for RedbStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        self.cache.get(&(dimension, *key)).map(Vec::as_slice)
    }

    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) {
        if dimension != dim::EPHEMERAL {
            self.dirty.push((dimension, key, value.clone()));
        }
        self.cache.insert((dimension, key), value);
    }

    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &self.dirty
    }

    fn commit(&mut self) -> [u8; 32] {
        let out = hash_dirty(&self.dirty);

        // Group by dimension so each table is opened exactly once per transaction.
        let mut by_dim: [Vec<([u8; 32], Vec<u8>)>; 13] =
            std::array::from_fn(|_| Vec::new());
        for (d, k, v) in &self.dirty {
            by_dim[*d as usize].push((*k, serialize_goldilocks(v)));
        }

        if let Ok(txn) = self.db.begin_write() {
            for (d, entries) in by_dim.iter().enumerate() {
                if entries.is_empty() { continue; }
                if let Ok(mut table) = txn.open_table(TABLES[d]) {
                    for (k, v) in entries {
                        let _ = table.insert(k.as_slice(), v.as_slice());
                    }
                }
            }
            let _ = txn.commit();
        }

        self.dirty.clear();
        out
    }

    fn get_mut(&mut self, _dimension: u8, _key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        // Disk backends do not support in-place mutation.
        None
    }

    fn mark_dirty(&mut self, _dimension: u8, _key: [u8; 32]) {
        // No-op: get_mut always returns None, so callers have nothing to dirty.
    }

    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>> {
        let val = self.cache.remove(&(dimension, *key))?;
        self.dirty.retain(|(d, k, _)| !(*d == dimension && k == key));
        if dimension != dim::EPHEMERAL {
            if let Ok(txn) = self.db.begin_write() {
                if let Ok(mut table) = txn.open_table(TABLES[dimension as usize]) {
                    let _ = table.remove(key.as_slice());
                }
                let _ = txn.commit();
            }
        }
        Some(val)
    }

    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        Box::new(
            self.cache.iter()
                .filter(move |(k, _)| k.0 == dimension)
                .map(|(k, v)| (&k.1, v.as_slice()))
        )
    }
}

impl RedbStore {
    /// Load a key directly from disk, bypassing the write-through cache.
    pub fn load(&self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>> {
        let txn = self.db.begin_read().ok()?;
        let table = txn.open_table(TABLES[dimension as usize]).ok()?;
        let guard = table.get(key.as_slice()).ok()??;
        Some(deserialize_goldilocks(guard.value()))
    }
}
