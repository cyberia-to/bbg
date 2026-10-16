//! Read-only, bounded inspection of original or sealed application stores.
use super::{ApplicationStore, Error, Head, Particle, decode_head, decode_request, validation};
use crate::storage::database::{
    Backend, ByteLimits, Database, MAX_TRANSACTION_BYTES, RecordDomain, Table,
};
use std::{collections::BTreeSet, path::Path};

pub const MAX_INSPECTION_ROWS: u64 = 1_000_000;
pub const MAX_INSPECTION_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub(super) const TABLES: [Table; 5] = [
    Table::Content,
    Table::Claims,
    Table::Requests,
    Table::History,
    Table::Heads,
];
pub(super) const SEAL: &[u8] = b"application-export";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveSeal {
    pub target: Particle,
    pub nonce: Particle,
    pub prior: Particle,
    pub manifest: Particle,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveSummary {
    pub rows: u64,
    pub bytes: u64,
    pub tables: [u64; 5],
    pub digest: Particle,
}
/// Holds the exclusive backend lock and exposes no mutation or Database handle.
pub struct ApplicationArchive {
    pub(super) store: ApplicationStore,
    pub(super) sources: Vec<(Particle, Head)>,
    seal: Option<ArchiveSeal>,
    transaction: Option<Particle>,
}
impl ApplicationArchive {
    pub fn open(path: &Path) -> Result<Self, Error> {
        // Fjall's existing version marker prevents inspection from creating an
        // empty database at a typo or empty directory. Backend validates bytes.
        if !path.is_dir()
            || !path
                .join("version")
                .symlink_metadata()
                .map(|m| m.file_type().is_file())
                .unwrap_or(false)
        {
            return Err(Error::Storage(
                "archive must be an existing SSD database directory".into(),
            ));
        }
        if crate::storage::database::migration_guard(path)?
            .try_exists()
            .map_err(crate::storage::access::io)?
        {
            return Err(Error::Conflict);
        }
        let db = Database::open_unchecked(path, Backend::Ssd)?;
        db.check_known_tables()?;
        let status = db.read(Table::Migration, b"status", 16)?;
        if status
            .as_deref()
            .is_some_and(|s| s != b"complete" && s != b"export-v1")
        {
            return Err(Error::Conflict);
        }
        let tiny = ByteLimits {
            max_entries: 1,
            max_bytes: MAX_TRANSACTION_BYTES,
        };
        for domain in 0..14 {
            if !db.scan(Table::Shard(domain), None, &[], tiny)?.is_empty() {
                return Err(Error::Conflict);
            }
        }
        for domain in [
            RecordDomain::NativeState,
            RecordDomain::NativeHistory,
            RecordDomain::NativeRequests,
            RecordDomain::NativeMetadata,
            RecordDomain::NativeBalances,
            RecordDomain::NativeBlocks,
            RecordDomain::NativeExport,
        ] {
            if !db.scan(Table::Native(domain), None, &[], tiny)?.is_empty() {
                return Err(Error::Conflict);
            }
        }
        let heads = db.scan(
            Table::Heads,
            None,
            &[],
            ByteLimits {
                max_entries: 257,
                max_bytes: 32768,
            },
        )?;
        if heads.is_empty() || heads.len() > 256 {
            return Err(Error::Limit);
        }
        let sources: Vec<(Particle, Head)> = heads
            .iter()
            .map(|(id, h)| {
                Ok((
                    id.as_slice().try_into().map_err(|_| Error::Corrupt)?,
                    decode_head(h)?,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let seal = db
            .read(Table::Migration, SEAL, 128)?
            .map(|bytes| {
                if bytes.len() != 128 || status.as_deref() != Some(b"export-v1") {
                    return Err(Error::Corrupt);
                }
                let mut hash = hemera::Hasher::new();
                hash.update(b"bbg/application-transfer/1\0");
                hash.update(&bytes[..96]);
                for (id, head) in &sources {
                    hash.update(id);
                    hash.update(&head.index.to_le_bytes());
                    hash.update(&head.commit);
                }
                if hash.finalize().as_bytes() != &bytes[96..] {
                    return Err(Error::Corrupt);
                }
                Ok(ArchiveSeal {
                    target: bytes[..32].try_into().map_err(|_| Error::Corrupt)?,
                    nonce: bytes[32..64].try_into().map_err(|_| Error::Corrupt)?,
                    prior: bytes[64..96].try_into().map_err(|_| Error::Corrupt)?,
                    manifest: bytes[96..].try_into().map_err(|_| Error::Corrupt)?,
                })
            })
            .transpose()?;
        if status.as_deref() == Some(b"export-v1") && seal.is_none() {
            return Err(Error::Corrupt);
        }
        let transaction = db.last_transaction()?;
        Ok(Self {
            store: ApplicationStore::from_database(db),
            sources,
            seal,
            transaction,
        })
    }
    pub fn sources(&self) -> &[(Particle, Head)] {
        &self.sources
    }
    pub fn seal(&self) -> Option<&ArchiveSeal> {
        self.seal.as_ref()
    }
    pub fn last_transaction(&self) -> Option<Particle> {
        self.transaction
    }
    pub fn head(&self, namespace: &Particle) -> Result<Option<Head>, Error> {
        self.store.head(namespace)
    }
    pub fn content(&self, id: &Particle, max_bytes: usize) -> Result<Option<Vec<u8>>, Error> {
        self.store.content(id, max_bytes)
    }
    pub fn resolve(
        &self,
        namespace: &Particle,
        request: &Particle,
    ) -> Result<Option<(Particle, Head)>, Error> {
        self.store.resolve(namespace, request)
    }
    pub fn history(
        &self,
        namespace: &Particle,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<Head>, Error> {
        self.store.history(namespace, after, limit)
    }
    pub fn inspect(
        &self,
        max_rows: u64,
        max_bytes: u64,
        validate_content: impl Fn(Particle, &[u8], &Self) -> Result<(), Error>,
    ) -> Result<ArchiveSummary, Error> {
        if max_rows == 0
            || max_rows > MAX_INSPECTION_ROWS
            || max_bytes == 0
            || max_bytes > MAX_INSPECTION_BYTES
        {
            return Err(Error::Limit);
        }
        let mut result = ArchiveSummary {
            rows: 0,
            bytes: 0,
            tables: [0; 5],
            digest: [0; 32],
        };
        let mut hash = hemera::Hasher::new();
        hash.update(b"bbg/application-archive-rows/1\0");
        // Bounded by max_rows. Unlike transfer completion, inspection creates no
        // temporary reverse-index transactions in the source.
        let mut receipts = BTreeSet::<(Particle, u64)>::new();
        for (ordinal, table) in TABLES.into_iter().enumerate() {
            hash.update(&[ordinal as u8]);
            let mut after = None;
            let mut previous = None::<(Particle, u64)>;
            loop {
                let rows = self.store.db.scan(
                    table,
                    after.as_deref(),
                    &[],
                    ByteLimits {
                        max_entries: 512,
                        max_bytes: MAX_TRANSACTION_BYTES - 4096,
                    },
                )?;
                if rows.is_empty() {
                    break;
                }
                for (key, value) in &rows {
                    result.rows = result.rows.checked_add(1).ok_or(Error::Limit)?;
                    result.bytes = result
                        .bytes
                        .checked_add((key.len() + value.len()) as u64)
                        .ok_or(Error::Limit)?;
                    if result.rows > max_rows || result.bytes > max_bytes {
                        return Err(Error::Limit);
                    }
                    result.tables[ordinal] += 1;
                    validation::validate_record(table, key, value)?;
                    hash.update(&(key.len() as u64).to_le_bytes());
                    hash.update(key);
                    hash.update(&(value.len() as u64).to_le_bytes());
                    hash.update(value);
                    if table == Table::Content {
                        validate_content(
                            key.as_slice().try_into().map_err(|_| Error::Corrupt)?,
                            value,
                            self,
                        )?;
                    } else if table != Table::Claims {
                        let namespace: Particle =
                            key[..32].try_into().map_err(|_| Error::Corrupt)?;
                        let head = match table {
                            Table::Heads => decode_head(value)?,
                            Table::Requests => {
                                let head = decode_request(value)?.1;
                                if !receipts.insert((namespace, head.index)) {
                                    return Err(Error::Corrupt);
                                }
                                head
                            }
                            Table::History => {
                                let index = u64::from_be_bytes(
                                    key[32..].try_into().map_err(|_| Error::Corrupt)?,
                                );
                                let expected = match previous {
                                    Some((ns, n)) if ns == namespace => {
                                        n.checked_add(1).ok_or(Error::Corrupt)?
                                    }
                                    _ => 0,
                                };
                                if index != expected || !receipts.remove(&(namespace, index)) {
                                    return Err(Error::Corrupt);
                                }
                                previous = Some((namespace, index));
                                Head {
                                    index,
                                    commit: value
                                        .as_slice()
                                        .try_into()
                                        .map_err(|_| Error::Corrupt)?,
                                }
                            }
                            _ => return Err(Error::Corrupt),
                        };
                        let selected = self.store.head(&namespace)?.ok_or(Error::Corrupt)?;
                        if head.index > selected.index
                            || self
                                .store
                                .db
                                .read(
                                    Table::History,
                                    &super::history_key(&namespace, head.index),
                                    32,
                                )?
                                .as_deref()
                                != Some(head.commit.as_slice())
                            || self
                                .store
                                .content(&head.commit, super::MAX_BYTES_VALUE)?
                                .is_none()
                        {
                            return Err(Error::Corrupt);
                        }
                    }
                }
                after = rows.last().map(|(key, _)| key.clone());
            }
        }
        if !receipts.is_empty() || self.store.db.last_transaction()? != self.transaction {
            return Err(Error::Corrupt);
        }
        result.digest = *hash.finalize().as_bytes();
        Ok(result)
    }
}
