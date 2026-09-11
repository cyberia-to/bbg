//! Pending disk mutations, cache entries and unresolved commit state.

use super::access::{MAX_PENDING_ELEMENTS, MAX_PENDING_KEYS, MAX_VALUE_ELEMENTS, check_dimension};
use super::{StorageError, StorageResult, dim};
use nebu::Goldilocks;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type EntryKey = (u8, [u8; 32]);

#[derive(Default)]
pub(crate) struct WriteBuffer {
    pub cache: BTreeMap<EntryKey, Vec<Goldilocks>>,
    pub dirty: Vec<(u8, [u8; 32], Vec<Goldilocks>)>,
    pub deleted: BTreeSet<EntryKey>,
    pub unknown: Option<StorageError>,
    positions: BTreeMap<EntryKey, usize>,
    elements: usize,
}

impl WriteBuffer {
    pub fn ready(&self) -> StorageResult<()> {
        match &self.unknown {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    #[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
    pub fn clean(&self) -> StorageResult<()> {
        self.ready()?;
        if !self.dirty.is_empty() || !self.deleted.is_empty() {
            return Err(StorageError::PendingWrites);
        }
        Ok(())
    }

    pub fn put(
        &mut self,
        dimension: u8,
        key: [u8; 32],
        value: Vec<Goldilocks>,
    ) -> StorageResult<()> {
        self.ready()?;
        check_dimension(dimension)?;
        if value.len() > MAX_VALUE_ELEMENTS {
            return Err(StorageError::Limit("value element count"));
        }
        if dimension != dim::EPHEMERAL {
            let position = self.positions.get(&(dimension, key)).copied();
            let elements =
                self.elements - position.map_or(0, |i| self.dirty[i].2.len()) + value.len();
            if elements > MAX_PENDING_ELEMENTS {
                return Err(StorageError::Limit("pending element count"));
            }
            self.check_key_capacity(dimension, &key)?;
            self.deleted.remove(&(dimension, key));
            if let Some(i) = position {
                self.dirty[i].2 = value.clone();
            } else {
                self.positions.insert((dimension, key), self.dirty.len());
                self.dirty.push((dimension, key, value.clone()));
            }
            self.elements = elements;
        }
        self.cache.insert((dimension, key), value);
        Ok(())
    }

    fn check_key_capacity(&self, dimension: u8, key: &[u8; 32]) -> StorageResult<()> {
        let exists = self.deleted.contains(&(dimension, *key))
            || self.positions.contains_key(&(dimension, *key));
        if !exists && self.dirty.len() + self.deleted.len() >= MAX_PENDING_KEYS {
            return Err(StorageError::Limit("pending key count"));
        }
        Ok(())
    }

    pub fn delete(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<()> {
        self.ready()?;
        check_dimension(dimension)?;
        if dimension != dim::EPHEMERAL {
            self.check_key_capacity(dimension, key)?;
            if let Some(i) = self.positions.remove(&(dimension, *key)) {
                let (_, _, value) = self.dirty.swap_remove(i);
                self.elements -= value.len();
                if let Some((d, k, _)) = self.dirty.get(i) {
                    self.positions.insert((*d, *k), i);
                }
            }
            self.deleted.insert((dimension, *key));
        }
        self.cache.remove(&(dimension, *key));
        Ok(())
    }

    #[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
    pub fn finish(&mut self) {
        self.clear_pending();
        // Disk remains authoritative; keep only local-only cache values.
        self.cache.retain(|(d, _), _| *d == dim::EPHEMERAL);
    }

    pub fn clear_pending(&mut self) {
        self.dirty.clear();
        self.deleted.clear();
        self.positions.clear();
        self.elements = 0;
    }

    #[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
    pub fn fail_commit(
        &mut self,
        change_id: [u8; 32],
        error: impl std::fmt::Display,
    ) -> StorageError {
        let error = StorageError::CommitUnknown {
            change_id,
            message: error.to_string(),
        };
        self.unknown = Some(error.clone());
        error
    }

    pub fn change_id(&self) -> [u8; 32] {
        let mut changes: BTreeMap<EntryKey, Option<&[Goldilocks]>> = BTreeMap::new();
        for (d, k, v) in &self.dirty {
            changes.insert((*d, *k), Some(v));
        }
        for key in &self.deleted {
            changes.insert(*key, None);
        }
        let mut bytes = b"bbg/shard-batch/v1\0".to_vec();
        bytes.extend_from_slice(&(changes.len() as u64).to_le_bytes());
        for ((dimension, key), value) in changes {
            bytes.push(dimension);
            bytes.extend_from_slice(&key);
            bytes.push(u8::from(value.is_some()));
            if let Some(value) = value {
                bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
                bytes.extend_from_slice(&super::serialize_goldilocks(value));
            }
        }
        let hash = hemera::hash(&bytes);
        let mut out = [0; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    }
}
