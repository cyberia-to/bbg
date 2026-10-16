//! Restartable transfer of a sealed application-only store into a shared owner.
#[cfg(all(test, feature = "backend-ssd"))]
mod tests;
use super::{ApplicationStore, Error, Head, Particle, immutable_put, validation};
use crate::storage::database::{ByteLimits, MAX_TRANSACTION_BYTES, Table, Transaction};
use std::path::Path;

use super::archive::{ApplicationArchive, SEAL, TABLES};
pub struct TransferSource {
    store: ApplicationStore,
    manifest: Particle,
    target: Particle,
    sources: Vec<(Particle, Head)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    pub manifest: Particle,
    pub rows: u64,
    pub bytes: u64,
    pub complete: bool,
}
impl TransferSource {
    /// Open an existing SSD source under its exclusive backend lock. A fresh
    /// seal or an exact resume is permitted; no normal writer is returned.
    pub fn open(path: &Path, target: Particle, key: Particle) -> Result<Self, Error> {
        Self::from_archive(ApplicationArchive::open(path)?, target, key)
    }
    /// Preserve the inspected source lock while establishing its exact seal.
    pub fn from_archive(
        archive: ApplicationArchive,
        target: Particle,
        key: Particle,
    ) -> Result<Self, Error> {
        let store = archive.store;
        let sources = archive.sources;
        let status = store.db.read(Table::Migration, b"status", 16)?;
        if sources.iter().any(|(id, _)| *id == target) {
            return Err(Error::Conflict);
        }
        let seal = if let Some(seal) = store.db.read(Table::Migration, SEAL, 128)? {
            if seal.len() != 128 || seal[..32] != target || seal[32..64] != key {
                return Err(Error::Conflict);
            }
            seal
        } else {
            if status.as_deref() == Some(b"export-v1") {
                return Err(Error::Corrupt);
            }
            let prior = store.db.last_transaction()?.unwrap_or([0; 32]);
            let mut hash = hemera::Hasher::new();
            hash.update(b"bbg/application-transfer/1\0");
            hash.update(&target);
            hash.update(&key);
            hash.update(&prior);
            for (id, h) in &sources {
                hash.update(id);
                hash.update(&h.index.to_le_bytes());
                hash.update(&h.commit);
            }
            let manifest = *hash.finalize().as_bytes();
            let seal = [
                target.as_slice(),
                key.as_slice(),
                prior.as_slice(),
                manifest.as_slice(),
            ]
            .concat();
            store.db.transaction::<_, Error>(|tx| {
                for (id, h) in &sources {
                    if tx.get(Table::Heads, id, 40)?.as_deref()
                        != Some(super::encode_head(*h).as_slice())
                    {
                        return Err(Error::Conflict);
                    }
                }
                immutable_put(tx, Table::Migration, SEAL, &seal, 128)?;
                tx.put(Table::Migration, b"status", b"export-v1")?;
                Ok(())
            })?;
            seal
        };
        let manifest = seal[96..].try_into().map_err(|_| Error::Corrupt)?;
        let mut hash = hemera::Hasher::new();
        hash.update(b"bbg/application-transfer/1\0");
        hash.update(&seal[..96]);
        for (id, h) in &sources {
            hash.update(id);
            hash.update(&h.index.to_le_bytes());
            hash.update(&h.commit);
        }
        if *hash.finalize().as_bytes() != manifest {
            return Err(Error::Corrupt);
        }
        Ok(Self {
            store,
            manifest,
            target,
            sources,
        })
    }
    pub fn manifest(&self) -> Particle {
        self.manifest
    }
    pub fn sources(&self) -> &[(Particle, Head)] {
        &self.sources
    }
    pub fn store(&self) -> &ApplicationStore {
        &self.store
    }

