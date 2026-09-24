#![cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
use bbg::storage::{
    StorageError,
    application::{ApplicationStore, Error as AppError, Head, Write},
    content::{ContentStore, Error, FileInfo, Result, Spec, State, Upload, Verifier},
    database::{Backend, Database},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-content-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn db(&self, backend: Backend) -> Database {
        Database::open(self.0.join("store"), backend).unwrap()
    }
}
impl Drop for Temp {
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
struct Hash(hemera::Hasher);
impl Hash {
    fn new() -> Self {
        Self(hemera::Hasher::new())
    }
}
impl Verifier for Hash {
    fn profile(&self) -> [u8; 32] {
        [7; 32]
    }
    fn update(&mut self, bytes: &[u8]) -> Result<()> {
        self.0.update(bytes);
        Ok(())
    }
    fn finish(self) -> Result<[u8; 32]> {
        Ok(*self.0.finalize().as_bytes())
    }
}
fn spec(bytes: &[u8], part_bytes: u32) -> Spec {
    Spec {
        particle: *hemera::hash(bytes).as_bytes(),
        profile: [7; 32],
        length: bytes.len() as u64,
        part_bytes,
    }
}
fn upload(n: u8) -> Upload {
    Upload {
        namespace: [1; 32],
        request: [n; 32],
    }
}
fn stage(store: &ContentStore, id: Upload, bytes: &[u8], size: u32) -> Spec {
    let spec = spec(bytes, size);
    store.begin(id, spec).unwrap();
    for (index, bytes) in bytes.chunks(size as usize).enumerate() {
        store.write_part(id, index as u64, bytes).unwrap();
    }
    spec
}
fn seal(store: &ContentStore, id: Upload) -> FileInfo {
    let mut verifier = store.verify(id, Hash::new()).unwrap();
    loop {
        if let Some(info) = verifier.step(1).unwrap() {
            return info;
        }
    }
}

#[test]
fn out_of_order_upload_resumes_after_reopen_and_rejects_conflicting_retries() {
    for backend in backends() {
        let temp = Temp::new();
        let bytes = b"binary\0file-with-tail";
        let spec = spec(bytes, 7);
        {
            let db = temp.db(backend);
            let store = ContentStore::from_database(db.clone());
            store.begin(upload(2), spec).unwrap();
            store.write_part(upload(2), 1, &bytes[7..14]).unwrap();
            let marker = db.last_transaction().unwrap();
            store.write_part(upload(2), 1, &bytes[7..14]).unwrap();
            assert_eq!(db.last_transaction().unwrap(), marker);
            assert_eq!(
                store.write_part(upload(2), 1, b"changed"),
                Err(Error::Conflict)
            );
            assert_eq!(
                store.coverage(upload(2), 0, 2).unwrap().present,
                vec![(0, false), (1, true)]
            );
            assert!(matches!(
                store.verify(upload(2), Hash::new()),
                Err(Error::Incomplete)
            ));
            assert!(store.file([1; 32], spec.particle).unwrap().is_none());
        }
        let store = ContentStore::from_database(temp.db(backend));
        assert_eq!(store.begin(upload(2), spec).unwrap().present_parts, 1);
        store.write_part(upload(2), 0, &bytes[..7]).unwrap();
        store.write_part(upload(2), 2, &bytes[14..]).unwrap();
        let mut verifier = store.verify(upload(2), Hash::new()).unwrap();
        assert!(verifier.step(1).unwrap().is_none());
        assert_eq!(verifier.verified_parts(), 1);
        drop(verifier);
        assert_eq!(seal(&store, upload(2)).spec, spec);
        assert_eq!(
            store
                .read_range([1; 32], spec.particle, [7; 32], 5, 10)
                .unwrap(),
            &bytes[5..15]
        );
        assert_eq!(
            store.read_range([2; 32], spec.particle, [7; 32], 0, 1),
            Err(Error::Missing)
        );
        assert_eq!(
            store.read_range([1; 32], spec.particle, [8; 32], 0, 1),
            Err(Error::ProfileMismatch)
        );
        assert_eq!(store.cancel(upload(2), 1), Err(Error::Conflict));
    }
}

#[test]
fn canonical_identity_is_checked_and_cancelled_sessions_cannot_reappear() {
    for backend in backends() {
        let temp = Temp::new();
        let store = ContentStore::from_database(temp.db(backend));
        let mut expected = spec(b"abcdef", 2);
        expected.particle = [9; 32];
        store.begin(upload(1), expected).unwrap();
        for (n, part) in b"abcdef".chunks(2).enumerate() {
            store.write_part(upload(1), n as u64, part).unwrap();
        }
        let mut verifier = store.verify(upload(1), Hash::new()).unwrap();
        assert_eq!(verifier.step(3), Err(Error::IdentityMismatch));
        assert!(store.file([1; 32], expected.particle).unwrap().is_none());
        let mut racing = store.verify(upload(1), Hash::new()).unwrap();
        assert!(racing.step(1).unwrap().is_none());
        assert!(!store.cancel(upload(1), 1).unwrap());
        assert_eq!(racing.step(3), Err(Error::Cancelled));
        drop(racing);
        drop(verifier);
        drop(store);
        let store = ContentStore::from_database(temp.db(backend));
        assert_eq!(store.begin(upload(1), expected), Err(Error::Cancelled));
        assert_eq!(store.write_part(upload(1), 0, b"ab"), Err(Error::Cancelled));
        assert!(store.cancel(upload(1), 10).unwrap());
        assert!(store.cancel(upload(1), 10).unwrap());
        assert_eq!(
            store.progress(upload(1)).unwrap().unwrap().state,
            State::Cancelled
        );
    }
}

#[test]
fn empty_and_duplicate_files_share_identity_across_physical_part_sizes() {
    for backend in backends() {
        let temp = Temp::new();
        let store = ContentStore::from_database(temp.db(backend));
        let empty = stage(&store, upload(1), b"", 4);
        seal(&store, upload(1));
        assert!(
            store
                .read_range([1; 32], empty.particle, [7; 32], 0, 9)
                .unwrap()
                .is_empty()
        );
        let first = stage(&store, upload(2), b"the same data", 3);
        let info = seal(&store, upload(2));
        stage(&store, upload(3), b"the same data", 7);
        assert_eq!(seal(&store, upload(3)), info);
        assert!(store.cancel(upload(3), 10).unwrap());
        assert_eq!(
            store
                .read_range([1; 32], first.particle, [7; 32], 0, 100)
                .unwrap(),
            b"the same data"
        );
    }
}

#[test]
fn namespace_uploads_are_paged_without_a_collection_limit() {
    for backend in backends() {
        let temp = Temp::new();
        let store = ContentStore::from_database(temp.db(backend));
        for n in 0..137 {
            store.begin(upload(n), spec(b"", 1)).unwrap();
        }
        let mut after = None;
        let mut seen = Vec::new();
        loop {
            let page = store.uploads([1; 32], after, 11).unwrap();
            if page.is_empty() {
                break;
            }
            after = Some(page.last().unwrap().0.request);
            seen.extend(page.into_iter().map(|(id, _)| id.request));
        }
        assert_eq!(seen.len(), 137);
        assert!(seen.windows(2).all(|w| w[0] < w[1]));
        assert!(store.uploads([2; 32], None, 11).unwrap().is_empty());
        assert!(matches!(
            store.uploads([1; 32], None, 0),
            Err(Error::Storage(StorageError::Limit(_)))
        ));
    }
}

#[test]
fn retained_content_and_application_head_publish_or_rollback_together() {
    for backend in backends() {
        let temp = Temp::new();
        let db = temp.db(backend);
        let store = ContentStore::from_database(db.clone());
        let file = stage(&store, upload(2), b"protected", 3);
        seal(&store, upload(2));
        let app = ApplicationStore::from_database(db.clone());
        let content = [([6; 32], b"head".to_vec())];
        let write = Write {
            namespace: [1; 32],
            request: [3; 32],
            fingerprint: [4; 32],
            expected: None,
            head: Head {
                index: 0,
                commit: [6; 32],
            },
            content: &content,
            claims: &[],
        };
        let result = app.apply_with(&write, |tx| {
            tx.retain_content([1; 32], file.particle, [7; 32], [6; 32])
                .unwrap();
            Err(AppError::Conflict)
        });
        assert!(matches!(result, Err(AppError::Conflict)));
        assert!(app.head(&[1; 32]).unwrap().is_none());
        assert!(
            !store
                .is_retained([1; 32], file.particle, [7; 32], [6; 32])
                .unwrap()
        );
        app.apply_with(&write, |tx| {
            tx.retain_content([1; 32], file.particle, [7; 32], [6; 32])
                .unwrap();
            Ok(())
        })
        .unwrap();
        let marker = db.last_transaction().unwrap();
        app.apply_with(&write, |_| panic!("duplicate transition"))
            .unwrap();
        assert_eq!(db.last_transaction().unwrap(), marker);
        drop(app);
        drop(store);
        drop(db);
        let db = temp.db(backend);
        assert!(
            ContentStore::from_database(db.clone())
                .is_retained([1; 32], file.particle, [7; 32], [6; 32])
                .unwrap()
        );
        assert_eq!(
            ApplicationStore::from_database(db).head(&[1; 32]).unwrap(),
            Some(write.head)
        );
    }
}

#[test]
fn process_exit_without_destructors_preserves_acknowledged_content() {
    for backend in backends() {
        for phase in 1..=4 {
            let temp = Temp::new();
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "content_crash_child", "--nocapture"])
                .env("BBG_CONTENT_CRASH_ROOT", &temp.0)
                .env(
                    "BBG_CONTENT_CRASH_BACKEND",
                    if backend == Backend::Ssd {
                        "ssd"
                    } else {
                        "hdd"
                    },
                )
                .env("BBG_CONTENT_CRASH_PHASE", phase.to_string())
                .output()
                .unwrap();
            assert_eq!(
                result.status.code(),
                Some(23),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let db = temp.db(backend);
            let store = ContentStore::from_database(db.clone());
            let expected = spec(b"abc", 1);
            let progress = store.progress(upload(1)).unwrap().unwrap();
            assert_eq!(progress.present_parts, if phase == 1 { 1 } else { 3 });
            assert_eq!(
                store.file([1; 32], expected.particle).unwrap().is_some(),
                phase >= 3
            );
            let app = ApplicationStore::from_database(db);
            assert_eq!(app.head(&[1; 32]).unwrap().is_some(), phase == 4);
            assert_eq!(
                store
                    .is_retained([1; 32], expected.particle, [7; 32], [6; 32])
                    .unwrap(),
                phase == 4
            );
            for (index, bytes) in b"abc".chunks(1).enumerate() {
                store.write_part(upload(1), index as u64, bytes).unwrap();
            }
            seal(&store, upload(1));
            assert_eq!(
                store
                    .read_range([1; 32], expected.particle, [7; 32], 0, 3)
                    .unwrap(),
                b"abc"
            );
        }
    }
}

