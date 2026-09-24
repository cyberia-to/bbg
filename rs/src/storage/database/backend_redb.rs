use super::{ByteLimits, Changes, RawEntry, Table, copy_value, push_row, scan_bounds, unknown};
use crate::storage::access::io;
use crate::storage::{StorageError, StorageResult};
use ::redb::{Database, Durability, TableDefinition, TableError};
use std::fs::OpenOptions;
use std::path::Path;

pub(in crate::storage::database) struct RedbEngine {
    db: Database,
}

impl RedbEngine {
    pub(super) fn open(path: &Path) -> StorageResult<Self> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path).map_err(io)?;
        let db = Database::builder()
            .create_file(file)
            .map_err(|error| match error {
                ::redb::DatabaseError::DatabaseAlreadyOpen => StorageError::Busy,
                error => io(error),
            })?;
        crate::storage::sync_parent(path)?;
        Ok(Self { db })
    }

    #[cfg(test)]
    pub(super) fn from_database(db: Database) -> Self {
        Self { db }
    }

    pub(super) fn get(
        &self,
        table: Table,
        key: &[u8],
        max_bytes: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        let txn = self.db.begin_read().map_err(io)?;
        let table = match txn.open_table(definition(table)) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(error) => return Err(io(error)),
        };
        table
            .get(key)
            .map_err(io)?
            .map(|value| copy_value(value.value(), max_bytes))
            .transpose()
    }

    pub(super) fn scan(
        &self,
        table: Table,
        after: Option<&[u8]>,
        prefix: &[u8],
        limits: ByteLimits,
    ) -> StorageResult<Vec<RawEntry>> {
        let Some((start, end)) = scan_bounds(after, prefix) else {
            return Ok(Vec::new());
        };
        let txn = self.db.begin_read().map_err(io)?;
        let table = match txn.open_table(definition(table)) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
            Err(error) => return Err(io(error)),
        };
        let mut page = Vec::new();
        let mut bytes = 0;
        let entries = table
            .range::<&[u8]>((
                start.as_ref().map(Vec::as_slice),
                end.as_ref().map(Vec::as_slice),
            ))
            .map_err(io)?;
        for entry in entries.take(limits.max_entries) {
            let (key, value) = entry.map_err(io)?;
            if !push_row(&mut page, &mut bytes, key.value(), value.value(), limits)? {
                break;
            }
        }
        Ok(page)
    }

    pub(super) fn commit(&mut self, changes: &Changes, id: [u8; 32]) -> StorageResult<()> {
        let mut txn = self.db.begin_write().map_err(io)?;
        txn.set_durability(Durability::Immediate);
        let mut entries = changes.iter().peekable();
        while let Some(((name, _), _)) = entries.peek() {
            let current = *name;
            let mut table = txn.open_table(definition(current)).map_err(io)?;
            while entries
                .peek()
                .is_some_and(|((name, _), _)| *name == current)
            {
                let Some(((_, key), value)) = entries.next() else {
                    break;
                };
                match value {
                    Some(value) => {
                        table.insert(key.as_slice(), value.as_slice()).map_err(io)?;
                    }
                    None => {
                        table.remove(key.as_slice()).map_err(io)?;
                    }
                }
            }
        }
        txn.commit().map_err(|error| unknown(id, error))
    }
}

fn definition(table: Table) -> TableDefinition<'static, &'static [u8], &'static [u8]> {
    TableDefinition::new(table.name())
}
