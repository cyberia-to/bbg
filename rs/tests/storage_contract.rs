#![cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]

use bbg::storage::{Durability, MemStore, ScanLimits, ShardStore, StorageError, TieredStore, dim};
use nebu::Goldilocks;
use std::path::{Path, PathBuf};

struct StorePath(PathBuf);
impl StorePath {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-contract-{}-{}",
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

fn backends() -> Vec<&'static str> {
    vec![
        #[cfg(feature = "backend-ssd")]
        "fjall",
        #[cfg(feature = "backend-hdd")]
        "redb",
    ]
}

fn open(backend: &str, path: &Path) -> Box<dyn ShardStore> {
    match backend {
        #[cfg(feature = "backend-ssd")]
        "fjall" => Box::new(bbg::storage::FjallStore::open(path).unwrap()),
        #[cfg(feature = "backend-hdd")]
        "redb" => Box::new(bbg::storage::RedbStore::open(path).unwrap()),
        _ => panic!("unknown backend"),
    }
}
fn g(value: u64) -> Vec<Goldilocks> {
    vec![Goldilocks::new(value)]
}
fn limits(entries: usize, elements: usize) -> ScanLimits {
    ScanLimits {
        max_entries: entries,
        max_elements: elements,
    }
}

#[test]
fn reopened_trait_reads_and_paginates_every_persistent_dimension() {
    for backend in backends() {
        let temp = StorePath::new();
        let path = temp.0.join(backend);
        let id;
        {
            let mut store = open(backend, &path);
            assert_eq!(store.last_commit().unwrap(), None);
            for dimension in 0..dim::EPHEMERAL {
                for key in [3, 1, 2] {
                    store.put(dimension, [key; 32], g(key as u64)).unwrap();
                }
            }
            store.put(dim::EPHEMERAL, [1; 32], g(999)).unwrap();
            assert!(matches!(
                store.scan(0, None, limits(2, 2)),
                Err(StorageError::PendingWrites)
            ));
            id = store.commit().unwrap();
            assert!(!store.has_pending());
            assert_eq!(store.commit().unwrap(), id);
        }
        let store = open(backend, &path);
        assert_eq!(store.durability(), Durability::Disk);
        assert_eq!(store.last_commit().unwrap(), Some(id));
        for dimension in 0..dim::EPHEMERAL {
            assert_eq!(store.read(dimension, &[2; 32], 1).unwrap(), Some(g(2)));
            assert_eq!(
                store.scan(dimension, None, limits(2, 2)).unwrap(),
                vec![([1; 32], g(1)), ([2; 32], g(2))]
            );
            assert_eq!(
                store.scan(dimension, Some([2; 32]), limits(2, 2)).unwrap(),
                vec![([3; 32], g(3))]
            );
            assert!(
                store
                    .scan(dimension, Some([255; 32]), limits(2, 2))
                    .unwrap()
                    .is_empty()
            );
        }
        assert_eq!(store.read(dim::EPHEMERAL, &[1; 32], 1).unwrap(), None);
    }
}

#[test]
fn deletes_are_staged_with_updates_and_survive_reopen() {
    for backend in backends() {
        let temp = StorePath::new();
        let path = temp.0.join(backend);
        let old_id;
        {
            let mut store = open(backend, &path);
            store.put(0, [1; 32], g(10)).unwrap();
            store.put(1, [1; 32], g(20)).unwrap();
            old_id = store.commit().unwrap();
            assert_eq!(store.remove(0, &[1; 32]).unwrap(), Some(g(10)));
            store.put(1, [1; 32], g(99)).unwrap();
            assert_eq!(store.read(0, &[1; 32], 1).unwrap(), None);
            // Dropping staged changes must preserve the previous transaction.
        }
        let new_id;
        {
            let mut store = open(backend, &path);
            assert_eq!(store.last_commit().unwrap(), Some(old_id));
            assert_eq!(store.read(0, &[1; 32], 1).unwrap(), Some(g(10)));
            assert_eq!(store.read(1, &[1; 32], 1).unwrap(), Some(g(20)));
            store.remove(0, &[1; 32]).unwrap();
            store.put(1, [1; 32], g(99)).unwrap();
            new_id = store.commit().unwrap();
            assert_ne!(old_id, new_id);
        }
        let store = open(backend, &path);
        assert_eq!(store.last_commit().unwrap(), Some(new_id));
        assert_eq!(store.read(0, &[1; 32], 1).unwrap(), None);
        assert_eq!(store.read(1, &[1; 32], 1).unwrap(), Some(g(99)));
    }
}

