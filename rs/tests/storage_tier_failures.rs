#![cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]

use bbg::storage::{MemStore, ShardStore, StorageError, StorageResult, TieredStore};
use nebu::Goldilocks;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct StorePath(PathBuf);
impl StorePath {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-tier-failure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for StorePath {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Successful HOT staging followed by an error clearing its dirty state.
struct FailingHot(MemStore);
impl ShardStore for FailingHot {
    fn get(&self, d: u8, key: &[u8; 32]) -> Option<&[Goldilocks]> {
        self.0.get(d, key)
    }
    fn put(&mut self, d: u8, key: [u8; 32], value: Vec<Goldilocks>) -> StorageResult<()> {
        self.0.put(d, key, value)
    }
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)] {
        self.0.dirty_entries()
    }
    fn commit(&mut self) -> StorageResult<[u8; 32]> {
        Err(StorageError::Io("injected HOT commit failure".into()))
    }
    fn get_mut(&mut self, d: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]> {
        self.0.get_mut(d, key)
    }
    fn mark_dirty(&mut self, d: u8, key: [u8; 32]) -> StorageResult<()> {
        self.0.mark_dirty(d, key)
    }
    fn remove(&mut self, d: u8, key: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>> {
        self.0.remove(d, key)
    }
    fn iter(&self, d: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_> {
        self.0.iter(d)
    }
}

fn open(backend: &str, path: &Path) -> Box<dyn ShardStore> {
    match backend {
        #[cfg(feature = "backend-ssd")]
        "fjall" => Box::new(bbg::storage::FjallStore::open(path.join("fjall")).unwrap()),
        #[cfg(feature = "backend-hdd")]
        "redb" => Box::new(bbg::storage::RedbStore::open(path.join("redb")).unwrap()),
        _ => unreachable!(),
    }
}

fn warm_commit_survives_hot_failure(backend: &str) {
    let path = StorePath::new();
    let error;
    {
        let mut warm = open(backend, &path.0);
        warm.put(0, [1; 32], vec![Goldilocks::new(10)]).unwrap();
        warm.put(1, [2; 32], vec![Goldilocks::new(20)]).unwrap();
        warm.commit().unwrap();

        let mut store = TieredStore::new(Box::new(FailingHot(MemStore::new())))
            .with_warm(warm)
            .unwrap();
        store.put(0, [1; 32], vec![Goldilocks::new(99)]).unwrap();
        assert_eq!(
            store.remove(1, &[2; 32]).unwrap(),
            Some(vec![Goldilocks::new(20)])
        );
        error = store.commit().unwrap_err();
        assert!(matches!(error, StorageError::CommitUnknown { .. }));
        assert!(store.is_poisoned());
        assert_eq!(store.dirty_entries().len(), 1);
        assert_eq!(store.commit(), Err(error.clone()));
        assert_eq!(store.read(0, &[1; 32], 1), Err(error.clone()));
        assert_eq!(
            store.put(0, [3; 32], vec![Goldilocks::new(30)]),
            Err(error.clone())
        );
    }

    let warm = open(backend, &path.0);
    let StorageError::CommitUnknown { change_id, .. } = error else {
        unreachable!()
    };
    assert_eq!(warm.last_commit().unwrap(), Some(change_id));
    assert_eq!(
        warm.read(0, &[1; 32], 1).unwrap(),
        Some(vec![Goldilocks::new(99)])
    );
    assert_eq!(warm.read(1, &[2; 32], 1).unwrap(), None);
    assert_eq!(warm.read(0, &[3; 32], 1).unwrap(), None);
}

#[cfg(feature = "backend-ssd")]
#[test]
fn fjall_warm_success_hot_failure_preserves_unknown_commit_identity() {
    warm_commit_survives_hot_failure("fjall");
}

#[cfg(feature = "backend-hdd")]
#[test]
fn redb_warm_success_hot_failure_preserves_unknown_commit_identity() {
    warm_commit_survives_hot_failure("redb");
}
