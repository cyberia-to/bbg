use super::*;
use crate::storage::database::Backend;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "content-fault-{}-{}",
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
struct Hash(hemera::Hasher);
impl Hash {
    fn new() -> Self {
        Self(hemera::Hasher::new())
    }
}
impl Verifier for Hash {
    fn profile(&self) -> Particle {
        [7; 32]
    }
    fn update(&mut self, bytes: &[u8]) -> Result<()> {
        self.0.update(bytes);
        Ok(())
    }
    fn finish(self) -> Result<Particle> {
        Ok(*self.0.finalize().as_bytes())
    }
}
fn upload() -> Upload {
    Upload {
        namespace: [1; 32],
        request: [2; 32],
    }
}
fn spec() -> Spec {
    Spec {
        particle: *hemera::hash(b"abcd").as_bytes(),
        profile: [7; 32],
        length: 4,
        part_bytes: 4,
    }
}

#[test]
fn corrupt_parts_are_rejected_by_verification_and_sealed_reads() {
    for backend in [
        #[cfg(feature = "backend-ssd")]
        Backend::Ssd,
        #[cfg(feature = "backend-hdd")]
        Backend::Hdd,
    ] {
        let temp = Temp::new();
        let db = Database::open(temp.0.join("db"), backend).unwrap();
        let store = ContentStore::from_database(db.clone());
        store.begin(upload(), spec()).unwrap();
        store.write_part(upload(), 0, b"abcd").unwrap();
        let original = store
            .verify(upload(), Hash::new())
            .unwrap()
            .step(1)
            .unwrap()
            .unwrap();
        db.transaction::<_, StorageError>(|tx| {
            tx.put(Table::Parts, &part_key(upload(), 0), b"abce")
        })
        .unwrap();
        assert!(matches!(
            store.read_range([1; 32], original.spec.particle, [7; 32], 0, 4),
            Err(Error::Storage(StorageError::Corrupt(_)))
        ));
        assert!(matches!(
            store.verify(upload(), Hash::new()).unwrap().step(1),
            Err(Error::Storage(StorageError::Corrupt(_)))
        ));
    }
}