#[test]
fn content_crash_child() {
    let Ok(root) = std::env::var("BBG_CONTENT_CRASH_ROOT") else {
        return;
    };
    let backend = if std::env::var("BBG_CONTENT_CRASH_BACKEND").unwrap() == "ssd" {
        Backend::Ssd
    } else {
        Backend::Hdd
    };
    let phase: u8 = std::env::var("BBG_CONTENT_CRASH_PHASE")
        .unwrap()
        .parse()
        .unwrap();
    let db = Database::open(PathBuf::from(root).join("store"), backend).unwrap();
    let store = ContentStore::from_database(db.clone());
    let expected = spec(b"abc", 1);
    store.begin(upload(1), expected).unwrap();
    store.write_part(upload(1), 0, b"a").unwrap();
    if phase == 1 {
        std::process::exit(23);
    }
    store.write_part(upload(1), 1, b"b").unwrap();
    store.write_part(upload(1), 2, b"c").unwrap();
    let mut verification = store.verify(upload(1), Hash::new()).unwrap();
    assert!(verification.step(1).unwrap().is_none());
    if phase == 2 {
        std::process::exit(23);
    }
    assert!(verification.step(2).unwrap().is_some());
    if phase == 3 {
        std::process::exit(23);
    }
    let app = ApplicationStore::from_database(db);
    app.apply_with(
        &Write {
            namespace: [1; 32],
            request: [3; 32],
            fingerprint: [4; 32],
            expected: None,
            head: Head {
                index: 0,
                commit: [6; 32],
            },
            content: &[([6; 32], b"head".to_vec())],
            claims: &[],
        },
        |tx| {
            tx.retain_content([1; 32], expected.particle, [7; 32], [6; 32])
                .unwrap();
            Ok(())
        },
    )
    .unwrap();
    std::process::exit(23);
}

#[test]
#[cfg(feature = "backend-ssd")]
fn legacy_application_archive_refuses_to_omit_streamed_content() {
    let temp = Temp::new();
    {
        let db = temp.db(Backend::Ssd);
        let store = ContentStore::from_database(db.clone());
        stage(&store, upload(1), b"keep me", 4);
        let app = ApplicationStore::from_database(db);
        app.apply(&Write {
            namespace: [1; 32],
            request: [3; 32],
            fingerprint: [4; 32],
            expected: None,
            head: Head {
                index: 0,
                commit: [6; 32],
            },
            content: &[([6; 32], b"head".to_vec())],
            claims: &[],
        })
        .unwrap();
    }
    assert!(
        matches!(bbg::storage::application::ApplicationArchive::open(&temp.0.join("store")),
        Err(AppError::Storage(message)) if message.contains("content-aware archive"))
    );
}
