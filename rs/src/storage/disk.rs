//! Shared shard view over the physical transaction owner.
use super::access::{check_dimension, check_read_limit, collect_page, copy_value};
use super::buffer::WriteBuffer;
use super::database::{ByteLimits, Database, Table};
use super::{
    Durability, MAX_VALUE_ELEMENTS, ScanLimits, ShardEntry, ShardStore, StorageError,
    StorageResult, decode_marker, deserialize_goldilocks, dim,
};
use nebu::Goldilocks;

pub struct DiskStore {
    pub(crate) db: Database,
    pub(crate) pending: WriteBuffer,
}

impl DiskStore {
    pub(crate) fn new(db: Database) -> Self {
        Self {
            db,
            pending: WriteBuffer::default(),
        }
    }
    pub fn database(&self) -> Database {
        self.db.clone()
    }
    fn ready(&self) -> StorageResult<()> {
        self.pending.ready()?;
        self.db.ready()
    }
    pub(crate) fn load(
        &self,
        dimension: u8,
        key: &[u8; 32],
        limit: usize,
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.ready()?;
        check_dimension(dimension)?;
        check_read_limit(limit)?;
        if dimension == dim::EPHEMERAL {
            return Ok(None);
        }
        self.db
            .read(Table::Shard(dimension), key, limit * 8)?
            .map(|bytes| deserialize_goldilocks(&bytes, limit))
            .transpose()
    }
}

impl ShardStore for DiskStore {
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
        self.ready()?;
        self.pending.put(dimension, key, value)
    }
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &self.pending.dirty
    }
    fn durability(&self) -> Durability {
        Durability::Disk
    }
    fn is_poisoned(&self) -> bool {
        self.pending.unknown.is_some() || self.db.is_poisoned()
    }
    fn has_pending(&self) -> bool {
        !self.pending.dirty.is_empty() || !self.pending.deleted.is_empty()
    }
    fn commit(&mut self) -> StorageResult<[u8; 32]> {
        self.ready()?;
        if !self.has_pending() {
            return Ok(self.last_commit()?.unwrap_or(self.pending.change_id()));
        }
        let result = self.db.transaction::<_, StorageError>(|tx| {
            for (d, k, v) in &self.pending.dirty {
                tx.put_shard(*d, *k, v)?;
            }
            for (d, k) in &self.pending.deleted {
                tx.remove(Table::Shard(*d), k)?;
            }
            Ok(())
        });
        match result {
            Ok(commit) => {
                let id = commit
                    .change_id
                    .ok_or(StorageError::Corrupt("missing shard commit identity"))?;
                self.pending.finish();
                Ok(id)
            }
            Err(error) => {
                if matches!(error, StorageError::CommitUnknown { .. }) {
                    self.pending.unknown = Some(error.clone());
                }
                Err(error)
            }
        }
    }
    fn last_commit(&self) -> StorageResult<Option<[u8; 32]>> {
        self.ready()?;
        self.db
            .read(Table::Metadata, b"last_commit", 32)?
            .map(|v| decode_marker(&v))
            .transpose()
    }
    fn read(
        &self,
        dimension: u8,
        key: &[u8; 32],
        max_elements: usize,
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.ready()?;
        check_dimension(dimension)?;
        check_read_limit(max_elements)?;
        if self.pending.deleted.contains(&(dimension, *key)) {
            return Ok(None);
        }
        if let Some(value) = self.pending.cache.get(&(dimension, *key)) {
            return Ok(Some(copy_value(value, max_elements)?));
        }
        self.load(dimension, key, max_elements)
    }
    fn scan(
        &self,
        dimension: u8,
        after: Option<[u8; 32]>,
        limits: ScanLimits,
    ) -> StorageResult<Vec<ShardEntry>> {
        self.ready()?;
        check_dimension(dimension)?;
        limits.validate()?;
        self.pending.clean()?;
        if dimension == dim::EPHEMERAL {
            return super::scan_cache(&self.pending.cache, dimension, after, limits);
        }
        let rows = self.db.scan(
            Table::Shard(dimension),
            after.as_ref().map(|k| k.as_slice()),
            &[],
            ByteLimits {
                max_entries: limits.max_entries,
                max_bytes: limits.max_elements * 8 + limits.max_entries * 32,
            },
        )?;
        collect_page(
            rows.into_iter().map(|(key, bytes)| {
                Ok((
                    key.as_slice()
                        .try_into()
                        .map_err(|_| StorageError::Corrupt("shard key length"))?,
                    deserialize_goldilocks(&bytes, limits.max_elements)?,
                ))
            }),
            limits,
        )
    }
    fn get_mut(&mut self, _dimension: u8, _key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        None
    }
    fn mark_dirty(&mut self, dimension: u8, _key: [u8; 32]) -> StorageResult<()> {
        self.ready()?;
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

// The public backend constructors retain their names while both use one view.
macro_rules! shard_view {
    ($name:ident) => {
        impl $name {
            fn from_database(db: super::database::Database) -> Self {
                Self {
                    inner: super::disk::DiskStore::new(db),
                }
            }
            pub fn database(&self) -> super::database::Database {
                self.inner.db.clone()
            }
            pub fn load(
                &self,
                dimension: u8,
                key: &[u8; 32],
            ) -> super::StorageResult<Option<Vec<nebu::Goldilocks>>> {
                self.inner.load(dimension, key, super::MAX_VALUE_ELEMENTS)
            }
        }
        impl super::ShardStore for $name {
            fn get(&self, d: u8, k: &[u8; 32]) -> Option<&[nebu::Goldilocks]> {
                self.inner.get(d, k)
            }
            fn get_mut(&mut self, d: u8, k: &[u8; 32]) -> Option<&mut [nebu::Goldilocks]> {
                self.inner.get_mut(d, k)
            }
            fn put(
                &mut self,
                d: u8,
                k: [u8; 32],
                v: Vec<nebu::Goldilocks>,
            ) -> super::StorageResult<()> {
                self.inner.put(d, k, v)
            }
            fn mark_dirty(&mut self, d: u8, k: [u8; 32]) -> super::StorageResult<()> {
                self.inner.mark_dirty(d, k)
            }
            fn remove(
                &mut self,
                d: u8,
                k: &[u8; 32],
            ) -> super::StorageResult<Option<Vec<nebu::Goldilocks>>> {
                self.inner.remove(d, k)
            }
            fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<nebu::Goldilocks>)] {
                self.inner.dirty_entries()
            }
            fn has_pending(&self) -> bool {
                self.inner.has_pending()
            }
            fn commit(&mut self) -> super::StorageResult<[u8; 32]> {
                self.inner.commit()
            }
            fn last_commit(&self) -> super::StorageResult<Option<[u8; 32]>> {
                self.inner.last_commit()
            }
            fn durability(&self) -> super::Durability {
                self.inner.durability()
            }
            fn is_poisoned(&self) -> bool {
                self.inner.is_poisoned()
            }
            fn read(
                &self,
                d: u8,
                k: &[u8; 32],
                max: usize,
            ) -> super::StorageResult<Option<Vec<nebu::Goldilocks>>> {
                self.inner.read(d, k, max)
            }
            fn scan(
                &self,
                d: u8,
                after: Option<[u8; 32]>,
                limits: super::ScanLimits,
            ) -> super::StorageResult<Vec<super::ShardEntry>> {
                self.inner.scan(d, after, limits)
            }
            fn iter(
                &self,
                d: u8,
            ) -> Box<dyn Iterator<Item = (&[u8; 32], &[nebu::Goldilocks])> + '_> {
                self.inner.iter(d)
            }
        }
    };
}
pub(crate) use shard_view;
