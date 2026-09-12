//! Physical engines behind the shared transaction owner.

use super::{Backend, ByteLimits, Changes, RawEntry, Table};
use crate::storage::{StorageError, StorageResult};
use std::ops::Bound::{self, Excluded, Included, Unbounded};
use std::path::Path;

#[cfg(feature = "backend-ssd")]
#[path = "backend_fjall.rs"]
mod fjall;
#[cfg(feature = "backend-hdd")]
#[path = "backend_redb.rs"]
mod redb;

pub(super) enum Engine {
    #[cfg(feature = "backend-ssd")]
    Fjall(fjall::FjallEngine),
    #[cfg(feature = "backend-hdd")]
    Redb(redb::RedbEngine),
}

impl Engine {
    pub(super) fn open(path: &Path, backend: Backend) -> StorageResult<Self> {
        match backend {
            Backend::Ssd => {
                #[cfg(feature = "backend-ssd")]
                return fjall::FjallEngine::open(path).map(Self::Fjall);
                #[cfg(not(feature = "backend-ssd"))]
                Err(StorageError::Unsupported("SSD backend was not compiled"))
            }
            Backend::Hdd => {
                #[cfg(feature = "backend-hdd")]
                return redb::RedbEngine::open(path).map(Self::Redb);
                #[cfg(not(feature = "backend-hdd"))]
                Err(StorageError::Unsupported("HDD backend was not compiled"))
            }
        }
    }

    pub(super) fn get(
        &self,
        table: Table,
        key: &[u8],
        max_bytes: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        match self {
            #[cfg(feature = "backend-ssd")]
            Self::Fjall(engine) => engine.get(table, key, max_bytes),
            #[cfg(feature = "backend-hdd")]
            Self::Redb(engine) => engine.get(table, key, max_bytes),
        }
    }

    pub(super) fn scan(
        &self,
        table: Table,
        after: Option<&[u8]>,
        prefix: &[u8],
        limits: ByteLimits,
    ) -> StorageResult<Vec<RawEntry>> {
        match self {
            #[cfg(feature = "backend-ssd")]
            Self::Fjall(engine) => engine.scan(table, after, prefix, limits),
            #[cfg(feature = "backend-hdd")]
            Self::Redb(engine) => engine.scan(table, after, prefix, limits),
        }
    }

    pub(super) fn commit(&mut self, changes: &Changes, id: [u8; 32]) -> StorageResult<()> {
        match self {
            #[cfg(feature = "backend-ssd")]
            Self::Fjall(engine) => engine.commit(changes, id),
            #[cfg(feature = "backend-hdd")]
            Self::Redb(engine) => engine.commit(changes, id),
        }
    }

    #[cfg(all(test, feature = "backend-hdd"))]
    pub(super) fn from_redb(db: ::redb::Database) -> Self {
        Self::Redb(redb::RedbEngine::from_database(db))
    }
}

type ScanBounds = (Bound<Vec<u8>>, Bound<Vec<u8>>);

/// An empty prefix starts unbounded so malformed short keys remain visible.
fn scan_bounds(after: Option<&[u8]>, prefix: &[u8]) -> Option<ScanBounds> {
    let upper = prefix.iter().rposition(|byte| *byte != u8::MAX).map(|i| {
        let mut upper = prefix[..=i].to_vec();
        upper[i] += 1;
        upper
    });
    if upper
        .as_ref()
        .is_some_and(|upper| after.is_some_and(|after| after >= upper.as_slice()))
    {
        return None;
    }
    let start = match after {
        Some(after) if prefix.is_empty() || after >= prefix => Excluded(after.to_vec()),
        _ if !prefix.is_empty() => Included(prefix.to_vec()),
        _ => Unbounded,
    };
    Some((start, upper.map_or(Unbounded, Excluded)))
}

fn copy_value(bytes: &[u8], max_bytes: usize) -> StorageResult<Vec<u8>> {
    if bytes.len() > max_bytes {
        return Err(StorageError::Limit("database value exceeds read budget"));
    }
    Ok(bytes.to_vec())
}

/// Check borrowed bytes before allocating either owned half of a result row.
fn push_row(
    page: &mut Vec<RawEntry>,
    used: &mut usize,
    key: &[u8],
    value: &[u8],
    limits: ByteLimits,
) -> StorageResult<bool> {
    if key.len() > 64 {
        return Err(StorageError::Corrupt("database key length"));
    }
    if value.len() > 16 * 1024 * 1024 + 1 {
        return Err(StorageError::Limit("database value length"));
    }
    let bytes = key
        .len()
        .checked_add(value.len())
        .ok_or(StorageError::Limit("database row length"))?;
    if bytes > limits.max_bytes {
        return Err(StorageError::Limit("database row exceeds page budget"));
    }
    if bytes > limits.max_bytes - *used {
        return Ok(false);
    }
    page.push((key.to_vec(), value.to_vec()));
    *used += bytes;
    Ok(true)
}

