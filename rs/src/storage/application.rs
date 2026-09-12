//! Application history and retry semantics over the selected BBG owner.
use super::StorageError;
use super::database::{Backend, ByteLimits, Database, MAX_BYTES_VALUE, Table, Transaction};
use std::{fmt, path::Path};

#[cfg(all(feature = "backend-ssd", feature = "backend-hdd"))]
mod migration;

pub type Particle = [u8; 32];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    pub index: u64,
    pub commit: Particle,
}

#[derive(Debug)]
pub enum Error {
    Storage(String),
    CommitUnknown(String),
    Conflict,
    HeadMismatch,
    InvalidSequence,
    Limit,
    Corrupt,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "application storage: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<StorageError> for Error {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::CommitUnknown { message, .. } => Self::CommitUnknown(message),
            StorageError::Limit(_) => Self::Limit,
            StorageError::Corrupt(_) => Self::Corrupt,
            error => Self::Storage(error.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct Write<'a> {
    pub namespace: Particle,
    pub request: Particle,
    /// Must bind all inputs to the application and optional shard transition.
    pub fingerprint: Particle,
    pub expected: Option<Head>,
    pub head: Head,
    pub content: &'a [(Particle, Vec<u8>)],
    pub claims: &'a [(Particle, Particle)],
}

pub struct ApplicationStore {
    db: Database,
}
impl ApplicationStore {
    /// Default working profile: Fjall on SSD, stored in a directory.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self::from_database(Database::open(path, Backend::Ssd)?))
    }
    pub fn from_database(db: Database) -> Self {
        Self { db }
    }
    pub fn database(&self) -> Database {
        self.db.clone()
    }

    #[cfg(all(feature = "backend-ssd", feature = "backend-hdd"))]
    pub fn migrate_redb(
        source: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<(), Error> {
        migration::migrate(source.as_ref(), destination.as_ref())
    }

    pub fn head(&self, namespace: &Particle) -> Result<Option<Head>, Error> {
        self.db
            .read(Table::Heads, namespace, 40)?
            .map(|b| decode_head(&b))
            .transpose()
    }
    pub fn content(&self, id: &Particle, max_bytes: usize) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.db.read(Table::Content, id, max_bytes)?)
    }
    pub fn resolve(
        &self,
        namespace: &Particle,
        request: &Particle,
    ) -> Result<Option<(Particle, Head)>, Error> {
        self.db
            .read(Table::Requests, &request_key(namespace, request), 72)?
            .map(|b| decode_request(&b))
            .transpose()
    }
    pub fn history(
        &self,
        namespace: &Particle,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<Head>, Error> {
        if limit == 0 || limit > 4096 {
            return Err(Error::Limit);
        }
        if after == Some(u64::MAX) {
            return Ok(Vec::new());
        }
        let after = after.map(|n| history_key(namespace, n));
        self.db
            .scan(
                Table::History,
                after.as_ref().map(|k| k.as_slice()),
                namespace,
                ByteLimits {
                    max_entries: limit,
                    max_bytes: limit * 72,
                },
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() != 40 || &key[..32] != namespace {
                    return Err(Error::Corrupt);
                }
                Ok(Head {
                    index: u64::from_be_bytes(key[32..].try_into().map_err(|_| Error::Corrupt)?),
                    commit: value.as_slice().try_into().map_err(|_| Error::Corrupt)?,
                })
            })
            .collect()
    }

    pub fn apply(&self, write: &Write<'_>) -> Result<Head, Error> {
        self.apply_with(write, |_| Ok(()))
    }

    /// The closure executes once for a new request, inside the application's
    /// transaction. It must use only tx for this owner's reads and mutations.
    pub fn apply_with(
        &self,
        write: &Write<'_>,
        transition: impl FnOnce(&mut Transaction<'_>) -> Result<(), Error>,
    ) -> Result<Head, Error> {
        validate_write(write)?;
        self.db
            .transaction::<_, Error>(|tx| {
                let receipt_key = request_key(&write.namespace, &write.request);
                if let Some(prior) = tx.get(Table::Requests, &receipt_key, 72)? {
                    let (fingerprint, head) = decode_request(&prior)?;
                    return if fingerprint == write.fingerprint {
                        Ok(head)
                    } else {
                        Err(Error::Conflict)
                    };
                }
                let current = tx
                    .get(Table::Heads, &write.namespace, 40)?
                    .map(|bytes| decode_head(&bytes))
                    .transpose()?;
                if current != write.expected {
                    return Err(Error::HeadMismatch);
                }
                let next = match current {
                    Some(head) => head.index.checked_add(1).ok_or(Error::InvalidSequence)?,
                    None => 0,
                };
                if write.head.index != next {
                    return Err(Error::InvalidSequence);
                }

                for (key, value) in write.claims {
                    immutable_put(tx, Table::Claims, key, value, 32)?;
                }
                for (id, bytes) in write.content {
                    immutable_put(tx, Table::Content, id, bytes, MAX_BYTES_VALUE)?;
                }
                if tx
                    .get(Table::Content, &write.head.commit, MAX_BYTES_VALUE)?
                    .is_none()
                {
                    return Err(Error::Corrupt);
                }
                transition(tx)?;
                tx.put(
                    Table::History,
                    &history_key(&write.namespace, next),
                    &write.head.commit,
                )?;
                tx.put(Table::Heads, &write.namespace, &encode_head(write.head))?;
                let mut receipt = [0; 72];
                receipt[..32].copy_from_slice(&write.fingerprint);
                receipt[32..].copy_from_slice(&encode_head(write.head));
                tx.put(Table::Requests, &receipt_key, &receipt)?;
                Ok(write.head)
            })
            .map(|commit| commit.value)
    }
}

