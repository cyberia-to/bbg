// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! SSD shard store backed by fjall (LSM-tree).
//!
//! Write-through: every put lands in both the in-memory cache and fjall.
//! get() reads from the cache; disk reads on cache miss are a future
//! optimization (requires lazy population on open or streaming reads).

use std::collections::HashMap;
use std::path::PathBuf;

use fjall::{Config, Keyspace, PartitionCreateOptions, PartitionHandle, PersistMode};
use nebu::Goldilocks;

use super::{deserialize_goldilocks, dim, hash_dirty, serialize_goldilocks, ShardStore};

pub struct FjallStore {
    keyspace: Keyspace,
    parts:    [PartitionHandle; 14],
    cache:    HashMap<(u8, [u8; 32]), Vec<Goldilocks>>,
    dirty:    Vec<(u8, [u8; 32], Vec<Goldilocks>)>,
}

impl FjallStore {
    pub fn open(path: impl Into<PathBuf>) -> fjall::Result<Self> {
        let ks = Config::new(path.into()).open()?;
        let p = |name| ks.open_partition(name, PartitionCreateOptions::default());
        let parts = [
            p("particles")?, p("axons_out")?, p("axons_in")?, p("neurons")?,
            p("locations")?, p("coins")?,     p("cards")?,    p("files")?,
            p("time")?,      p("signals")?,   p("commitments")?, p("nullifiers")?,
            p("intents")?,   p("ephemeral")?,
        ];
        Ok(Self { keyspace: ks, parts, cache: HashMap::new(), dirty: Vec::new() })
    }
}

impl ShardStore for FjallStore {
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
        for (d, k, v) in &self.dirty {
            let _ = self.parts[*d as usize].insert(k, serialize_goldilocks(v));
        }
        let _ = self.keyspace.persist(PersistMode::SyncAll);
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
            let _ = self.parts[dimension as usize].remove(*key);
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

impl FjallStore {
    /// Load a key directly from disk, bypassing the write-through cache.
    /// Used for cold reads after a store is reopened over an existing dataset.
    pub fn load(&self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>> {
        let bytes = self.parts[dimension as usize].get(key).ok()??;
        Some(deserialize_goldilocks(bytes.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A fresh directory under the OS temp dir, unique per test process and
    /// call, so parallel `cargo test` runs never collide on the same
    /// keyspace path.
    fn temp_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "bbg-fjall-test-{tag}-{}-{n}",
            std::process::id()
        ))
    }

    fn g(v: u64) -> Goldilocks { Goldilocks::new(v) }

    #[test]
    fn commit_survives_reopen_at_the_same_path() {
        let path = temp_path("restart");
        let key = [7u8; 32];
        let value = vec![g(1), g(2), g(3)];

        {
            let mut store = FjallStore::open(&path).expect("open for write");
            store.put(dim::PARTICLES, key, value.clone());
            store.commit();
            // store is dropped here, simulating process exit.
        }

        let reopened = FjallStore::open(&path).expect("reopen over existing dataset");
        // The in-memory cache starts empty on a fresh handle; `load` must
        // read the committed bytes back from disk, not from the cache.
        assert_eq!(reopened.load(dim::PARTICLES, &key), Some(value));

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn uncommitted_writes_do_not_survive_reopen() {
        let path = temp_path("uncommitted");
        let key = [9u8; 32];

        {
            let mut store = FjallStore::open(&path).expect("open for write");
            store.put(dim::PARTICLES, key, vec![g(42)]);
            // No commit(): a crash before persist must not fabricate durability.
        }

        let reopened = FjallStore::open(&path).expect("reopen over existing dataset");
        assert_eq!(reopened.load(dim::PARTICLES, &key), None);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn ephemeral_entries_never_reach_disk() {
        let path = temp_path("ephemeral");
        let key = [3u8; 32];

        {
            let mut store = FjallStore::open(&path).expect("open for write");
            store.put(dim::EPHEMERAL, key, vec![g(99)]);
            store.commit();
        }

        let reopened = FjallStore::open(&path).expect("reopen over existing dataset");
        assert_eq!(reopened.load(dim::EPHEMERAL, &key), None);

        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn removed_entries_do_not_survive_reopen() {
        let path = temp_path("removed");
        let key = [5u8; 32];

        {
            let mut store = FjallStore::open(&path).expect("open for write");
            store.put(dim::PARTICLES, key, vec![g(11)]);
            store.commit();
            store.remove(dim::PARTICLES, &key);
        }

        let reopened = FjallStore::open(&path).expect("reopen over existing dataset");
        assert_eq!(reopened.load(dim::PARTICLES, &key), None);

        std::fs::remove_dir_all(&path).ok();
    }
}
