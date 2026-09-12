//! HOT cache with an authoritative WARM store and optional archival COLD data.
use super::{
    Durability, MAX_VALUE_ELEMENTS, NetworkStore, ScanLimits, ShardEntry, ShardStore, StorageError,
    StorageResult, dim,
};
use crate::types::Particle;
use nebu::Goldilocks;

pub struct TieredStore {
    hot: Box<dyn ShardStore>,
    warm: Option<Box<dyn ShardStore>>,
    cold: Option<Box<dyn ShardStore>>,
    network: Option<Box<dyn NetworkStore>>,
    failure: Option<StorageError>,
}

impl TieredStore {
    pub fn new(hot: Box<dyn ShardStore>) -> Self {
        Self {
            hot,
            warm: None,
            cold: None,
            network: None,
            failure: None,
        }
    }
    pub fn with_warm(mut self, warm: Box<dyn ShardStore>) -> StorageResult<Self> {
        if self.warm.is_some() {
            return Err(StorageError::Unsupported("WARM is already attached"));
        }
        if warm.durability() == Durability::Disk && self.hot.durability() != Durability::Memory {
            return Err(StorageError::Unsupported(
                "durable WARM requires a memory HOT tier",
            ));
        }
        if warm.durability() == Durability::Disk
            && (self.hot.has_pending()
                || (0..dim::EPHEMERAL).any(|d| self.hot.iter(d).next().is_some()))
        {
            return Err(StorageError::Unsupported(
                "attach durable WARM to an empty persistent HOT cache",
            ));
        }
        self.warm = Some(warm);
        Ok(self)
    }
    pub fn with_cold(mut self, cold: Box<dyn ShardStore>) -> Self {
        self.cold = Some(cold);
        self
    }
    pub fn with_network(mut self, net: Box<dyn NetworkStore>) -> Self {
        self.network = Some(net);
        self
    }

