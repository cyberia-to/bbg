//! Explicit legacy export: source stays locked; incomplete targets stay sealed.
use super::{Error, decode_head, decode_request};
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
fn hash_row(hash: &mut hemera::Hasher, key: &[u8], value: &[u8]) {
    hash.update(&(key.len() as u64).to_le_bytes());
    hash.update(key);
    hash.update(&(value.len() as u64).to_le_bytes());
    hash.update(value);
}
fn validate_record(table: Table, key: &[u8], value: &[u8]) -> Result<(), Error> {
    let valid = match table {
        Table::Content => key.len() == 32,
        Table::Heads => key.len() == 32 && decode_head(value).is_ok(),
        Table::History => key.len() == 40 && value.len() == 32,
        Table::Requests => key.len() == 64 && decode_request(value).is_ok(),
        Table::Claims => key.len() == 32 && value.len() == 32,
        _ => false,
    };
    if valid { Ok(()) } else { Err(Error::Corrupt) }
}
fn validate_links(db: &Database) -> Result<(), Error> {
    for table in [Table::Heads, Table::Requests, Table::History] {
        let mut after: Option<Vec<u8>> = None;
        let mut previous: Option<([u8; 32], u64)> = None;
        loop {
            let rows = db.scan(table, after.as_deref(), &[], page_limits())?;
            if rows.is_empty() {
                break;
            }
            if table == Table::Requests {
                // A disk-backed reverse index checks receipt coverage without
                // retaining an unbounded set of history positions in memory.
                db.transaction::<_, Error>(|tx| {
                    for (key, value) in &rows {
                        let head = decode_request(value)?.1;
                        let index = receipt_index(&key[..32], head.index);
                        if tx.get(Table::Migration, &index, 32)?.is_some() {
                            return Err(Error::Corrupt);
                        }
                        tx.put(Table::Migration, &index, &key[32..])?;
                    }
                    Ok(())
                })?;
            }
            for (key, value) in &rows {
                let namespace: [u8; 32] = key[..32].try_into().map_err(|_| Error::Corrupt)?;
                let head = match table {
                    Table::Heads => decode_head(value)?,
                    Table::Requests => decode_request(value)?.1,
                    Table::History => {
                        let index =
                            u64::from_be_bytes(key[32..].try_into().map_err(|_| Error::Corrupt)?);
                        let expected = match previous {
                            Some((ns, n)) if ns == namespace => {
                                n.checked_add(1).ok_or(Error::Corrupt)?
                            }
                            _ => 0,
                        };
                        if index != expected {
                            return Err(Error::Corrupt);
                        }
                        previous = Some((namespace, index));
                        super::Head {
                            index,
                            commit: value.as_slice().try_into().map_err(|_| Error::Corrupt)?,
                        }
                    }
                    _ => return Err(Error::Corrupt),
                };
                if db
                    .read(
                        Table::History,
                        &super::history_key(&namespace, head.index),
                        32,
                    )?
                    .as_deref()
                    != Some(head.commit.as_slice())
                    || db
                        .read(Table::Content, &head.commit, super::MAX_BYTES_VALUE)?
                        .is_none()
                {
                    return Err(Error::Corrupt);
                }
                let selected = db
                    .read(Table::Heads, &namespace, 40)?
                    .ok_or(Error::Corrupt)?;
                let selected = decode_head(&selected)?;
                if head.index > selected.index {
                    return Err(Error::Corrupt);
                }
                if table == Table::History
                    && db
                        .read(Table::Migration, &receipt_index(&namespace, head.index), 32)?
                        .is_none()
                {
                    return Err(Error::Corrupt);
                }
            }
            after = rows.last().map(|(key, _)| key.clone());
        }
    }
    loop {
        let rows = db.scan(Table::Migration, None, b"r", page_limits())?;
        if rows.is_empty() {
            break;
        }
        db.transaction::<_, StorageError>(|tx| {
            for (key, _) in &rows {
                tx.remove(Table::Migration, key)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn receipt_index(namespace: &[u8], index: u64) -> [u8; 41] {
    let mut key = [0; 41];
    key[0] = b'r';
    key[1..33].copy_from_slice(namespace);
    key[33..].copy_from_slice(&index.to_be_bytes());
    key
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
