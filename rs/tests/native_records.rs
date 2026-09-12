#![cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]

use bbg::storage::application::{ApplicationStore, Error, Head, Write};
use bbg::storage::database::{Backend, Database, RecordDomain, RecordLimits};
use bbg::storage::{ShardStore, StorageError};
use nebu::Goldilocks;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DOMAINS: [RecordDomain; 7] = [
    RecordDomain::NativeState,
    RecordDomain::NativeHistory,
    RecordDomain::NativeRequests,
    RecordDomain::NativeMetadata,
    RecordDomain::NativeBalances,
    RecordDomain::NativeBlocks,
    RecordDomain::NativeExport,
];
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-native-records-{}-{}",
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
fn limits(max_entries: usize, max_bytes: usize) -> RecordLimits {
    RecordLimits {
        max_entries,
        max_bytes,
    }
}

fn seed(db: &Database) {
    db.transaction::<_, StorageError>(|tx| {
        for (i, domain) in DOMAINS.into_iter().enumerate() {
            tx.put_record(domain, b"current", &[i as u8, 0])?;
            tx.put_record(domain, b"deleted", &[i as u8, 9])?;
        }
        tx.put_shard(0, [1; 32], &[Goldilocks::ONE])?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn native_domains_shards_and_application_publish_in_one_reopenable_commit() {
    for backend in backends() {
        let fixture = Fixture::new();
        let path = fixture.path(backend);
        let db = Database::open(&path, backend).unwrap();
        seed(&db);
        let clone = db.clone();
        let app = ApplicationStore::from_database(db.clone());
        let shards = db.shards();
        let selected = Head {
            index: 0,
            commit: [3; 32],
        };
        let request = Write {
            namespace: [2; 32],
            request: [4; 32],
            fingerprint: [5; 32],
            expected: None,
            head: selected,
            content: &[([3; 32], b"accepted".to_vec())],
            claims: &[([6; 32], [7; 32])],
        };
        app.apply_with(&request, |tx| {
            for (i, domain) in DOMAINS.into_iter().enumerate() {
                tx.put_record(domain, b"current", &[i as u8, 1])?;
                tx.remove_record(domain, b"deleted")?;
                tx.put_record(domain, b"new", &[i as u8, 2])?;
                assert_eq!(
                    tx.read_record(domain, b"current", 2)?,
                    Some(vec![i as u8, 1])
                );
                assert_eq!(tx.read_record(domain, b"deleted", 2)?, None);
            }
            tx.remove_shard(0, &[1; 32])?;
            tx.put_shard(1, [8; 32], &[Goldilocks::new(99)])?;
            Ok(())
        })
        .unwrap();
        let marker = db.last_transaction().unwrap();
        assert_eq!(shards.last_commit().unwrap(), marker);
        for (i, domain) in DOMAINS.into_iter().enumerate() {
            assert_eq!(
                clone.read_record(domain, b"current", 2).unwrap(),
                Some(vec![i as u8, 1])
            );
            assert_eq!(clone.read_record(domain, b"deleted", 2).unwrap(), None);
        }
        assert_eq!(
            app.apply_with(&request, |_| panic!("recorded retry ran again"))
                .unwrap(),
            selected
        );
        assert_eq!(db.last_transaction().unwrap(), marker);
        drop(db);
        drop(clone);
        assert!(matches!(
            Database::open(&path, backend),
            Err(StorageError::Busy)
        ));
        drop(app);
        assert!(matches!(
            Database::open(&path, backend),
            Err(StorageError::Busy)
        ));
        drop(shards);
        let reopened = Database::open(&path, backend).unwrap();
        let app = ApplicationStore::from_database(reopened.clone());
        assert_eq!(reopened.last_transaction().unwrap(), marker);
        assert_eq!(app.head(&[2; 32]).unwrap(), Some(selected));
        assert_eq!(
            app.resolve(&[2; 32], &[4; 32]).unwrap(),
            Some(([5; 32], selected))
        );
        assert_eq!(
            app.content(&[3; 32], 8).unwrap(),
            Some(b"accepted".to_vec())
        );
        assert_eq!(app.history(&[2; 32], None, 10).unwrap(), vec![selected]);
        assert_eq!(reopened.shards().read(0, &[1; 32], 1).unwrap(), None);
        assert_eq!(
            reopened.shards().read(1, &[8; 32], 1).unwrap(),
            Some(vec![Goldilocks::new(99)])
        );
        for (i, domain) in DOMAINS.into_iter().enumerate() {
            assert_eq!(
                reopened
                    .scan_records(domain, None, b"", limits(10, 100))
                    .unwrap(),
                vec![
                    (b"current".to_vec(), vec![i as u8, 1]),
                    (b"new".to_vec(), vec![i as u8, 2])
                ]
            );
        }
        let other = Write {
            namespace: [9; 32],
            claims: &[([6; 32], [10; 32])],
            ..request
        };
        assert!(matches!(app.apply(&other), Err(Error::Conflict)));
    }
}

#[test]
fn rejected_application_transition_discards_all_native_and_shard_changes() {
    for backend in backends() {
        let fixture = Fixture::new();
        let path = fixture.path(backend);
        let db = Database::open(&path, backend).unwrap();
        seed(&db);
        let before = db.last_transaction().unwrap();
        let app = ApplicationStore::from_database(db.clone());
        let request = Write {
            namespace: [2; 32],
            request: [4; 32],
            fingerprint: [5; 32],
            expected: None,
            head: Head {
                index: 0,
                commit: [3; 32],
            },
            content: &[([3; 32], b"staged".to_vec())],
            claims: &[([6; 32], [7; 32])],
        };
        let result = app.apply_with(&request, |tx| {
            for domain in DOMAINS {
                tx.put_record(domain, b"current", b"changed")?;
                tx.remove_record(domain, b"deleted")?;
                tx.put_record(domain, b"new", b"unpublished")?;
            }
            tx.remove_shard(0, &[1; 32])?;
            tx.put_shard(1, [8; 32], &[Goldilocks::new(99)])?;
            Err(Error::Conflict)
        });
        assert!(matches!(result, Err(Error::Conflict)));
        assert!(!db.is_poisoned());
        assert_eq!(db.last_transaction().unwrap(), before);
        drop(app);
        drop(db);
        let db = Database::open(&path, backend).unwrap();
        for (i, domain) in DOMAINS.into_iter().enumerate() {
            assert_eq!(
                db.read_record(domain, b"current", 2).unwrap(),
                Some(vec![i as u8, 0])
            );
            assert_eq!(
                db.read_record(domain, b"deleted", 2).unwrap(),
                Some(vec![i as u8, 9])
            );
            assert_eq!(db.read_record(domain, b"new", 100).unwrap(), None);
        }
        assert_eq!(
            db.shards().read(0, &[1; 32], 1).unwrap(),
            Some(vec![Goldilocks::ONE])
        );
        assert_eq!(db.shards().read(1, &[8; 32], 1).unwrap(), None);
        let app = ApplicationStore::from_database(db);
        assert_eq!(app.head(&[2; 32]).unwrap(), None);
        assert_eq!(app.resolve(&[2; 32], &[4; 32]).unwrap(), None);
        assert_eq!(app.content(&[3; 32], 100).unwrap(), None);
        // The rejected claim did not reserve this global identity.
        let competitor = Write {
            claims: &[([6; 32], [11; 32])],
            ..request
        };
        app.apply(&competitor).unwrap();
    }
}

#[test]
fn native_record_reads_and_scans_enforce_bounds_without_cross_domain_leaks() {
    for backend in backends() {
        let fixture = Fixture::new();
        let db = Database::open(fixture.path(backend), backend).unwrap();
        db.transaction::<_, StorageError>(|tx| {
            for (i, domain) in DOMAINS.into_iter().enumerate() {
                for key in [b"a".as_slice(), b"ab", b"ac", b"b"] {
                    tx.put_record(domain, key, &[i as u8; 4])?;
                }
            }
            Ok(())
        })
        .unwrap();
        let before = db.last_transaction().unwrap();
        for (i, domain) in DOMAINS.into_iter().enumerate() {
            assert_eq!(
                db.scan_records(domain, Some(b"a"), b"a", limits(10, 100))
                    .unwrap(),
                vec![
                    (b"ab".to_vec(), vec![i as u8; 4]),
                    (b"ac".to_vec(), vec![i as u8; 4])
                ]
            );
            assert_eq!(
                db.scan_records(domain, None, b"a", limits(10, 10))
                    .unwrap()
                    .len(),
                1
            );
            assert!(matches!(
                db.read_record(domain, b"a", 3),
                Err(StorageError::Limit(_))
            ));
            assert!(matches!(
                db.scan_records(domain, None, b"a", limits(10, 4)),
                Err(StorageError::Limit(_))
            ));
            assert!(matches!(
                db.scan_records(domain, None, b"", limits(0, 100)),
                Err(StorageError::Limit(_))
            ));
            let result = db.transaction::<_, StorageError>(|tx| {
                tx.put_record(domain, b"a", b"pending")?;
                assert!(matches!(
                    tx.scan_records(domain, None, b"", limits(10, 100)),
                    Err(StorageError::PendingWrites)
                ));
                tx.put_record(domain, &[0; 65], b"invalid")
            });
            assert!(matches!(result, Err(StorageError::Limit(_))));
            assert_eq!(
                db.read_record(domain, b"a", 4).unwrap(),
                Some(vec![i as u8; 4])
            );
        }
        assert_eq!(db.last_transaction().unwrap(), before);
        assert!(!db.is_poisoned());
    }
}
