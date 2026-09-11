use super::*;
use redb::{StorageBackend, backends::FileBackend};
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

#[derive(Debug)]
struct FaultFile {
    file: FileBackend,
    fault: Arc<AtomicU8>,
}

impl FaultFile {
    fn trip(&self, mode: u8) -> std::io::Result<()> {
        if self
            .fault
            .compare_exchange(mode, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Err(std::io::Error::other("injected storage I/O failure"));
        }
        Ok(())
    }
}

impl StorageBackend for FaultFile {
    fn len(&self) -> std::io::Result<u64> {
        self.file.len()
    }
    fn read(&self, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        self.trip(1)?;
        self.file.read(offset, len)
    }
    fn set_len(&self, len: u64) -> std::io::Result<()> {
        self.file.set_len(len)
    }
    fn write(&self, offset: u64, data: &[u8]) -> std::io::Result<()> {
        self.trip(2)?;
        self.file.write(offset, data)
    }
    fn sync_data(&self, eventual: bool) -> std::io::Result<()> {
        self.trip(3)?;
        self.file.sync_data(eventual)?;
        // Simulate a barrier completing on disk while its acknowledgement fails.
        self.trip(4)
    }
}

struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-redb-fault-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fault_store(path: &Path) -> (RedbStore, Arc<AtomicU8>) {
    let fault = Arc::new(AtomicU8::new(0));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .unwrap();
    let backend = FaultFile {
        file: FileBackend::new(file).unwrap(),
        fault: fault.clone(),
    };
    let db = Database::builder()
        .set_cache_size(0)
        .create_with_backend(backend)
        .unwrap();
    (
        RedbStore {
            db,
            pending: WriteBuffer::default(),
        },
        fault,
    )
}

#[test]
fn disk_read_failure_is_an_error_and_preserves_committed_data() {
    let temp = Temp::new();
    let path = temp.0.join("state.redb");
    let (mut store, fault) = fault_store(&path);
    store.put(0, [1; 32], vec![Goldilocks::ONE; 8192]).unwrap();
    let id = store.commit().unwrap();
    fault.store(1, Ordering::SeqCst);
    assert!(matches!(
        store.read(0, &[1; 32], 8192),
        Err(StorageError::Io(_))
    ));
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "actual backend read must consume the fault"
    );
    drop(store);
    let store = RedbStore::open(&path).unwrap();
    assert_eq!(store.last_commit().unwrap(), Some(id));
    assert_eq!(store.read(0, &[1; 32], 8192).unwrap().unwrap().len(), 8192);
}

fn commit_fault(mode: u8) {
    let temp = Temp::new();
    let path = temp.0.join("state.redb");
    let (mut store, fault) = fault_store(&path);
    store.put(0, [1; 32], vec![Goldilocks::ONE; 8192]).unwrap();
    let previous = store.commit().unwrap();
    store.remove(0, &[1; 32]).unwrap();
    store
        .put(1, [2; 32], vec![Goldilocks::new(99); 8192])
        .unwrap();
    let attempted = store.pending.change_id();
    fault.store(mode, Ordering::SeqCst);
    let error = store
        .commit()
        .expect_err("backend I/O fault cannot return success");
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "backend must consume the injected fault"
    );
    assert!(store.has_pending());
    assert_eq!(store.dirty_entries().len(), 1);
    assert!(store.pending.deleted.contains(&(0, [1; 32])));
    if let StorageError::CommitUnknown { change_id, .. } = &error {
        assert_eq!(*change_id, attempted);
        assert_eq!(store.commit(), Err(error.clone()));
        assert_eq!(
            store.put(2, [3; 32], vec![Goldilocks::ONE]),
            Err(error.clone())
        );
        assert_eq!(store.remove(0, &[1; 32]), Err(error.clone()));
        assert_eq!(store.read(0, &[1; 32], 8192), Err(error));
    } else {
        assert!(matches!(error, StorageError::Io(_)));
    }
    drop(store);
    let recovered = RedbStore::open(&path).unwrap();
    let marker = recovered.last_commit().unwrap().unwrap();
    if marker == attempted {
        assert_eq!(recovered.read(0, &[1; 32], 8192).unwrap(), None);
        assert_eq!(
            recovered.read(1, &[2; 32], 8192).unwrap(),
            Some(vec![Goldilocks::new(99); 8192])
        );
    } else {
        assert_eq!(marker, previous);
        assert_eq!(
            recovered.read(0, &[1; 32], 8192).unwrap(),
            Some(vec![Goldilocks::ONE; 8192])
        );
        assert_eq!(recovered.read(1, &[2; 32], 8192).unwrap(), None);
    }
}

#[test]
fn disk_write_failure_keeps_pending_batch_and_recovers_atomically() {
    commit_fault(2);
}
#[test]
fn disk_sync_failure_keeps_pending_batch_and_recovers_atomically() {
    commit_fault(3);
}
#[test]
fn lost_sync_acknowledgement_is_resolved_by_reopened_marker() {
    commit_fault(4);
}

#[test]
fn failed_warm_commit_retains_hot_pending_state_and_blocks_publication() {
    use super::super::{MemStore, TieredStore};
    let temp = Temp::new();
    let (warm, fault) = fault_store(&temp.0.join("warm.redb"));
    let mut store = TieredStore::new(Box::new(MemStore::new()))
        .with_warm(Box::new(warm))
        .unwrap();
    store.put(0, [1; 32], vec![Goldilocks::new(42)]).unwrap();
    fault.store(3, Ordering::SeqCst);
    let error = store.commit().unwrap_err();
    assert!(store.is_poisoned());
    assert_eq!(store.dirty_entries().len(), 1);
    assert_eq!(store.dirty_entries()[0].2, vec![Goldilocks::new(42)]);
    assert_eq!(store.commit(), Err(error.clone()));
    assert_eq!(store.read(0, &[1; 32], 1), Err(error));
    assert!(store.get_mut(0, &[1; 32]).is_none());
}
