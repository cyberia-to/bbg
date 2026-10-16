//! Fjall SSD view over BBG's shared transaction owner.
use super::database::{Backend, Database};
use super::{StorageResult, disk::DiskStore};
use std::path::PathBuf;

pub struct FjallStore {
    pub(crate) inner: DiskStore,
}
impl FjallStore {
    pub fn open(path: impl Into<PathBuf>) -> StorageResult<Self> {
        Ok(Self::from_database(Database::open(
            path.into(),
            Backend::Ssd,
        )?))
    }
}
super::disk::shard_view!(FjallStore);