#[test]
fn commit_identity_binds_values_deletes_and_canonical_order() {
    for backend in backends() {
        let temp = StorePath::new();
        let mut left = open(backend, &temp.0.join("left"));
        let mut right = open(backend, &temp.0.join("right"));
        left.put(0, [1; 32], g(1)).unwrap();
        left.put(0, [2; 32], g(2)).unwrap();
        right.put(0, [2; 32], g(100)).unwrap();
        right.put(0, [1; 32], g(1)).unwrap();
        right.put(0, [2; 32], g(2)).unwrap();
        assert_eq!(left.commit().unwrap(), right.commit().unwrap());
        left.put(0, [1; 32], g(3)).unwrap();
        right.put(0, [1; 32], g(4)).unwrap();
        assert_ne!(left.commit().unwrap(), right.commit().unwrap());
        left.remove(0, &[1; 32]).unwrap();
        right.put(0, [1; 32], Vec::new()).unwrap();
        assert_ne!(left.commit().unwrap(), right.commit().unwrap());
    }
}

#[test]
fn disk_access_enforces_dimensions_and_allocation_budgets() {
    for backend in backends() {
        let temp = StorePath::new();
        let mut store = open(backend, &temp.0.join(backend));
        store.put(0, [1; 32], vec![Goldilocks::ONE; 3]).unwrap();
        store.put(0, [2; 32], vec![Goldilocks::ONE; 3]).unwrap();
        store.commit().unwrap();
        assert!(matches!(
            store.read(0, &[1; 32], 2),
            Err(StorageError::Limit(_))
        ));
        assert!(matches!(
            store.read(255, &[1; 32], 2),
            Err(StorageError::InvalidDimension(255))
        ));
        assert!(matches!(
            store.remove(255, &[1; 32]),
            Err(StorageError::InvalidDimension(255))
        ));
        assert!(matches!(
            store.put(255, [1; 32], g(1)),
            Err(StorageError::InvalidDimension(255))
        ));
        assert!(matches!(
            store.scan(0, None, limits(0, 3)),
            Err(StorageError::Limit(_))
        ));
        assert!(matches!(
            store.scan(0, None, limits(10, 2)),
            Err(StorageError::Limit(_))
        ));
        assert_eq!(store.scan(0, None, limits(10, 4)).unwrap().len(), 1);
        assert!(!store.has_pending());
    }
}

#[test]
fn warm_recovery_and_deletion_override_an_old_archive() {
    for backend in backends() {
        let temp = StorePath::new();
        let path = temp.0.join(backend);
        {
            let mut warm = open(backend, &path);
            warm.put(0, [1; 32], g(10)).unwrap();
            warm.commit().unwrap();
        }
        let mut archive = MemStore::new();
        archive.put(0, [1; 32], g(5)).unwrap();
        let mut tiered = TieredStore::default()
            .with_warm(open(backend, &path))
            .unwrap()
            .with_cold(Box::new(archive));
        assert_eq!(tiered.read(0, &[1; 32], 1).unwrap(), Some(g(10)));
        assert!(tiered.promote(0, &[1; 32]).unwrap());
        tiered.get_mut(0, &[1; 32]).unwrap()[0] = Goldilocks::new(99);
        tiered.mark_dirty(0, [1; 32]).unwrap();
        tiered.evict(0, &[1; 32]).unwrap();
        assert!(
            tiered.get_mut(0, &[1; 32]).is_some(),
            "unfinished write must retain HOT"
        );
        tiered.commit().unwrap();
        tiered.evict(0, &[1; 32]).unwrap();
        assert!(tiered.get_mut(0, &[1; 32]).is_none());
        assert_eq!(tiered.remove(0, &[1; 32]).unwrap(), Some(g(99)));
        tiered.commit().unwrap();
        assert_eq!(tiered.read(0, &[1; 32], 1).unwrap(), None);
        drop(tiered);
        let mut archive = MemStore::new();
        archive.put(0, [1; 32], g(5)).unwrap();
        let tiered = TieredStore::default()
            .with_warm(open(backend, &path))
            .unwrap()
            .with_cold(Box::new(archive));
        assert_eq!(tiered.read(0, &[1; 32], 1).unwrap(), None);
    }
}