fn validate_write(write: &Write<'_>) -> Result<(), Error> {
    if write.content.len() > 131_072 || write.claims.len() > 4096 {
        return Err(Error::Limit);
    }
    let total = write.content.iter().try_fold(0usize, |n, (_, bytes)| {
        n.checked_add(bytes.len()).ok_or(Error::Limit)
    })?;
    if total > 16 * 1024 * 1024 {
        return Err(Error::Limit);
    }
    Ok(())
}
fn immutable_put(
    tx: &mut Transaction<'_>,
    table: Table,
    key: &[u8],
    value: &[u8],
    max: usize,
) -> Result<(), Error> {
    if let Some(previous) = tx.get(table, key, max)? {
        if previous != value {
            return Err(Error::Conflict);
        }
    } else {
        tx.put(table, key, value)?;
    }
    Ok(())
}

fn request_key(namespace: &Particle, request: &Particle) -> [u8; 64] {
    let mut key = [0; 64];
    key[..32].copy_from_slice(namespace);
    key[32..].copy_from_slice(request);
    key
}
fn history_key(namespace: &Particle, index: u64) -> [u8; 40] {
    let mut key = [0; 40];
    key[..32].copy_from_slice(namespace);
    key[32..].copy_from_slice(&index.to_be_bytes());
    key
}
fn encode_head(head: Head) -> [u8; 40] {
    let mut bytes = [0; 40];
    bytes[..8].copy_from_slice(&head.index.to_le_bytes());
    bytes[8..].copy_from_slice(&head.commit);
    bytes
}
fn decode_head(bytes: &[u8]) -> Result<Head, Error> {
    if bytes.len() != 40 {
        return Err(Error::Corrupt);
    }
    Ok(Head {
        index: u64::from_le_bytes(bytes[..8].try_into().map_err(|_| Error::Corrupt)?),
        commit: bytes[8..].try_into().map_err(|_| Error::Corrupt)?,
    })
}
fn decode_request(bytes: &[u8]) -> Result<(Particle, Head), Error> {
    if bytes.len() != 72 {
        return Err(Error::Corrupt);
    }
    Ok((
        bytes[..32].try_into().map_err(|_| Error::Corrupt)?,
        decode_head(&bytes[32..])?,
    ))
}
