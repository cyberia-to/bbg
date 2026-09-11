//! Atomic local graph application history and content. See application-storage.md.
use std::fmt;
use std::fs::{File, OpenOptions};
use std::path::Path;

use ::redb::{Database, Durability, ReadableTable, TableDefinition};

pub type Particle = [u8; 32];
type BytesTable = TableDefinition<'static, &'static [u8], &'static [u8]>;
const CONTENT: BytesTable = TableDefinition::new("application_content");
const HEADS: BytesTable = TableDefinition::new("application_heads");
const HISTORY: BytesTable = TableDefinition::new("application_history");
const REQUESTS: BytesTable = TableDefinition::new("application_requests");
const CLAIMS: BytesTable = TableDefinition::new("application_unique_claims");

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
fn storage(e: impl fmt::Display) -> Error {
    Error::Storage(e.to_string())
}

#[derive(Debug)]
pub struct Write<'a> {
    pub namespace: Particle,
    pub request: Particle,
    pub fingerprint: Particle,
    pub expected: Option<Head>,
    pub head: Head,
    pub content: &'a [(Particle, Vec<u8>)],
    /// Store-wide immutable key/value assignments, checked in the same transaction.
    pub claims: &'a [(Particle, Particle)],
}

pub struct ApplicationStore {
    db: Database,
}

impl ApplicationStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path).map_err(storage)?;
        let db = Database::builder().create_file(file).map_err(storage)?;
        let mut tx = db.begin_write().map_err(storage)?;
        tx.set_durability(Durability::Immediate);
        for definition in [CONTENT, HEADS, HISTORY, REQUESTS, CLAIMS] {
            tx.open_table(definition).map_err(storage)?;
        }
        tx.commit()
            .map_err(|e| Error::CommitUnknown(e.to_string()))?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(storage)?;
        Ok(Self { db })
    }

    pub fn head(&self, namespace: &Particle) -> Result<Option<Head>, Error> {
        self.read(HEADS, namespace, 40)?
            .map(|b| decode_head(&b))
            .transpose()
    }

    pub fn content(&self, id: &Particle, max_bytes: usize) -> Result<Option<Vec<u8>>, Error> {
        self.read(CONTENT, id, max_bytes)
    }

    pub fn resolve(
        &self,
        namespace: &Particle,
        request: &Particle,
    ) -> Result<Option<(Particle, Head)>, Error> {
        self.read(REQUESTS, &request_key(namespace, request), 72)?
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
        let start = match after {
            Some(u64::MAX) => return Ok(Vec::new()),
            Some(n) => n + 1,
            None => 0,
        };
        let tx = self.db.begin_read().map_err(storage)?;
        let table = tx.open_table(HISTORY).map_err(storage)?;
        let lower = history_key(namespace, start);
        let upper = history_key(namespace, u64::MAX);
        let mut result = Vec::new();
        for row in table
            .range(lower.as_slice()..=upper.as_slice())
            .map_err(storage)?
            .take(limit)
        {
            let (key, value) = row.map_err(storage)?;
            let index = u64::from_be_bytes(
                key.value()
                    .get(32..40)
                    .ok_or(Error::Corrupt)?
                    .try_into()
                    .map_err(|_| Error::Corrupt)?,
            );
            let commit = value.value().try_into().map_err(|_| Error::Corrupt)?;
            result.push(Head { index, commit });
        }
        Ok(result)
    }

    pub fn apply(&self, write: &Write<'_>) -> Result<Head, Error> {
        if write.content.len() > 131_072 || write.claims.len() > 4096 {
            return Err(Error::Limit);
        }
        let total = write.content.iter().try_fold(0usize, |n, (_, bytes)| {
            n.checked_add(bytes.len()).ok_or(Error::Limit)
        })?;
        if total > 16 * 1024 * 1024 {
            return Err(Error::Limit);
        }
        let mut tx = self.db.begin_write().map_err(storage)?;
        tx.set_durability(Durability::Immediate);
        {
            let mut requests = tx.open_table(REQUESTS).map_err(storage)?;
            let request_key = request_key(&write.namespace, &write.request);
            if let Some(prior) = requests.get(request_key.as_slice()).map_err(storage)? {
                let (fingerprint, head) = decode_request(prior.value())?;
                return if fingerprint == write.fingerprint {
                    Ok(head)
                } else {
                    Err(Error::Conflict)
                };
            }
            let mut heads = tx.open_table(HEADS).map_err(storage)?;
            let current = heads
                .get(write.namespace.as_slice())
                .map_err(storage)?
                .map(|v| decode_head(v.value()))
                .transpose()?;
            if current != write.expected {
                return Err(Error::HeadMismatch);
            }
            let index = match current {
                Some(h) => h.index.checked_add(1).ok_or(Error::InvalidSequence)?,
                None => 0,
            };
            if write.head.index != index {
                return Err(Error::InvalidSequence);
            }
            let mut claims = tx.open_table(CLAIMS).map_err(storage)?;
            for (key, value) in write.claims {
                let prior = claims.get(key.as_slice()).map_err(storage)?;
                if prior
                    .as_ref()
                    .is_some_and(|p| p.value() != value.as_slice())
                {
                    return Err(Error::Conflict);
                }
                drop(prior);
                claims
                    .insert(key.as_slice(), value.as_slice())
                    .map_err(storage)?;
            }
            let mut content = tx.open_table(CONTENT).map_err(storage)?;
            for (id, bytes) in write.content {
                let present = {
                    let previous = content.get(id.as_slice()).map_err(storage)?;
                    if let Some(previous) = previous {
                        if previous.value() != bytes.as_slice() {
                            return Err(Error::Conflict);
                        }
                        true
                    } else {
                        false
                    }
                };
                if !present {
                    content
                        .insert(id.as_slice(), bytes.as_slice())
                        .map_err(storage)?;
                }
            }
            if content
                .get(write.head.commit.as_slice())
                .map_err(storage)?
                .is_none()
            {
                return Err(Error::Corrupt);
            }
            let mut history = tx.open_table(HISTORY).map_err(storage)?;
            history
                .insert(
                    history_key(&write.namespace, index).as_slice(),
                    write.head.commit.as_slice(),
                )
                .map_err(storage)?;
            let head_bytes = encode_head(write.head);
            heads
                .insert(write.namespace.as_slice(), head_bytes.as_slice())
                .map_err(storage)?;
            let mut receipt = [0; 72];
            receipt[..32].copy_from_slice(&write.fingerprint);
            receipt[32..].copy_from_slice(&head_bytes);
            requests
                .insert(request_key.as_slice(), receipt.as_slice())
                .map_err(storage)?;
        }
        tx.commit()
            .map_err(|e| Error::CommitUnknown(e.to_string()))?;
        Ok(write.head)
    }

    fn read(
        &self,
        definition: BytesTable,
        key: &[u8],
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Error> {
        let tx = self.db.begin_read().map_err(storage)?;
        let table = tx.open_table(definition).map_err(storage)?;
        table
            .get(key)
            .map_err(storage)?
            .map(|v| {
                if v.value().len() > max_bytes {
                    Err(Error::Limit)
                } else {
                    Ok(v.value().to_vec())
                }
            })
            .transpose()
    }
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
