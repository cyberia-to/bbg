//! Validation shared by backend conversion and sealed application transfer.
use super::{Error, decode_head, decode_request};
use crate::storage::{
    StorageError,
    database::{ByteLimits, Database, MAX_TRANSACTION_BYTES, Table},
};
fn page_limits() -> ByteLimits {
    ByteLimits {
        max_entries: 4096,
        max_bytes: MAX_TRANSACTION_BYTES - 1024,
    }
}
#[cfg(all(feature = "backend-ssd", feature = "backend-hdd"))]
pub(super) fn hash_row(hash: &mut hemera::Hasher, key: &[u8], value: &[u8]) {
    hash.update(&(key.len() as u64).to_le_bytes());
    hash.update(key);
    hash.update(&(value.len() as u64).to_le_bytes());
    hash.update(value);
}
pub(super) fn validate_record(table: Table, key: &[u8], value: &[u8]) -> Result<(), Error> {
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
pub(super) fn validate_links(db: &Database) -> Result<(), Error> {
    for table in [Table::Heads, Table::Requests, Table::History] {
        let mut after: Option<Vec<u8>> = None;
        let mut previous: Option<([u8; 32], u64)> = None;
        loop {
            let rows = db.scan(table, after.as_deref(), &[], page_limits())?;
            if rows.is_empty() {
                break;
            }
            for (key, value) in &rows {
                validate_record(table, key, value)?;
            }
            if table == Table::Requests {
                // A disk-backed reverse index checks receipt coverage without
                // retaining an unbounded set of history positions in memory.
                db.transaction::<_, Error>(|tx| {
                    for (key, value) in &rows {
                        let head = decode_request(value)?.1;
                        let index = receipt_index(&key[..32], head.index);
                        if tx
                            .get(Table::Migration, &index, 32)?
                            .as_deref()
                            .is_some_and(|v| v != &key[32..])
                        {
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
