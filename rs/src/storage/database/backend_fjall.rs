use super::{ByteLimits, Changes, RawEntry, Table, copy_value, push_row, scan_bounds, unknown};
use crate::storage::access::io;
use crate::storage::{StorageError, StorageResult};
use ::fjall::{Config, Keyspace, PartitionCreateOptions, PartitionHandle, PersistMode};
use std::collections::BTreeMap;
use std::fs::{DirBuilder, File, OpenOptions};
use std::path::Path;

pub(in crate::storage::database) struct FjallEngine {
    keyspace: Keyspace,
    tables: BTreeMap<Table, PartitionHandle>,
    // The filesystem writer lock outlives every keyspace/partition handle.
    _lock: File,
}

impl FjallEngine {
    pub(super) fn open(path: &Path) -> StorageResult<Self> {
        let mut directory = DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        match directory.create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !path.is_dir() {
                    return Err(StorageError::Unsupported(
                        "legacy redb file requires explicit migration",
                    ));
                }
            }
            Err(error) => return Err(io(error)),
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(path.join("bbg.lock")).map_err(io)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StorageError::Busy,
            std::fs::TryLockError::Error(error) => io(error),
        })?;
        let keyspace = Config::new(path).open().map_err(io)?;
        let names = (0..14).map(Table::Shard).chain([
            Table::Metadata,
            Table::Content,
            Table::Heads,
            Table::History,
            Table::Requests,
            Table::Claims,
            Table::Migration,
        ]);
        let mut tables = BTreeMap::new();
        for table in names {
            let partition = keyspace
                .open_partition(table.name(), PartitionCreateOptions::default())
                .map_err(io)?;
            tables.insert(table, partition);
        }
        crate::storage::sync_parent(path)?;
        Ok(Self {
            keyspace,
            tables,
            _lock: lock,
        })
    }

    fn table(&self, table: Table) -> StorageResult<&PartitionHandle> {
        self.tables
            .get(&table)
            .ok_or(StorageError::Corrupt("unknown database table"))
    }

    pub(super) fn get(
        &self,
        table: Table,
        key: &[u8],
        max_bytes: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        self.table(table)?
            .get(key)
            .map_err(io)?
            .map(|bytes| copy_value(bytes.as_ref(), max_bytes))
            .transpose()
    }

    pub(super) fn scan(
        &self,
        table: Table,
        after: Option<&[u8]>,
        prefix: &[u8],
        limits: ByteLimits,
    ) -> StorageResult<Vec<RawEntry>> {
        let Some(bounds) = scan_bounds(after, prefix) else {
            return Ok(Vec::new());
        };
        let mut page = Vec::new();
        let mut bytes = 0;
        for entry in self.table(table)?.range(bounds).take(limits.max_entries) {
            let (key, value) = entry.map_err(io)?;
            if !push_row(&mut page, &mut bytes, &key, &value, limits)? {
                break;
            }
        }
        Ok(page)
    }

    pub(super) fn commit(&mut self, changes: &Changes, id: [u8; 32]) -> StorageResult<()> {
        let mut batch = self.keyspace.batch().durability(Some(PersistMode::SyncAll));
        for ((table, key), value) in changes {
            let partition = self.table(*table)?;
            match value {
                Some(value) => batch.insert(partition, key.as_slice(), value.as_slice()),
                None => batch.remove(partition, key.as_slice()),
            }
        }
        batch.commit().map_err(|error| unknown(id, error))
    }
}