fn write_corrupt(backend: &str, path: &Path, key: &[u8], bytes: &[u8]) {
    match backend {
        #[cfg(feature = "backend-ssd")]
        "fjall" => {
            let db = fjall::Config::new(path).open().unwrap();
            let part = db
                .open_partition("particles", fjall::PartitionCreateOptions::default())
                .unwrap();
            let mut batch = db.batch().durability(Some(fjall::PersistMode::SyncAll));
            batch.insert(&part, key, bytes);
            batch.commit().unwrap();
        }
        #[cfg(feature = "backend-hdd")]
        "redb" => {
            let db = redb::Database::create(path).unwrap();
            let txn = db.begin_write().unwrap();
            txn.open_table(redb::TableDefinition::<&[u8], &[u8]>::new("particles"))
                .unwrap()
                .insert(key, bytes)
                .unwrap();
            txn.commit().unwrap();
        }
        _ => panic!("unknown backend"),
    }
}

#[test]
fn malformed_disk_values_are_errors_not_missing_or_reduced_values() {
    for backend in backends() {
        for bytes in [vec![1, 2, 3], nebu::field::P.to_le_bytes().to_vec()] {
            let temp = StorePath::new();
            let path = temp.0.join(backend);
            write_corrupt(backend, &path, &[1; 32], &bytes);
            let store = open(backend, &path);
            assert!(matches!(
                store.read(0, &[1; 32], 10),
                Err(StorageError::Corrupt(_))
            ));
            assert!(matches!(
                store.scan(0, None, limits(10, 10)),
                Err(StorageError::Corrupt(_))
            ));
            assert_eq!(store.read(0, &[2; 32], 10).unwrap(), None);
        }
    }
}

#[test]
fn full_scan_detects_malformed_keys_before_the_smallest_valid_key() {
    for backend in backends() {
        let temp = StorePath::new();
        let path = temp.0.join(backend);
        write_corrupt(backend, &path, &[0], &1u64.to_le_bytes());
        let store = open(backend, &path);
        assert!(matches!(
            store.scan(0, None, limits(10, 10)),
            Err(StorageError::Corrupt(_))
        ));
    }
}

#[test]
fn a_second_writer_cannot_open_the_same_store() {
    for backend in backends() {
        let temp = StorePath::new();
        let path = temp.0.join(backend);
        let first = open(backend, &path);
        match backend {
            #[cfg(feature = "backend-ssd")]
            "fjall" => assert!(matches!(
                bbg::storage::FjallStore::open(&path),
                Err(StorageError::Busy)
            )),
            #[cfg(feature = "backend-hdd")]
            "redb" => assert!(matches!(
                bbg::storage::RedbStore::open(&path),
                Err(StorageError::Busy)
            )),
            _ => unreachable!(),
        }
        drop(first);
        drop(open(backend, &path));
    }
}

#[test]
fn durable_attachment_rejects_a_conflicting_hot_cache() {
    for backend in backends() {
        let temp = StorePath::new();
        let mut hot = MemStore::new();
        hot.put(0, [1; 32], g(10)).unwrap();
        hot.commit().unwrap();
        let mut warm = open(backend, &temp.0.join(backend));
        warm.put(0, [1; 32], g(99)).unwrap();
        warm.commit().unwrap();
        assert!(matches!(
            TieredStore::new(Box::new(hot)).with_warm(warm),
            Err(StorageError::Unsupported(_))
        ));
        let disk_hot = open(backend, &temp.0.join("disk-hot"));
        let warm = open(backend, &temp.0.join("another-warm"));
        assert!(matches!(
            TieredStore::new(disk_hot).with_warm(warm),
            Err(StorageError::Unsupported(_))
        ));
    }
}

