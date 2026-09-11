//! Fjall SSD shards: staged mutations and atomic, synced batches.

use super::access::{check_dimension, check_read_limit, collect_page, copy_value, io};
use super::buffer::WriteBuffer;
use super::{
    Durability, MAX_VALUE_ELEMENTS, ScanLimits, ShardEntry, ShardStore, StorageError,
    StorageResult, decode_marker, deserialize_goldilocks, dim, serialize_goldilocks,
};
use fjall::{Config, Keyspace, PartitionCreateOptions, PartitionHandle, PersistMode};
use nebu::Goldilocks;
use std::ops::Bound::{Excluded, Unbounded};
use std::path::PathBuf;

pub struct FjallStore {
    keyspace: Keyspace,
    parts: [PartitionHandle; 14],
    metadata: PartitionHandle,
    pending: WriteBuffer,
    // Drop after all keyspace/partition handles have released their resources.
    _lock: std::fs::File,
}

impl FjallStore {
    pub fn open(path: impl Into<PathBuf>) -> StorageResult<Self> {
        let path = path.into();
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
            Err(error) => return Err(io(error)),
        }
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join("bbg.lock"))
            .map_err(io)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StorageError::Busy,
            std::fs::TryLockError::Error(error) => io(error),
        })?;
        let keyspace = Config::new(&path).open().map_err(io)?;
        let p = |name| {
            keyspace
                .open_partition(name, PartitionCreateOptions::default())
                .map_err(io)
        };
        let parts = [
            p("particles")?,
            p("axons_out")?,
            p("axons_in")?,
            p("neurons")?,
            p("locations")?,
            p("coins")?,
            p("cards")?,
            p("files")?,
            p("time")?,
            p("signals")?,
            p("commitments")?,
            p("nullifiers")?,
            p("intents")?,
            p("ephemeral")?,
        ];
        let metadata = p("bbg_storage_v1")?;
        super::sync_parent(&path)?;
        let store = Self {
            keyspace,
            parts,
            metadata,
            pending: WriteBuffer::default(),
            _lock: lock,
        };
        store.last_commit()?;
        Ok(store)
    }

    /// Committed disk value, bypassing staged writes; prefer bounded read().
    pub fn load(&self, dimension: u8, key: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.load_bounded(dimension, key, MAX_VALUE_ELEMENTS)
    }

    fn load_bounded(
        &self,
        dimension: u8,
        key: &[u8; 32],
        limit: usize,
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.pending.ready()?;
        check_dimension(dimension)?;
        check_read_limit(limit)?;
        if dimension == dim::EPHEMERAL {
            return Ok(None);
        }
        self.parts[dimension as usize]
            .get(key)
            .map_err(io)?
            .map(|v| deserialize_goldilocks(v.as_ref(), limit))
            .transpose()
    }
}

impl ShardStore for FjallStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        if self.is_poisoned() {
            return None;
        }
        self.pending
            .cache
            .get(&(dimension, *key))
            .map(Vec::as_slice)
    }

    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) -> StorageResult<()> {
        self.pending.put(dimension, key, value)
    }

    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &self.pending.dirty
    }
    fn durability(&self) -> Durability {
        Durability::Disk
    }
    fn is_poisoned(&self) -> bool {
        self.pending.unknown.is_some()
    }
    fn has_pending(&self) -> bool {
        !self.pending.dirty.is_empty() || !self.pending.deleted.is_empty()
    }

    fn commit(&mut self) -> StorageResult<[u8; 32]> {
        self.pending.ready()?;
        let id = self.pending.change_id();
        if self.pending.dirty.is_empty() && self.pending.deleted.is_empty() {
            return Ok(self.last_commit()?.unwrap_or(id));
        }
        let mut batch = self.keyspace.batch().durability(Some(PersistMode::SyncAll));
        for (d, k, v) in &self.pending.dirty {
            batch.insert(&self.parts[*d as usize], *k, serialize_goldilocks(v));
        }
        for (d, k) in &self.pending.deleted {
            batch.remove(&self.parts[*d as usize], *k);
        }
        batch.insert(&self.metadata, b"last_commit", id);
        batch
            .commit()
            .map_err(|e| self.pending.fail_commit(id, e))?;
        self.pending.finish();
        Ok(id)
    }

    fn last_commit(&self) -> StorageResult<Option<[u8; 32]>> {
        self.pending.ready()?;
        self.metadata
            .get(b"last_commit")
            .map_err(io)?
            .map(|v| decode_marker(v.as_ref()))
            .transpose()
    }

    fn read(
        &self,
        dimension: u8,
        key: &[u8; 32],
        max_elements: usize,
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.pending.ready()?;
        check_dimension(dimension)?;
        check_read_limit(max_elements)?;
        if self.pending.deleted.contains(&(dimension, *key)) {
            return Ok(None);
        }
        if let Some(value) = self.pending.cache.get(&(dimension, *key)) {
            return Ok(Some(copy_value(value, max_elements)?));
        }
        self.load_bounded(dimension, key, max_elements)
    }

    fn scan(
        &self,
        dimension: u8,
        after: Option<[u8; 32]>,
        limits: ScanLimits,
    ) -> StorageResult<Vec<ShardEntry>> {
        check_dimension(dimension)?;
        limits.validate()?;
        self.pending.clean()?;
        if dimension == dim::EPHEMERAL {
            return super::scan_cache(&self.pending.cache, dimension, after, limits);
        }
        let start = after.map_or(Unbounded, Excluded);
        let entries = self.parts[dimension as usize]
            .range((start, Unbounded))
            .map(|item| {
                let (key, bytes) = item.map_err(io)?;
                let key = key
                    .as_ref()
                    .try_into()
                    .map_err(|_| StorageError::Corrupt("shard key length"))?;
                Ok((
                    key,
                    deserialize_goldilocks(bytes.as_ref(), limits.max_elements)?,
                ))
            });
        collect_page(entries, limits)
    }

    fn get_mut(&mut self, _dimension: u8, _key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        None
    }

    fn mark_dirty(&mut self, dimension: u8, _key: [u8; 32]) -> StorageResult<()> {
        self.pending.ready()?;
        check_dimension(dimension)
    }

    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>> {
        let value = self.read(dimension, key, MAX_VALUE_ELEMENTS)?;
        self.pending.delete(dimension, key)?;
        Ok(value)
    }

    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        Box::new(
            self.pending
                .cache
                .iter()
                .filter(move |(k, _)| k.0 == dimension && !self.is_poisoned())
                .map(|(k, v)| (&k.1, v.as_slice())),
        )
    }
}