    fn ready(&self) -> StorageResult<()> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        Ok(())
    }

    fn retain_failure<T>(&mut self, result: StorageResult<T>) -> StorageResult<T> {
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    pub fn promote(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<bool> {
        self.ready()?;
        if let Some(value) = self.read(dimension, key, MAX_VALUE_ELEMENTS)? {
            self.hot.put(dimension, *key, value)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Evict only an unchanged value whose WARM batch is already committed.
    /// An eviction never commits the caller's unfinished transaction.
    pub fn evict(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<()> {
        self.ready()?;
        super::access::check_dimension(dimension)?;
        if dimension == dim::EPHEMERAL {
            return Ok(());
        }
        if let Some(warm) = &self.warm
            && !warm.has_pending()
        {
            let hot = self.hot.get(dimension, key);
            if let Some(value) = warm.read(dimension, key, MAX_VALUE_ELEMENTS)?
                && hot == Some(value.as_slice())
            {
                self.hot.remove(dimension, key)?;
            }
        }
        Ok(())
    }

    pub fn fetch_content(&self, particle: &Particle) -> Option<Vec<u8>> {
        self.network.as_ref()?.fetch(particle)
    }

    pub fn archive(&mut self) -> StorageResult<Option<[u8; 32]>> {
        self.ready()?;
        self.cold.as_mut().map(|c| c.commit()).transpose()
    }
}

impl ShardStore for TieredStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        if self.is_poisoned() {
            return None;
        }
        if let Some(value) = self.hot.get(dimension, key) {
            return Some(value);
        }
        if dimension == dim::EPHEMERAL {
            return None;
        }
        if let Some(warm) = &self.warm {
            return warm.get(dimension, key);
        }
        self.cold.as_ref().and_then(|cold| cold.get(dimension, key))
    }

    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) -> StorageResult<()> {
        self.ready()?;
        if dimension != dim::EPHEMERAL
            && let Some(warm) = &mut self.warm
        {
            warm.put(dimension, key, value.clone())?;
        }
        let result = self.hot.put(dimension, key, value);
        if dimension != dim::EPHEMERAL && self.warm.is_some() {
            self.retain_failure(result)
        } else {
            result
        }
    }

    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        self.hot.dirty_entries()
    }
    fn has_pending(&self) -> bool {
        self.hot.has_pending() || self.warm.as_ref().is_some_and(|w| w.has_pending())
    }
    fn durability(&self) -> Durability {
        self.warm
            .as_ref()
            .map_or_else(|| self.hot.durability(), |w| w.durability())
    }
    fn is_poisoned(&self) -> bool {
        self.failure.is_some()
            || self.hot.is_poisoned()
            || self.warm.as_ref().is_some_and(|w| w.is_poisoned())
    }

    fn commit(&mut self) -> StorageResult<[u8; 32]> {
        self.ready()?;
        let result = (|| {
            let durable_id = self.warm.as_mut().map(|warm| warm.commit()).transpose()?;
            let hot_id = self.hot.commit().map_err(|error| match durable_id {
                Some(change_id) if self.durability() == Durability::Disk => {
                    StorageError::CommitUnknown {
                        change_id,
                        message: format!("WARM committed; HOT publication failed: {error}"),
                    }
                }
                _ => error,
            })?;
            Ok(durable_id.unwrap_or(hot_id))
        })();
        self.retain_failure(result)
    }

    fn last_commit(&self) -> StorageResult<Option<[u8; 32]>> {
        self.ready()?;
        self.warm
            .as_ref()
            .map_or_else(|| self.hot.last_commit(), |w| w.last_commit())
    }

    fn read(
        &self,
        dimension: u8,
        key: &[u8; 32],
        max_elements: usize,
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.ready()?;
        // A poisoned owner must surface its unresolved outcome even on a HOT hit.
        if dimension != dim::EPHEMERAL
            && let Some(warm) = &self.warm
        {
            if !warm.is_poisoned()
                && let Some(hot) = self.hot.get(dimension, key)
            {
                return Ok(Some(super::access::copy_value(hot, max_elements)?));
            }
            return warm.read(dimension, key, max_elements);
        }
        let value = self.hot.read(dimension, key, max_elements)?;
        if value.is_some() || dimension == dim::EPHEMERAL {
            return Ok(value);
        }
        self.cold
            .as_ref()
            .map_or(Ok(None), |c| c.read(dimension, key, max_elements))
    }

    fn scan(
        &self,
        dimension: u8,
        after: Option<[u8; 32]>,
        limits: ScanLimits,
    ) -> StorageResult<Vec<ShardEntry>> {
        self.ready()?;
        if dimension != dim::EPHEMERAL {
            if let Some(warm) = &self.warm {
                return warm.scan(dimension, after, limits);
            }
            if self.cold.is_some() {
                return Err(StorageError::Unsupported(
                    "scan mixed HOT/archive without WARM",
                ));
            }
        }
        self.hot.scan(dimension, after, limits)
    }

    fn get_mut(&mut self, dimension: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        if self.is_poisoned() {
            return None;
        }
        self.hot.get_mut(dimension, key)
    }

    fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]) -> StorageResult<()> {
        self.ready()?;
        super::access::check_dimension(dimension)?;
        let result = (|| {
            if dimension != dim::EPHEMERAL
                && let (Some(value), Some(warm)) = (self.hot.get(dimension, &key), &mut self.warm)
            {
                warm.put(dimension, key, value.to_vec())?;
            }
            self.hot.mark_dirty(dimension, key)
        })();
        self.retain_failure(result)
    }

    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.ready()?;
        if dimension != dim::EPHEMERAL && self.warm.is_none() && self.cold.is_some() {
            return Err(StorageError::Unsupported(
                "archive deletion requires authoritative WARM",
            ));
        }
        let value = self.read(dimension, key, MAX_VALUE_ELEMENTS)?;
        if dimension != dim::EPHEMERAL
            && let Some(warm) = &mut self.warm
        {
            warm.remove(dimension, key)?;
        }
        let result = self.hot.remove(dimension, key).map(|_| value);
        if dimension != dim::EPHEMERAL && self.warm.is_some() {
            self.retain_failure(result)
        } else {
            result
        }
    }

    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        if self.is_poisoned() {
            return Box::new(std::iter::empty());
        }
        self.hot.iter(dimension)
    }
}

impl Default for TieredStore {
    fn default() -> Self {
        Self::new(Box::new(super::mem::MemStore::new()))
    }
}

#[cfg(test)]
#[path = "tiered_tests.rs"]
mod tests;
