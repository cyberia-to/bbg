//! In-memory shards with coalesced pending changes and bounded range reads.

use super::buffer::WriteBuffer;
use super::{ScanLimits, ShardEntry, ShardStore, StorageResult, dim};
use nebu::Goldilocks;

#[derive(Default)]
pub struct MemStore {
    state: WriteBuffer,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ShardStore for MemStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        self.state.cache.get(&(dimension, *key)).map(Vec::as_slice)
    }

    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) -> StorageResult<()> {
        self.state.put(dimension, key, value)
    }

    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        &self.state.dirty
    }

    fn has_pending(&self) -> bool {
        !self.state.dirty.is_empty() || !self.state.deleted.is_empty()
    }

    fn commit(&mut self) -> StorageResult<[u8; 32]> {
        let id = self.state.change_id();
        self.state.clear_pending();
        Ok(id)
    }

    fn get_mut(&mut self, dimension: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        self.state
            .cache
            .get_mut(&(dimension, *key))
            .map(Vec::as_mut_slice)
    }

    fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]) -> StorageResult<()> {
        super::access::check_dimension(dimension)?;
        if dimension != dim::EPHEMERAL
            && let Some(value) = self.state.cache.get(&(dimension, key)).cloned()
        {
            self.state.put(dimension, key, value)?;
        }
        Ok(())
    }

    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>> {
        let value = self.state.cache.get(&(dimension, *key)).cloned();
        self.state.delete(dimension, key)?;
        Ok(value)
    }

    fn scan(
        &self,
        dimension: u8,
        after: Option<[u8; 32]>,
        limits: ScanLimits,
    ) -> StorageResult<Vec<ShardEntry>> {
        super::scan_cache(&self.state.cache, dimension, after, limits)
    }

    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        Box::new(
            self.state
                .cache
                .range((dimension, [0; 32])..=(dimension, [255; 32]))
                .map(|(k, v)| (&k.1, v.as_slice())),
        )
    }
}