#[test]
fn pending_element_budget_rejects_growth_without_losing_earlier_values() {
    let mut store = MemStore::new();
    for key in 0..16 {
        store
            .put(0, [key; 32], vec![Goldilocks::new(key as u64); 131_072])
            .unwrap();
    }
    assert!(matches!(
        store.put(0, [16; 32], vec![Goldilocks::ONE; 1]),
        Err(StorageError::Limit(_))
    ));
    assert_eq!(store.dirty_entries().len(), 16);
    assert_eq!(store.read(0, &[16; 32], 1).unwrap(), None);
    // Shrinking an existing value frees exactly its pending element budget.
    store
        .put(0, [0; 32], vec![Goldilocks::ONE; 131_071])
        .unwrap();
    store.put(0, [16; 32], vec![Goldilocks::ONE]).unwrap();
    assert_eq!(
        store.read(0, &[16; 32], 1).unwrap(),
        Some(vec![Goldilocks::ONE])
    );
    assert_eq!(
        store.read(0, &[1; 32], 131_072).unwrap(),
        Some(vec![Goldilocks::ONE; 131_072])
    );
}

#[test]
fn repeated_hot_overwrites_are_coalesced_and_rejected_writes_leave_store_usable() {
    for backend in backends() {
        let temp = StorePath::new();
        let mut store = TieredStore::default()
            .with_warm(open(backend, &temp.0.join(backend)))
            .unwrap();
        for value in 0..64 {
            store
                .put(0, [1; 32], vec![Goldilocks::new(value); 131_072])
                .unwrap();
            assert_eq!(store.dirty_entries().len(), 1);
        }
        assert!(matches!(
            store.put(255, [1; 32], g(1)),
            Err(StorageError::InvalidDimension(255))
        ));
        assert!(matches!(
            store.put(0, [2; 32], vec![Goldilocks::ONE; 131_073]),
            Err(StorageError::Limit(_))
        ));
        assert!(!store.is_poisoned());
        store.commit().unwrap();
        assert_eq!(
            store.read(0, &[1; 32], 131_072).unwrap().unwrap()[0],
            Goldilocks::new(63)
        );
    }
}

#[test]
fn storage_child() {
    use std::io::Write;
    let Ok(path) = std::env::var("BBG_CONTRACT_CHILD_PATH") else {
        return;
    };
    let backend = std::env::var("BBG_CONTRACT_CHILD_BACKEND").unwrap();
    let phase = std::env::var("BBG_CONTRACT_CHILD_PHASE").unwrap();
    let mut store = open(&backend, Path::new(&path));
    store.remove(0, &[1; 32]).unwrap();
    store.put(1, [1; 32], g(99)).unwrap();
    store.put(2, [1; 32], g(77)).unwrap();
    if phase == "committed" {
        store.commit().unwrap();
    }
    println!("BBG_CONTRACT_KILL_READY");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn process_kill_preserves_atomic_batches_and_releases_the_writer() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    for backend in backends() {
        for phase in ["staged", "committed"] {
            let temp = StorePath::new();
            let path = temp.0.join(backend);
            let old_id;
            {
                let mut store = open(backend, &path);
                store.put(0, [1; 32], g(10)).unwrap();
                store.put(1, [1; 32], g(20)).unwrap();
                old_id = store.commit().unwrap();
            }
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "storage_child", "--nocapture"])
                .env("BBG_CONTRACT_CHILD_PATH", &path)
                .env("BBG_CONTRACT_CHILD_BACKEND", backend)
                .env("BBG_CONTRACT_CHILD_PHASE", phase)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (send, receive) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                let ready = BufReader::new(stdout)
                    .lines()
                    .any(|line| line.is_ok_and(|line| line.contains("BBG_CONTRACT_KILL_READY")));
                let _ = send.send(ready);
            });
            let ready = receive.recv_timeout(std::time::Duration::from_secs(20));
            let _ = child.kill();
            let status = child.wait().unwrap();
            reader.join().unwrap();
            assert_eq!(ready, Ok(true), "child failed to reach {phase}");
            assert!(!status.success());
            let store = open(backend, &path);
            if phase == "committed" {
                assert_eq!(store.read(0, &[1; 32], 1).unwrap(), None);
                assert_eq!(store.read(1, &[1; 32], 1).unwrap(), Some(g(99)));
                assert_eq!(store.read(2, &[1; 32], 1).unwrap(), Some(g(77)));
                assert_ne!(store.last_commit().unwrap(), Some(old_id));
            } else {
                assert_eq!(store.read(0, &[1; 32], 1).unwrap(), Some(g(10)));
                assert_eq!(store.read(1, &[1; 32], 1).unwrap(), Some(g(20)));
                assert_eq!(store.read(2, &[1; 32], 1).unwrap(), None);
                assert_eq!(store.last_commit().unwrap(), Some(old_id));
            }
        }
    }
}
