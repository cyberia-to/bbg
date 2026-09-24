use super::{Backend, Database, RecordDomain, RecordLimits};
use crate::storage::application::{ApplicationStore, Error, Head, Write};
use crate::storage::{ShardStore, StorageError};
use ::redb::{Database as RedbDatabase, StorageBackend, backends::FileBackend};
use nebu::Goldilocks;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

#[derive(Debug)]
struct FaultFile {
    file: FileBackend,
    mode: Arc<AtomicU8>,
}
impl FaultFile {
    fn trip(&self, mode: u8) -> std::io::Result<()> {
        if self
            .mode
            .compare_exchange(mode, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Err(std::io::Error::other(
                "injected native composition I/O failure",
            ));
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
    fn write(&self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        self.trip(2)?;
        self.file.write(offset, bytes)
    }
    fn sync_data(&self, eventual: bool) -> std::io::Result<()> {
        self.trip(3)?;
        self.file.sync_data(eventual)?;
        self.trip(4)
    }
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bbg-native-fault-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fault_database(path: &Path) -> (Database, Arc<AtomicU8>) {
    let mode = Arc::new(AtomicU8::new(0));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    let engine = RedbDatabase::builder()
        .set_cache_size(0)
        .create_with_backend(FaultFile {
            file: FileBackend::new(file).unwrap(),
            mode: mode.clone(),
        })
        .unwrap();
    (Database::from_redb(engine), mode)
}

fn seed(db: &Database) -> [u8; 32] {
    let app = ApplicationStore::from_database(db.clone());
    app.apply_with(
        &Write {
            namespace: [1; 32],
            request: [10; 32],
            fingerprint: [11; 32],
            expected: None,
            head: Head {
                index: 0,
                commit: [12; 32],
            },
            content: &[([12; 32], b"old application".to_vec())],
            claims: &[],
        },
        |tx| {
            for (i, domain) in RecordDomain::ALL.into_iter().enumerate() {
                tx.put_record(domain, b"current", &[i as u8, 0])?;
                tx.put_record(domain, b"deleted", &[i as u8, 9])?;
            }
            tx.put_record(RecordDomain::NativeState, b"probe", &vec![7; 65_536])?;
            tx.put_shard(0, [2; 32], &[Goldilocks::ONE])?;
            Ok(())
        },
    )
    .unwrap();
    db.last_transaction().unwrap().unwrap()
}

fn transition(tx: &mut super::Transaction<'_>) -> Result<(), Error> {
    for (i, domain) in RecordDomain::ALL.into_iter().enumerate() {
        tx.put_record(domain, b"current", &[i as u8, 1])?;
        tx.remove_record(domain, b"deleted")?;
        tx.put_record(domain, b"new", &[i as u8, 2])?;
    }
    tx.remove_shard(0, &[2; 32])?;
    tx.put_shard(1, [3; 32], &[Goldilocks::new(99)])?;
    Ok(())
}

fn assert_frozen(db: &Database, app: &ApplicationStore, request: &Write<'_>) -> [u8; 32] {
    let another = db.clone();
    let other_app = ApplicationStore::from_database(db.clone());
    let mut shards = db.shards();
    assert!(db.is_poisoned());
    assert!(shards.is_poisoned());
    let StorageError::CommitUnknown { change_id, .. } = db.last_transaction().unwrap_err() else {
        panic!("expected shared commit failure")
    };
    for domain in RecordDomain::ALL {
        assert!(matches!(
            another.read_record(domain, b"current", 2),
            Err(StorageError::CommitUnknown { .. })
        ));
        assert!(matches!(
            another.scan_records(
                domain,
                None,
                b"",
                RecordLimits {
                    max_entries: 1,
                    max_bytes: 100
                }
            ),
            Err(StorageError::CommitUnknown { .. })
        ));
    }
    assert!(matches!(
        another.transaction::<(), StorageError>(|_| panic!("frozen transaction ran")),
        Err(StorageError::CommitUnknown { .. })
    ));
    assert!(matches!(app.head(&[1; 32]), Err(Error::CommitUnknown(_))));
    assert!(matches!(
        other_app.content(&[12; 32], 100),
        Err(Error::CommitUnknown(_))
    ));
    assert!(matches!(
        other_app.resolve(&[1; 32], &[20; 32]),
        Err(Error::CommitUnknown(_))
    ));
    assert!(matches!(
        other_app.apply_with(request, |_| panic!("frozen application ran")),
        Err(Error::CommitUnknown(_))
    ));
    assert!(matches!(
        shards.put(2, [4; 32], vec![Goldilocks::ONE]),
        Err(StorageError::CommitUnknown { .. })
    ));
    assert!(matches!(
        shards.read(0, &[2; 32], 1),
        Err(StorageError::CommitUnknown { .. })
    ));
    assert!(matches!(
        shards.commit(),
        Err(StorageError::CommitUnknown { .. })
    ));
    change_id
}

fn assert_recovered(db: &Database, accepted: bool) {
    for (i, domain) in RecordDomain::ALL.into_iter().enumerate() {
        assert_eq!(
            db.read_record(domain, b"current", 2).unwrap(),
            Some(vec![i as u8, u8::from(accepted)])
        );
        assert_eq!(
            db.read_record(domain, b"deleted", 2).unwrap(),
            (!accepted).then(|| vec![i as u8, 9])
        );
        assert_eq!(
            db.read_record(domain, b"new", 2).unwrap(),
            accepted.then(|| vec![i as u8, 2])
        );
    }
    assert_eq!(
        db.read_record(RecordDomain::NativeState, b"probe", 65_536)
            .unwrap(),
        Some(vec![7; 65_536])
    );
    let shards = db.shards();
    assert_eq!(
        shards.read(0, &[2; 32], 1).unwrap(),
        (!accepted).then(|| vec![Goldilocks::ONE])
    );
    assert_eq!(
        shards.read(1, &[3; 32], 1).unwrap(),
        accepted.then(|| vec![Goldilocks::new(99)])
    );
    let app = ApplicationStore::from_database(db.clone());
    let expected = if accepted {
        Head {
            index: 1,
            commit: [13; 32],
        }
    } else {
        Head {
            index: 0,
            commit: [12; 32],
        }
    };
    assert_eq!(app.head(&[1; 32]).unwrap(), Some(expected));
    assert_eq!(
        app.history(&[1; 32], None, 10).unwrap().len(),
        1 + usize::from(accepted)
    );
    assert_eq!(
        app.resolve(&[1; 32], &[20; 32]).unwrap(),
        accepted.then_some(([21; 32], expected))
    );
    assert_eq!(
        app.content(&[13; 32], 100).unwrap(),
        accepted.then(|| b"new application".to_vec())
    );
    assert_eq!(
        shards.last_commit().unwrap(),
        db.last_transaction().unwrap()
    );
}

fn composition_fault(mode: u8) {
    let fixture = Fixture::new();
    let path = fixture.0.join("native.redb");
    let (db, fault) = fault_database(&path);
    let old_marker = seed(&db);
    let app = ApplicationStore::from_database(db.clone());
    let content = [([13; 32], b"new application".to_vec())];
    let request = Write {
        namespace: [1; 32],
        request: [20; 32],
        fingerprint: [21; 32],
        expected: Some(Head {
            index: 0,
            commit: [12; 32],
        }),
        head: Head {
            index: 1,
            commit: [13; 32],
        },
        content: &content,
        claims: &[([40; 32], [50; 32])],
    };
    let result = app.apply_with(&request, |tx| {
        transition(tx)?;
        fault.store(mode, Ordering::SeqCst);
        if mode == 1 {
            tx.read_record(RecordDomain::NativeState, b"probe", 65_536)?;
        }
        Ok(())
    });
    assert_eq!(
        fault.load(Ordering::SeqCst),
        0,
        "the actual backend must consume fault {mode}"
    );
    let attempted = match result {
        Err(Error::CommitUnknown(_)) => Some(assert_frozen(&db, &app, &request)),
        Err(Error::Storage(_)) if mode <= 2 => {
            assert!(!db.is_poisoned());
            None
        }
        other => panic!("fault {mode} returned {other:?}"),
    };
    if mode == 1 {
        assert!(attempted.is_none());
        // Redb seals its own handle after a physical read failure. The closure
        // aborted before commit; recovery must still explicitly reopen it.
        assert!(matches!(
            db.read_record(RecordDomain::NativeState, b"current", 2),
            Err(StorageError::Io(_))
        ));
    }
    drop(app);
    drop(db);
    let db = Database::open(&path, Backend::Hdd).unwrap();
    let app = ApplicationStore::from_database(db.clone());
    let accepted = app.resolve(&[1; 32], &[20; 32]).unwrap().is_some();
    assert_recovered(&db, accepted);
    if accepted {
        assert_eq!(db.last_transaction().unwrap(), attempted);
        assert_eq!(
            app.apply_with(&request, |_| panic!("accepted transition executed twice"))
                .unwrap(),
            request.head
        );
        assert_eq!(db.last_transaction().unwrap(), attempted);
    } else {
        assert_eq!(db.last_transaction().unwrap(), Some(old_marker));
        app.apply_with(&request, transition).unwrap();
        assert_recovered(&db, true);
    }
    // The default checksummed redb commit has one barrier after root publication.
    // Returning an error after that real barrier must recover the committed state.
    if mode == 4 {
        assert!(
            accepted,
            "completed barrier must retain all composed domains"
        );
    }
    let competitor = Write {
        namespace: [6; 32],
        expected: None,
        head: Head {
            index: 0,
            commit: [13; 32],
        },
        claims: &[([40; 32], [51; 32])],
        ..request
    };
    assert!(matches!(app.apply(&competitor), Err(Error::Conflict)));
}

#[test]
fn native_precommit_read_error_rolls_back_every_composed_domain() {
    composition_fault(1);
}
#[test]
fn native_write_error_recovers_a_whole_composed_commit() {
    composition_fault(2);
}
#[test]
fn native_sync_error_freezes_every_shared_view_until_reopen() {
    composition_fault(3);
}
#[test]
fn native_lost_barrier_ack_recovers_all_domains_and_deduplicates_retry() {
    composition_fault(4);
}