    /// Copy at most `pages` atomic batches. Content validation is supplied by
    /// the graph owner. Completion validates all referenced archive content.
    pub fn stage(
        &self,
        target: &ApplicationStore,
        pages: usize,
        validate_content: impl Fn(Particle, &[u8], &ApplicationStore) -> Result<(), Error>,
    ) -> Result<TransferProgress, Error> {
        if pages == 0 || pages > 4096 {
            return Err(Error::Limit);
        }
        self.prepare_target(target)?;
        let key = cursor_key(&self.manifest);
        let mut cursor = Cursor::decode(
            &target
                .db
                .read(Table::Migration, &key, 82)?
                .ok_or(Error::Corrupt)?,
        )?;
        for _ in 0..pages {
            if cursor.table == 5 {
                break;
            }
            let table = TABLES[cursor.table as usize];
            let rows = self
                .store
                .db
                .scan(table, cursor.after.as_deref(), &[], limits())?;
            let mut next = cursor.clone();
            if rows.is_empty() {
                next.table += 1;
                next.after = None;
                if next.table == 5 {
                    // Source is sealed and every target row was immutably copied.
                    validation::validate_links(&self.store.db)?;
                }
            } else {
                for (key, value) in &rows {
                    validation::validate_record(table, key, value)?;
                    if table == Table::Content {
                        validate_content(
                            key.as_slice().try_into().map_err(|_| Error::Corrupt)?,
                            value,
                            &self.store,
                        )?;
                    }
                    next.rows = next.rows.checked_add(1).ok_or(Error::Limit)?;
                    next.bytes = next
                        .bytes
                        .checked_add((key.len() + value.len()) as u64)
                        .ok_or(Error::Limit)?;
                }
                next.after = rows.last().map(|(k, _)| k.clone());
            }
            target.db.transaction::<_, Error>(|tx| {
                if tx.get(Table::Migration, &key, 82)?.as_deref()
                    != Some(cursor.encode().as_slice())
                {
                    return Err(Error::Conflict);
                }
                for (key, value) in &rows {
                    immutable_put(tx, table, key, value, super::MAX_BYTES_VALUE)?;
                }
                tx.put(Table::Migration, &key, &next.encode())?;
                Ok(())
            })?;
            cursor = next;
        }
        Ok(TransferProgress {
            manifest: self.manifest,
            rows: cursor.rows,
            bytes: cursor.bytes,
            complete: cursor.table == 5,
        })
    }
    fn prepare_target(&self, target: &ApplicationStore) -> Result<(), Error> {
        target.db.transaction::<_, Error>(|tx| {
            let key = cursor_key(&self.manifest);
            if tx.get(Table::Migration, &key, 82)?.is_some() {
                return Ok(());
            }
            crate::storage::database::promote_generation(
                tx,
                crate::storage::database::ReaderGeneration::NeuronV1,
            )?;
            for (id, _) in &self.sources {
                if *id == self.target
                    || tx.get(Table::Heads, id, 40)?.is_some()
                    || tx
                        .get(Table::Migration, &super::fence_key(id), 64)?
                        .is_some()
                    || tx.get(Table::Migration, &stage_key(id), 32)?.is_some()
                {
                    return Err(Error::Conflict);
                }
                tx.put(Table::Migration, &stage_key(id), &self.manifest)?;
            }
            tx.put(Table::Migration, &target_key(&self.manifest), &self.target)?;
            tx.put(
                Table::Migration,
                &key,
                &Cursor {
                    table: 0,
                    after: None,
                    rows: 0,
                    bytes: 0,
                }
                .encode(),
            )?;
            Ok(())
        })?;
        Ok(())
    }
}
fn limits() -> ByteLimits {
    ByteLimits {
        max_entries: 512,
        max_bytes: MAX_TRANSACTION_BYTES - 4096,
    }
}
pub(super) fn stage_key(id: &Particle) -> [u8; 33] {
    let mut k = [0; 33];
    k[0] = b's';
    k[1..].copy_from_slice(id);
    k
}
fn cursor_key(id: &Particle) -> [u8; 33] {
    let mut k = [0; 33];
    k[0] = b't';
    k[1..].copy_from_slice(id);
    k
}
fn target_key(id: &Particle) -> [u8; 33] {
    let mut k = [0; 33];
    k[0] = b'u';
    k[1..].copy_from_slice(id);
    k
}
pub(super) fn require_complete(
    tx: &mut Transaction<'_>,
    origin: &Particle,
    target: &Particle,
) -> Result<(), Error> {
    if let Some(manifest) = tx.get(Table::Migration, &stage_key(origin), 32)? {
        let manifest = manifest.as_slice().try_into().map_err(|_| Error::Corrupt)?;
        let cursor = tx
            .get(Table::Migration, &cursor_key(&manifest), 82)?
            .ok_or(Error::Corrupt)?;
        if Cursor::decode(&cursor)?.table != 5 {
            return Err(Error::Fenced);
        }
        if tx
            .get(Table::Migration, &target_key(&manifest), 32)?
            .as_deref()
            != Some(target.as_slice())
        {
            return Err(Error::Conflict);
        }
    }
    Ok(())
}
#[derive(Clone)]
struct Cursor {
    table: u8,
    after: Option<Vec<u8>>,
    rows: u64,
    bytes: u64,
}
impl Cursor {
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![self.table, self.after.as_ref().map_or(0, |a| a.len() as u8)];
        b.extend(self.rows.to_le_bytes());
        b.extend(self.bytes.to_le_bytes());
        if let Some(a) = &self.after {
            b.extend(a);
        }
        b
    }
    fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 18 || b[0] > 5 || b[1] > 64 || b.len() != 18 + b[1] as usize {
            return Err(Error::Corrupt);
        }
        Ok(Self {
            table: b[0],
            after: if b[1] == 0 {
                None
            } else {
                Some(b[18..].to_vec())
            },
            rows: u64::from_le_bytes(b[2..10].try_into().map_err(|_| Error::Corrupt)?),
            bytes: u64::from_le_bytes(b[10..18].try_into().map_err(|_| Error::Corrupt)?),
        })
    }
}
