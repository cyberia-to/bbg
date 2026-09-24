//! Bounded, resumable content storage under the shared Database owner.
mod codec;
mod read;
#[cfg(test)]
mod tests;
mod verify;
use super::StorageError;
use super::database::{ByteLimits, Database, Table, Transaction};
use codec::*;
pub use verify::{Verification, Verifier};

pub type Particle = [u8; 32];
/// A single part/read budget; a file may contain any number of parts.
pub const MAX_PART_BYTES: usize = 1024 * 1024;
pub const MAX_PAGE_ENTRIES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Storage(StorageError),
    Conflict,
    Missing,
    Incomplete,
    IdentityMismatch,
    ProfileMismatch,
    Cancelled,
    InvalidRange,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "content storage: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<StorageError> for Error {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub particle: Particle,
    pub profile: Particle,
    pub length: u64,
    pub part_bytes: u32,
}
impl Spec {
    pub fn validate(self) -> Result<()> {
        if self.part_bytes == 0 || self.part_bytes as usize > MAX_PART_BYTES {
            return Err(StorageError::Limit("content part budget").into());
        }
        Ok(())
    }
    pub fn parts(self) -> u64 {
        // Division is reached only for validated specs from begin/decode.
        let size = u64::from(self.part_bytes);
        if size == 0 {
            return 0;
        }
        self.length / size + u64::from(self.length % size != 0)
    }
    fn part_len(self, index: u64) -> Result<usize> {
        self.validate()?;
        if index >= self.parts() {
            return Err(Error::InvalidRange);
        }
        Ok(
            (self.length - index * u64::from(self.part_bytes)).min(u64::from(self.part_bytes))
                as usize,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upload {
    pub namespace: Particle,
    pub request: Particle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Staging,
    Sealed,
    Cancelled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub spec: Spec,
    pub present_parts: u64,
    pub state: State,
    pub reclaimed_through: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    pub spec: Spec,
    pub upload: Upload,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Coverage {
    /// Durable arrivals, not independently authenticated ranges.
    pub present: Vec<(u64, bool)>,
    pub next: Option<u64>,
}

#[derive(Clone)]
pub struct ContentStore {
    db: Database,
}
impl ContentStore {
    pub fn from_database(db: Database) -> Self {
        Self { db }
    }

    pub fn begin(&self, upload: Upload, spec: Spec) -> Result<Progress> {
        spec.validate()?;
        self.db
            .transaction::<_, Error>(|tx| {
                writable(tx, &upload.namespace)?;
                if let Some(prior) = progress(tx, upload)? {
                    if prior.spec != spec {
                        return Err(Error::Conflict);
                    }
                    if prior.state == State::Cancelled {
                        return Err(Error::Cancelled);
                    }
                    return Ok(prior);
                }
                let value = Progress {
                    spec,
                    present_parts: 0,
                    state: State::Staging,
                    reclaimed_through: 0,
                };
                put_progress(tx, upload, value)?;
                Ok(value)
            })
            .map(|commit| commit.value)
    }

    pub fn progress(&self, upload: Upload) -> Result<Option<Progress>> {
        self.db
            .transaction::<_, Error>(|tx| progress(tx, upload))
            .map(|commit| commit.value)
    }

    pub fn write_part(&self, upload: Upload, index: u64, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_PART_BYTES {
            return Err(StorageError::Limit("content part bytes").into());
        }
        let checksum = hemera::hash(bytes);
        let key = part_key(upload, index);
        self.db
            .transaction::<_, Error>(|tx| {
                writable(tx, &upload.namespace)?;
                let mut current = progress(tx, upload)?.ok_or(Error::Missing)?;
                if current.state == State::Cancelled {
                    return Err(Error::Cancelled);
                }
                if bytes.len() != current.spec.part_len(index)? {
                    return Err(Error::InvalidRange);
                }
                if let Some(existing) = tx.get(Table::Parts, &key, MAX_PART_BYTES)? {
                    if existing != bytes {
                        return Err(Error::Conflict);
                    }
                    if tx.get(Table::PartChecks, &key, 32)?.as_deref() != Some(checksum.as_bytes())
                    {
                        return Err(StorageError::Corrupt("content part checksum").into());
                    }
                    return Ok(());
                }
                if current.state != State::Staging {
                    return Err(Error::Conflict);
                }
                if tx.get(Table::PartChecks, &key, 32)?.is_some() {
                    return Err(StorageError::Corrupt("content coverage without bytes").into());
                }
                current.present_parts = current
                    .present_parts
                    .checked_add(1)
                    .filter(|n| *n <= current.spec.parts())
                    .ok_or(StorageError::Corrupt("content part count"))?;
                tx.put(Table::Parts, &key, bytes)?;
                tx.put(Table::PartChecks, &key, checksum.as_bytes())?;
                put_progress(tx, upload, current)?;
                Ok(())
            })
            .map(|_| ())
    }

    /// Freeze new arrivals by requiring complete immutable parts. Dropping this
    /// session preserves all persisted parts; a new session rehashes from zero.
    pub fn verify<V: Verifier>(&self, upload: Upload, verifier: V) -> Result<Verification<V>> {
        let current = self.progress(upload)?.ok_or(Error::Missing)?;
        if current.state == State::Cancelled {
            return Err(Error::Cancelled);
        }
        if current.spec.profile != verifier.profile() {
            return Err(Error::ProfileMismatch);
        }
        if current.present_parts != current.spec.parts() {
            return Err(Error::Incomplete);
        }
        Ok(Verification::new(
            self.clone(),
            upload,
            current.spec,
            verifier,
        ))
    }

    pub fn file(&self, namespace: Particle, particle: Particle) -> Result<Option<FileInfo>> {
        self.db
            .transaction::<_, Error>(|tx| file(tx, namespace, particle))
            .map(|c| c.value)
    }

    /// Cancel and reclaim at most `limit` stored parts. The request tombstone
    /// prevents a delayed sender or verifier from reusing this upload identity.
    pub fn cancel(&self, upload: Upload, limit: usize) -> Result<bool> {
        page_limit(limit)?;
        self.db
            .transaction::<_, Error>(|tx| {
                writable(tx, &upload.namespace)?;
                let mut current = progress(tx, upload)?.ok_or(Error::Missing)?;
                if file(tx, upload.namespace, current.spec.particle)?
                    .is_some_and(|f| f.upload == upload)
                {
                    return Err(Error::Conflict);
                }
                let first = part_key(upload, 0);
                let after = current
                    .reclaimed_through
                    .checked_sub(1)
                    .map(|n| part_key(upload, n));
                let rows = tx.scan(
                    Table::PartChecks,
                    after.as_ref().map(|k| k.as_slice()),
                    &first[..32],
                    ByteLimits {
                        max_entries: limit,
                        max_bytes: limit * 72,
                    },
                )?;
                let exhausted = rows.len() < limit;
                current.state = State::Cancelled;
                for (key, checksum) in rows {
                    if key.len() != 40 || checksum.len() != 32 {
                        return Err(StorageError::Corrupt("cancelled content part record").into());
                    }
                    let index = u64::from_be_bytes(key[32..].try_into().unwrap());
                    if index < current.reclaimed_through || index >= current.spec.parts() {
                        return Err(StorageError::Corrupt("cancelled content part index").into());
                    }
                    tx.remove(Table::Parts, &key)?;
                    tx.remove(Table::PartChecks, &key)?;
                    current.reclaimed_through = index + 1;
                    current.present_parts = current
                        .present_parts
                        .checked_sub(1)
                        .ok_or(StorageError::Corrupt("cancelled content coverage"))?;
                }
                if exhausted {
                    if current.present_parts != 0 {
                        return Err(
                            StorageError::Corrupt("cancelled content missing coverage").into()
                        );
                    }
                    current.reclaimed_through = current.spec.parts();
                }
                put_progress(tx, upload, current)?;
                Ok(current.reclaimed_through == current.spec.parts())
            })
            .map(|c| c.value)
    }
}

impl Transaction<'_> {
    /// Retain a sealed file in the same transaction as an application head.
    /// Authorization and the complete request fingerprint belong to the caller.
    pub fn retain_content(
        &mut self,
        namespace: Particle,
        particle: Particle,
        profile: Particle,
        root: Particle,
    ) -> Result<FileInfo> {
        writable(self, &namespace)?;
        let info = file(self, namespace, particle)?.ok_or(Error::Missing)?;
        if info.spec.profile != profile {
            return Err(Error::ProfileMismatch);
        }
        let key = retention_key(namespace, root, particle);
        if let Some(old) = self.get(Table::RetainedContent, &key, 32)? {
            if old != profile {
                return Err(Error::Conflict);
            }
        } else {
            self.put(Table::RetainedContent, &key, &profile)?;
        }
        Ok(info)
    }
}

fn page_limit(limit: usize) -> Result<()> {
    if limit == 0 || limit > MAX_PAGE_ENTRIES {
        return Err(StorageError::Limit("content page budget").into());
    }
    Ok(())
}
fn writable(tx: &Transaction<'_>, namespace: &Particle) -> Result<()> {
    if tx.get(Table::Migration, b"status", 16)?.as_deref() == Some(b"export-v1")
        || tx
            .get(
                Table::Migration,
                &super::application::transfer_stage_key(namespace),
                32,
            )?
            .is_some()
        || tx
            .get(
                Table::Migration,
                &super::application::fence_key(namespace),
                64,
            )?
            .is_some()
    {
        return Err(Error::Conflict);
    }
    Ok(())
}