fn unknown(id: [u8; 32], error: impl std::fmt::Display) -> StorageError {
    StorageError::CommitUnknown {
        change_id: id,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "bbg-engine-{}-{timestamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self, backend: Backend) -> PathBuf {
            self.0.join(match backend {
                Backend::Ssd => "ssd",
                Backend::Hdd => "hdd.redb",
            })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn backends() -> Vec<Backend> {
        vec![
            #[cfg(feature = "backend-ssd")]
            Backend::Ssd,
            #[cfg(feature = "backend-hdd")]
            Backend::Hdd,
        ]
    }

    fn limits(max_entries: usize, max_bytes: usize) -> ByteLimits {
        ByteLimits {
            max_entries,
            max_bytes,
        }
    }

    #[test]
    fn engine_reopens_changes_and_deletions_across_tables() {
        for backend in backends() {
            let fixture = Fixture::new();
            let path = fixture.path(backend);
            let mut engine = Engine::open(&path, backend).unwrap();
            assert_eq!(engine.get(Table::Content, b"content", 100).unwrap(), None);
            assert!(
                engine
                    .scan(Table::Heads, None, b"", limits(10, 100))
                    .unwrap()
                    .is_empty()
            );
            let mut changes = Changes::new();
            changes.insert((Table::Content, b"content".to_vec()), Some(b"old".to_vec()));
            changes.insert((Table::Heads, b"head".to_vec()), Some(b"head".to_vec()));
            changes.insert((Table::Shard(0), vec![1; 32]), Some(vec![0; 8]));
            engine.commit(&changes, [1; 32]).unwrap();
            changes.clear();
            changes.insert((Table::Content, b"content".to_vec()), Some(b"new".to_vec()));
            changes.insert((Table::Heads, b"head".to_vec()), None);
            changes.insert(
                (Table::Metadata, b"last_transaction".to_vec()),
                Some(vec![2; 32]),
            );
            engine.commit(&changes, [2; 32]).unwrap();
            drop(engine);
            let reopened = Engine::open(&path, backend).unwrap();
            assert_eq!(
                reopened.get(Table::Content, b"content", 3).unwrap(),
                Some(b"new".to_vec())
            );
            assert_eq!(reopened.get(Table::Heads, b"head", 100).unwrap(), None);
            assert_eq!(
                reopened.get(Table::Shard(0), &[1; 32], 8).unwrap(),
                Some(vec![0; 8])
            );
            assert_eq!(
                reopened
                    .get(Table::Metadata, b"last_transaction", 32)
                    .unwrap(),
                Some(vec![2; 32])
            );
        }
    }

    #[test]
    fn engine_scans_prefixes_and_exclusive_cursors_with_byte_limits() {
        for backend in backends() {
            let fixture = Fixture::new();
            let mut engine = Engine::open(&fixture.path(backend), backend).unwrap();
            let keys: &[&[u8]] = &[
                b"\0",
                b"a",
                b"ab",
                b"ab0",
                b"ab\xff",
                b"ac",
                b"\xff",
                b"\xff\xff",
            ];
            let changes = keys
                .iter()
                .map(|key| ((Table::Content, key.to_vec()), Some(vec![7; 4])))
                .collect();
            engine.commit(&changes, [1; 32]).unwrap();
            let page = |after, prefix| {
                engine
                    .scan(Table::Content, after, prefix, limits(100, 1000))
                    .unwrap()
            };
            assert_eq!(page(None, b"").first().unwrap().0, b"\0");
            assert_eq!(
                page(None, b"ab")
                    .into_iter()
                    .map(|(key, _)| key)
                    .collect::<Vec<_>>(),
                [b"ab".to_vec(), b"ab0".to_vec(), b"ab\xff".to_vec()]
            );
            assert_eq!(page(Some(b"ab"), b"ab").len(), 2);
            assert_eq!(page(Some(b"a"), b"ab").len(), 3);
            assert!(page(Some(b"ac"), b"ab").is_empty());
            assert_eq!(page(None, b"\xff").len(), 2);
            assert_eq!(page(Some(b"\xff"), b"\xff").len(), 1);
            assert_eq!(
                engine
                    .scan(Table::Content, None, b"ab", limits(1, 1000))
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                engine
                    .scan(Table::Content, None, b"ab", limits(100, 12))
                    .unwrap()
                    .len(),
                1
            );
            assert!(matches!(
                engine.scan(Table::Content, None, b"ab", limits(100, 5)),
                Err(StorageError::Limit(_))
            ));
            assert!(matches!(
                engine.get(Table::Content, b"ab", 3),
                Err(StorageError::Limit(_))
            ));
        }
    }

    #[test]
    fn engine_rejects_a_second_writer_and_creates_private_paths() {
        for backend in backends() {
            let fixture = Fixture::new();
            let path = fixture.path(backend);
            let engine = Engine::open(&path, backend).unwrap();
            assert!(matches!(
                Engine::open(&path, backend),
                Err(StorageError::Busy)
            ));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let expected = match backend {
                    Backend::Ssd => 0o700,
                    Backend::Hdd => 0o600,
                };
                assert_eq!(
                    std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    expected
                );
            }
            drop(engine);
            assert!(Engine::open(&path, backend).is_ok());
        }
    }

    #[test]
    fn engine_scan_rejects_oversized_keys_before_copying() {
        for backend in backends() {
            let fixture = Fixture::new();
            let mut engine = Engine::open(&fixture.path(backend), backend).unwrap();
            let changes = [((Table::Content, vec![1; 65]), Some(vec![2]))].into();
            engine.commit(&changes, [1; 32]).unwrap();
            assert!(matches!(
                engine.scan(Table::Content, None, b"", limits(100, 1000)),
                Err(StorageError::Corrupt("database key length"))
            ));
        }
    }
}
