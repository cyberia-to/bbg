//! Explicit legacy export: source stays locked; incomplete targets stay sealed.
use super::{
    Error,
    validation::{hash_row, validate_links, validate_record},
};
use crate::storage::{
    StorageError,
    access::io,
    database::{
        APPLICATION_TABLES, Backend, ByteLimits, Database, MAX_TRANSACTION_BYTES, RawEntry, Table,
    },
};
use redb::{Database as LegacyDatabase, TableDefinition, TableHandle};
use std::fs::OpenOptions;
use std::ops::Bound::{Excluded, Unbounded};
use std::path::Path;

pub(super) fn migrate(source: &Path, destination: &Path) -> Result<(), Error> {
    // open() requires an existing source; a typo cannot create a new database.
    let legacy = LegacyDatabase::open(source).map_err(io)?;
    let snapshot = legacy.begin_read().map_err(io)?;
    if snapshot
        .list_multimap_tables()
        .map_err(io)?
        .next()
        .is_some()
    {
        return Err(Error::Storage(
            "migration rejects unsupported multimap tables".into(),
        ));
    }
    let tables = snapshot.list_tables().map_err(io)?;
    let mut count = 0;
    for table in tables {
        let name = table.name();
        if !APPLICATION_TABLES.iter().any(|t| t.name() == name) {
            return Err(Error::Storage(
                "migration expects a legacy application-only database".into(),
            ));
        }
        count += 1;
    }
    if count != APPLICATION_TABLES.len() {
        return Err(Error::Corrupt);
    }
    if destination.try_exists().map_err(io)? {
        return Err(Error::Storage(
            "migration destination already exists".into(),
        ));
    }
    // This sibling guard is durable BEFORE creating the destination. Even a
    // crash before its first DB transaction cannot expose an empty new session.
    let guard_path = crate::storage::database::migration_guard(destination)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let guard = options.open(&guard_path).map_err(io)?;
    guard.sync_all().map_err(io)?;
    crate::storage::sync_parent(&guard_path)?;
    if let Err(error) = create_destination(destination) {
        // No import owns the target yet. Do not seal a competing store forever.
        drop(guard);
        std::fs::remove_file(&guard_path).map_err(io)?;
        crate::storage::sync_parent(&guard_path)?;
        return Err(error.into());
    }
    let target = Database::open_unchecked(destination, Backend::Ssd)?;
    target.transaction::<_, StorageError>(|tx| tx.put(Table::Migration, b"status", b"copying"))?;
    for table in APPLICATION_TABLES {
        let source_table = snapshot.open_table(definition(table)).map_err(io)?;
        let mut after: Option<Vec<u8>> = None;
        let mut hash = hemera::Hasher::new();
        loop {
            let rows = source_page(&source_table, after.as_deref())?;
            if rows.is_empty() {
                break;
            }
            for (key, value) in &rows {
                validate_record(table, key, value)?;
                hash_row(&mut hash, key, value);
            }
            target.transaction::<_, StorageError>(|tx| {
                for (key, value) in &rows {
                    tx.put(table, key, value)?;
                }
                Ok(())
            })?;
            after = rows.last().map(|(key, _)| key.clone());
        }
        let expected = *hash.finalize().as_bytes();
        let mut actual = hemera::Hasher::new();
        let mut after: Option<Vec<u8>> = None;
        loop {
            let rows = target.scan(table, after.as_deref(), &[], page_limits())?;
            if rows.is_empty() {
                break;
            }
            for (key, value) in &rows {
                hash_row(&mut actual, key, value);
            }
            after = rows.last().map(|(key, _)| key.clone());
        }
        if expected != *actual.finalize().as_bytes() {
            return Err(Error::Corrupt);
        }
    }
    validate_links(&target)?;
    target.transaction::<_, StorageError>(|tx| tx.put(Table::Migration, b"status", b"complete"))?;
    drop(target);
    drop(guard);
    std::fs::remove_file(&guard_path).map_err(io)?;
    crate::storage::sync_parent(&guard_path)?;
    Ok(())
}

fn definition(table: Table) -> TableDefinition<'static, &'static [u8], &'static [u8]> {
    TableDefinition::new(table.name())
}

fn create_destination(destination: &Path) -> crate::storage::StorageResult<()> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    // Atomic create must reject AlreadyExists. The general engine opener may
    // reopen a directory, so it cannot establish fresh-import ownership.
    builder.create(destination).map_err(io)
}

fn page_limits() -> ByteLimits {
    ByteLimits {
        max_entries: 4096,
        max_bytes: MAX_TRANSACTION_BYTES - 1024,
    }
}
fn source_page(
    table: &redb::ReadOnlyTable<&[u8], &[u8]>,
    after: Option<&[u8]>,
) -> Result<Vec<RawEntry>, Error> {
    let mut rows = Vec::new();
    let mut bytes = 0;
    let limits = page_limits();
    for row in table
        .range::<&[u8]>((after.map_or(Unbounded, Excluded), Unbounded))
        .map_err(io)?
        .take(limits.max_entries)
    {
        let (key, value) = row.map_err(io)?;
        if key.value().len() > 64 || value.value().len() > super::MAX_BYTES_VALUE {
            return Err(Error::Corrupt);
        }
        let size = key.value().len() + value.value().len();
        if size > limits.max_bytes {
            return Err(Error::Limit);
        }
        if size > limits.max_bytes - bytes {
            break;
        }
        rows.push((key.value().to_vec(), value.value().to_vec()));
        bytes += size;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    #[test]
    fn import_directory_creation_preserves_a_competing_store() {
        let root =
            std::env::temp_dir().join(format!("bbg-import-exclusive-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let store = root.join("bbg");
        super::create_destination(&store).unwrap();
        std::fs::write(store.join("sentinel"), b"competing data").unwrap();
        assert!(super::create_destination(&store).is_err());
        assert_eq!(
            std::fs::read(store.join("sentinel")).unwrap(),
            b"competing data"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