#[test]
fn cancelling_a_sparse_huge_upload_visits_only_stored_parts() {
    for backend in [
        #[cfg(feature = "backend-ssd")]
        Backend::Ssd,
        #[cfg(feature = "backend-hdd")]
        Backend::Hdd,
    ] {
        let temp = Temp::new();
        let db = Database::open(temp.0.join("db"), backend).unwrap();
        let store = ContentStore::from_database(db.clone());
        let spec = Spec {
            length: u64::MAX,
            part_bytes: 1,
            ..spec()
        };
        store.begin(upload(), spec).unwrap();
        store.write_part(upload(), u64::MAX - 1, b"x").unwrap();
        assert!(store.cancel(upload(), 1).unwrap());
        let progress = store.progress(upload()).unwrap().unwrap();
        assert_eq!(progress.present_parts, 0);
        assert_eq!(progress.reclaimed_through, u64::MAX);
        assert!(
            db.read(Table::Parts, &part_key(upload(), u64::MAX - 1), 1)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn publication_and_uploads_obey_existing_namespace_and_export_fences() {
    for backend in [
        #[cfg(feature = "backend-ssd")]
        Backend::Ssd,
        #[cfg(feature = "backend-hdd")]
        Backend::Hdd,
    ] {
        for mode in 0..3 {
            let temp = Temp::new();
            let db = Database::open(temp.0.join("db"), backend).unwrap();
            let store = ContentStore::from_database(db.clone());
            store.begin(upload(), spec()).unwrap();
            store.write_part(upload(), 0, b"abcd").unwrap();
            db.transaction::<_, StorageError>(|tx| match mode {
                0 => tx.put(Table::Migration, b"status", b"export-v1"),
                1 => tx.put(
                    Table::Migration,
                    &crate::storage::application::fence_key(&[1; 32]),
                    &[8; 64],
                ),
                _ => tx.put(
                    Table::Migration,
                    &crate::storage::application::transfer_stage_key(&[1; 32]),
                    &[8; 32],
                ),
            })
            .unwrap();
            assert_eq!(store.begin(upload(), spec()), Err(Error::Conflict));
            assert_eq!(store.write_part(upload(), 0, b"abcd"), Err(Error::Conflict));
            assert_eq!(
                store.verify(upload(), Hash::new()).unwrap().step(1),
                Err(Error::Conflict)
            );
            assert_eq!(store.cancel(upload(), 1), Err(Error::Conflict));
        }
    }
}

#[test]
fn legacy_namespace_migration_preserves_staging_and_sealed_files() {
    use crate::storage::application::{
        ApplicationStore, Error as AppError, Head, NamespaceMigration, Write,
    };
    for backend in [
        #[cfg(feature = "backend-ssd")]
        Backend::Ssd,
        #[cfg(feature = "backend-hdd")]
        Backend::Hdd,
    ] {
        for sealed in [false, true] {
            let temp = Temp::new();
            let db = Database::open(temp.0.join("db"), backend).unwrap();
            let app = ApplicationStore::from_database(db.clone());
            let store = ContentStore::from_database(db);
            let head = Head {
                index: 0,
                commit: [9; 32],
            };
            let content = [(head.commit, b"head".to_vec())];
            let write = Write {
                namespace: upload().namespace,
                request: [8; 32],
                fingerprint: [7; 32],
                expected: None,
                head,
                content: &content,
                claims: &[],
            };
            app.apply(&write).unwrap();
            store.begin(upload(), spec()).unwrap();
            if sealed {
                store.write_part(upload(), 0, b"abcd").unwrap();
                store
                    .verify(upload(), Hash::new())
                    .unwrap()
                    .step(1)
                    .unwrap();
            }
            let target = Write {
                namespace: [3; 32],
                ..write
            };
            let result = app.apply_migration(
                &target,
                &NamespaceMigration {
                    manifest: [4; 32],
                    sources: &[(upload().namespace, head)],
                },
            );
            assert!(
                matches!(result, Err(AppError::Storage(message)) if message.contains("content-aware"))
            );
            assert_eq!(app.head(&target.namespace).unwrap(), None);
            assert_eq!(app.migration_target(&upload().namespace).unwrap(), None);
            store.write_part(upload(), 0, b"abcd").unwrap();
            assert!(
                store
                    .verify(upload(), Hash::new())
                    .unwrap()
                    .step(1)
                    .unwrap()
                    .is_some()
            );
        }
    }
}

#[test]
#[cfg(feature = "backend-ssd")]
fn legacy_archive_import_cannot_fence_existing_uploads_at_its_destination() {
    use crate::storage::application::{
        ApplicationStore, Error as AppError, Head, TransferSource, Write,
    };
    for backend in [
        #[cfg(feature = "backend-ssd")]
        Backend::Ssd,
        #[cfg(feature = "backend-hdd")]
        Backend::Hdd,
    ] {
        let temp = Temp::new();
        let source_path = temp.0.join("source");
        let app =
            ApplicationStore::from_database(Database::open(&source_path, Backend::Ssd).unwrap());
        app.apply(&Write {
            namespace: upload().namespace,
            request: [8; 32],
            fingerprint: [7; 32],
            expected: None,
            head: Head {
                index: 0,
                commit: [9; 32],
            },
            content: &[([9; 32], b"head".to_vec())],
            claims: &[],
        })
        .unwrap();
        drop(app);
        let source = TransferSource::open(&source_path, [3; 32], [4; 32]).unwrap();
        let db = Database::open(temp.0.join("target"), backend).unwrap();
        let target = ApplicationStore::from_database(db.clone());
        let store = ContentStore::from_database(db);
        store.begin(upload(), spec()).unwrap();
        assert!(matches!(source.stage(&target, 1, |_, _, _| Ok(())),
            Err(AppError::Storage(message)) if message.contains("content-aware")));
        assert_eq!(target.head(&upload().namespace).unwrap(), None);
        store.write_part(upload(), 0, b"abcd").unwrap();
        assert!(
            store
                .verify(upload(), Hash::new())
                .unwrap()
                .step(1)
                .unwrap()
                .is_some()
        );
    }
}

#[cfg(feature = "backend-hdd")]
mod faults {
    use super::*;
    use crate::storage::application::{ApplicationStore, Head, Write};
    use ::redb::{StorageBackend, backends::FileBackend};
    use std::sync::{Arc, atomic::AtomicU8};

    #[derive(Debug)]
    struct Fault {
        file: FileBackend,
        mode: Arc<AtomicU8>,
    }
    impl Fault {
        fn trip(&self, mode: u8) -> std::io::Result<()> {
            if self
                .mode
                .compare_exchange(mode, 0, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                Err(std::io::Error::other("injected content durability failure"))
            } else {
                Ok(())
            }
        }
    }
    impl StorageBackend for Fault {
        fn len(&self) -> std::io::Result<u64> {
            self.file.len()
        }
        fn read(&self, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
            self.file.read(offset, len)
        }
        fn set_len(&self, len: u64) -> std::io::Result<()> {
            self.file.set_len(len)
        }
        fn write(&self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
            self.trip(1)?;
            self.file.write(offset, bytes)
        }
        fn sync_data(&self, eventual: bool) -> std::io::Result<()> {
            self.trip(2)?;
            self.file.sync_data(eventual)?;
            self.trip(3)
        }
    }

    #[test]
    fn uncertain_part_seal_and_head_commits_freeze_every_view_and_recover_atomically() {
        fn failed_content<T: std::fmt::Debug>(result: Result<T>) -> bool {
            match result {
                Err(Error::Storage(StorageError::CommitUnknown { .. })) => true,
                Err(Error::Storage(StorageError::Io(_))) => false,
                other => panic!("unexpected injected content outcome: {other:?}"),
            }
        }
        for mode in 1..=3 {
            for phase in 0..3 {
                let temp = Temp::new();
                let path = temp.0.join("db");
                let armed = Arc::new(AtomicU8::new(0));
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .unwrap();
                let raw = ::redb::Database::builder()
                    .set_cache_size(0)
                    .create_with_backend(Fault {
                        file: FileBackend::new(file).unwrap(),
                        mode: armed.clone(),
                    })
                    .unwrap();
                let db = Database::from_redb(raw);
                let store = ContentStore::from_database(db.clone());
                let app = ApplicationStore::from_database(db.clone());
                store.begin(upload(), spec()).unwrap();
                if phase > 0 {
                    store.write_part(upload(), 0, b"abcd").unwrap();
                }
                if phase > 1 {
                    store
                        .verify(upload(), Hash::new())
                        .unwrap()
                        .step(1)
                        .unwrap();
                }
                let prior = db.last_transaction().unwrap();
                armed.store(mode, Ordering::SeqCst);
                let uncertain = match phase {
                    0 => failed_content(store.write_part(upload(), 0, b"abcd")),
                    1 => failed_content(store.verify(upload(), Hash::new()).unwrap().step(1)),
                    _ => match app.apply_with(
                        &Write {
                            namespace: [1; 32],
                            request: [8; 32],
                            fingerprint: [9; 32],
                            expected: None,
                            head: Head {
                                index: 0,
                                commit: [6; 32],
                            },
                            content: &[([6; 32], b"head".to_vec())],
                            claims: &[],
                        },
                        |tx| {
                            tx.retain_content([1; 32], spec().particle, [7; 32], [6; 32])
                                .unwrap();
                            Ok(())
                        },
                    ) {
                        Err(crate::storage::application::Error::CommitUnknown(_)) => true,
                        Err(crate::storage::application::Error::Storage(_)) => false,
                        other => panic!("unexpected injected publication outcome: {other:?}"),
                    },
                };
                assert_eq!(armed.load(Ordering::SeqCst), 0);
                assert_eq!(db.is_poisoned(), uncertain);
                if uncertain {
                    assert!(matches!(
                        store.progress(upload()),
                        Err(Error::Storage(StorageError::CommitUnknown { .. }))
                    ));
                    assert!(app.head(&[1; 32]).is_err());
                } else {
                    assert_eq!(
                        mode, 1,
                        "only writes before commit can have a known rejection"
                    );
                }
                drop(app);
                drop(store);
                drop(db);
                let db = Database::open(&path, Backend::Hdd).unwrap();
                let store = ContentStore::from_database(db.clone());
                let progress = store.progress(upload()).unwrap().unwrap();
                let present = store.coverage(upload(), 0, 1).unwrap().present[0].1;
                assert_eq!(progress.present_parts, u64::from(present));
                if present {
                    assert_eq!(
                        db.read(Table::Parts, &part_key(upload(), 0), 4)
                            .unwrap()
                            .unwrap(),
                        b"abcd"
                    );
                }
                let descriptor = store.file([1; 32], spec().particle).unwrap();
                assert_eq!(descriptor.is_some(), progress.state == State::Sealed);
                if descriptor.is_some() {
                    assert_eq!(
                        store
                            .read_range([1; 32], spec().particle, [7; 32], 0, 4)
                            .unwrap(),
                        b"abcd"
                    );
                }
                let head = ApplicationStore::from_database(db.clone())
                    .head(&[1; 32])
                    .unwrap();
                assert_eq!(
                    head.is_some(),
                    store
                        .is_retained([1; 32], spec().particle, [7; 32], [6; 32])
                        .unwrap()
                );
                if head.is_some() {
                    assert!(descriptor.is_some());
                }
                let committed = match phase {
                    0 => present,
                    1 => descriptor.is_some(),
                    _ => head.is_some(),
                };
                if !uncertain {
                    assert!(!committed);
                    assert_eq!(db.last_transaction().unwrap(), prior);
                }
                if mode == 3 {
                    assert!(
                        committed,
                        "completed durability barrier must survive reopen"
                    );
                }
            }
        }
    }
}
