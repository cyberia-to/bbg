//! Redb HDD view over BBG's shared transaction owner.
use super::database::{Backend, Database};
use super::{StorageResult, disk::DiskStore};
use std::path::Path;

pub struct RedbStore {
    pub(crate) inner: DiskStore,
}
impl RedbStore {
    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        Ok(Self::from_database(Database::open(path, Backend::Hdd)?))
    }
}
super::disk::shard_view!(RedbStore);

#[cfg(test)]
#[path = "redb_tests.rs"]
mod tests;
