use super::super::{
    StorageError, StorageResult,
    access::{check_dimension, check_read_limit},
    deserialize_goldilocks, dim, serialize_goldilocks,
};
use super::{
    ByteLimits, Changes, Engine, MAX_BYTES_VALUE, MAX_TRANSACTION_BYTES, MAX_TRANSACTION_KEYS,
    RawEntry, Table,
};
use nebu::Goldilocks;

pub struct Transaction<'a> {
    engine: &'a Engine,
    changes: Changes,
    bytes: usize,
}

impl<'a> Transaction<'a> {
    pub(super) fn new(engine: &'a Engine) -> Self {
        Self {
            engine,
            changes: Changes::new(),
            bytes: 0,
        }
    }
    pub(super) fn into_changes(self) -> Changes {
        self.changes
    }

    pub fn read_shard(
        &self,
        dimension: u8,
        key: &[u8; 32],
        max_elements: usize,
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        disk_dimension(dimension)?;
        check_read_limit(max_elements)?;
        self.get(Table::Shard(dimension), key, max_elements * 8)?
            .map(|v| deserialize_goldilocks(&v, max_elements))
            .transpose()
    }

    pub fn put_shard(
        &mut self,
        dimension: u8,
        key: [u8; 32],
        value: &[Goldilocks],
    ) -> StorageResult<()> {
        disk_dimension(dimension)?;
        check_read_limit(value.len())?;
        self.put(Table::Shard(dimension), &key, &serialize_goldilocks(value))
    }

    pub fn remove_shard(
        &mut self,
        dimension: u8,
        key: &[u8; 32],
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        let previous = self.read_shard(dimension, key, super::super::MAX_VALUE_ELEMENTS)?;
        self.remove(Table::Shard(dimension), key)?;
        Ok(previous)
    }

    pub(crate) fn get(
        &self,
        table: Table,
        key: &[u8],
        max: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        if key.len() > 64 || max > MAX_BYTES_VALUE {
            return Err(StorageError::Limit("byte read budget"));
        }
        if let Some(value) = self.changes.get(&(table, key.to_vec())) {
            return value
                .as_ref()
                .map(|v| {
                    if v.len() > max {
                        Err(StorageError::Limit("value exceeds read budget"))
                    } else {
                        Ok(v.clone())
                    }
                })
                .transpose();
        }
        self.engine.get(table, key, max)
    }

    pub(crate) fn put(&mut self, table: Table, key: &[u8], value: &[u8]) -> StorageResult<()> {
        self.stage(table, key, Some(value))
    }
    pub(crate) fn remove(&mut self, table: Table, key: &[u8]) -> StorageResult<()> {
        self.stage(table, key, None)
    }
    fn stage(&mut self, table: Table, key: &[u8], value: Option<&[u8]>) -> StorageResult<()> {
        if key.len() > 64 || value.is_some_and(|v| v.len() > MAX_BYTES_VALUE) {
            return Err(StorageError::Limit("transaction record"));
        }
        let address = (table, key.to_vec());
        let previous = self.changes.get(&address);
        // Reserve both automatic marker records: last_commit (11+32 bytes),
        // last_transaction (16+32 bytes). Total transaction limits include them.
        if previous.is_none() && self.changes.len() >= MAX_TRANSACTION_KEYS - 2 {
            return Err(StorageError::Limit("transaction key count"));
        }
        let old = previous.map_or(0, |v| key.len() + v.as_ref().map_or(0, Vec::len));
        let bytes = self.bytes - old + key.len() + value.map_or(0, <[u8]>::len);
        if bytes > MAX_TRANSACTION_BYTES - 91 {
            return Err(StorageError::Limit("transaction bytes"));
        }
        self.changes.insert(address, value.map(<[u8]>::to_vec));
        self.bytes = bytes;
        Ok(())
    }

    pub(crate) fn scan(
        &self,
        table: Table,
        after: Option<&[u8]>,
        prefix: &[u8],
        limits: ByteLimits,
    ) -> StorageResult<Vec<RawEntry>> {
        limits.validate()?;
        if after.is_some_and(|v| v.len() > 64) || prefix.len() > 64 {
            return Err(StorageError::Limit("scan key"));
        }
        if self.changes.keys().any(|(t, _)| *t == table) {
            return Err(StorageError::PendingWrites);
        }
        self.engine.scan(table, after, prefix, limits)
    }
}

fn disk_dimension(dimension: u8) -> StorageResult<()> {
    check_dimension(dimension)?;
    if dimension == dim::EPHEMERAL {
        return Err(StorageError::Unsupported("EPHEMERAL in disk transaction"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::database::{Backend, Database};

    #[test]
    fn staging_reserves_automatic_markers_within_total_transaction_limits() {
        let root = std::env::temp_dir().join(format!("bbg-tx-limit-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let backend = if cfg!(feature = "backend-ssd") {
            Backend::Ssd
        } else {
            Backend::Hdd
        };
        let db = Database::open(root.join("db"), backend).unwrap();
        let result = db.transaction::<_, StorageError>(|tx| {
            // Exactly 32 MiB of caller payload would leave no room for markers.
            let value = vec![1; 1024 * 1024 - 32];
            for n in 0..32u8 {
                tx.put(Table::Content, &[n; 32], &value)?;
            }
            Ok(())
        });
        assert!(matches!(
            result,
            Err(StorageError::Limit("transaction bytes"))
        ));
        assert!(db.last_transaction().unwrap().is_none());
        let result = db.transaction::<_, StorageError>(|tx| {
            for n in 0..MAX_TRANSACTION_KEYS - 1 {
                let mut key = [0; 32];
                key[..8].copy_from_slice(&(n as u64).to_le_bytes());
                tx.put(Table::Content, &key, &[])?;
            }
            Ok(())
        });
        assert!(matches!(
            result,
            Err(StorageError::Limit("transaction key count"))
        ));
        assert!(db.last_transaction().unwrap().is_none());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
