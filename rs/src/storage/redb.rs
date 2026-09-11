//! Redb shards: staged mutations and one Immediate transaction per commit.

use super::access::{check_dimension, check_read_limit, collect_page, copy_value, io};
use super::buffer::WriteBuffer;
use super::{
    Durability, MAX_VALUE_ELEMENTS, ScanLimits, ShardEntry, ShardStore, StorageError,
    StorageResult, decode_marker, deserialize_goldilocks, dim, serialize_goldilocks,
};
use nebu::Goldilocks;
use redb::{Database, Durability as RedbDurability, TableDefinition, TableError};
use std::ops::Bound::{Excluded, Unbounded};
use std::path::Path;

const TABLES: [TableDefinition<&[u8], &[u8]>; 14] = [
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
    TableDefinition::new("intents"),
    TableDefinition::new("ephemeral"),
];
const METADATA: TableDefinition<&[u8], &[u8]> = TableDefinition::new("bbg_storage_v1");

#[cfg(test)]
#[path = "redb_tests.rs"]
mod tests;

pub struct RedbStore {
    db: Database,
    pending: WriteBuffer,
}

impl RedbStore {
    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        let db = Database::create(path.as_ref()).map_err(|e| match e {
            redb::DatabaseError::DatabaseAlreadyOpen => StorageError::Busy,
            other => io(other),
        })?;
        super::sync_parent(path.as_ref())?;
        let store = Self {
            db,
            pending: WriteBuffer::default(),
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
        let txn = self.db.begin_read().map_err(io)?;
        let table = match txn.open_table(TABLES[dimension as usize]) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(io(e)),
        };
        table
            .get(key.as_slice())
            .map_err(io)?
            .map(|v| deserialize_goldilocks(v.value(), limit))
            .transpose()
    }
}
impl ShardStore for RedbStore {
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
        let mut txn = self.db.begin_write().map_err(io)?;
        txn.set_durability(RedbDurability::Immediate);
        for (dimension, definition) in TABLES.iter().enumerate().take(dim::EPHEMERAL as usize) {
            let d = dimension as u8;
            if !self.pending.dirty.iter().any(|(dim, _, _)| *dim == d)
                && !self.pending.deleted.iter().any(|(dim, _)| *dim == d)
            {
                continue;
            }
            let mut table = txn.open_table(*definition).map_err(io)?;
            for (_, key, value) in self.pending.dirty.iter().filter(|(dim, _, _)| *dim == d) {
                let bytes = serialize_goldilocks(value);
                table.insert(key.as_slice(), bytes.as_slice()).map_err(io)?;
            }
            for (_, key) in self.pending.deleted.iter().filter(|(dim, _)| *dim == d) {
                table.remove(key.as_slice()).map_err(io)?;
            }
        }
        txn.open_table(METADATA)
            .map_err(io)?
            .insert(b"last_commit".as_slice(), id.as_slice())
            .map_err(io)?;
        txn.commit().map_err(|e| self.pending.fail_commit(id, e))?;
        self.pending.finish();
        Ok(id)
    }

    fn last_commit(&self) -> StorageResult<Option<[u8; 32]>> {
        self.pending.ready()?;
        let txn = self.db.begin_read().map_err(io)?;
        let table = match txn.open_table(METADATA) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(io(e)),
        };
        table
            .get(b"last_commit".as_slice())
            .map_err(io)?
            .map(|v| decode_marker(v.value()))
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
        let txn = self.db.begin_read().map_err(io)?;
        let table = match txn.open_table(TABLES[dimension as usize]) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(e) => return Err(io(e)),
        };
        let entries = table
            .range::<&[u8]>((start.as_ref().map(|v| v.as_slice()), Unbounded))
            .map_err(io)?
            .map(|item| {
                let (key, value) = item.map_err(io)?;
                let key = key
                    .value()
                    .try_into()
                    .map_err(|_| StorageError::Corrupt("shard key length"))?;
                Ok((
                    key,
                    deserialize_goldilocks(value.value(), limits.max_elements)?,
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
