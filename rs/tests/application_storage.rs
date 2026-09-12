#![cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
use bbg::storage::application::{ApplicationStore, Error, Head, Write};
use bbg::storage::database::{Backend, Database};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct StorePath(PathBuf, Backend);
impl StorePath {
    fn new(backend: Backend) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bbg-app-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root, backend)
    }
    fn open(&self) -> ApplicationStore {
        ApplicationStore::from_database(Database::open(self.0.join("graph"), self.1).unwrap())
    }
}
impl Drop for StorePath {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn committed_head_content_and_request_survive_reopen_for(backend: Backend) {
    let path = StorePath::new(backend);
    let head = Head {
        index: 0,
        commit: [2; 32],
    };
    let content = [([2; 32], b"birth".to_vec())];
    let write = Write {
        namespace: [1; 32],
        request: [3; 32],
        fingerprint: [4; 32],
        expected: None,
        head,
        content: &content,
        claims: &[],
    };
    {
        let store = path.open();
        assert_eq!(store.apply(&write).unwrap(), head);
    }
    let store = path.open();
    assert_eq!(store.head(&[1; 32]).unwrap(), Some(head));
    assert_eq!(
        store.content(&[2; 32], 16).unwrap(),
        Some(b"birth".to_vec())
    );
    assert_eq!(
        store.resolve(&[1; 32], &[3; 32]).unwrap(),
        Some(([4; 32], head))
    );
    assert_eq!(store.apply(&write).unwrap(), head);
    assert_eq!(store.history(&[1; 32], None, 5).unwrap(), vec![head]);
}

fn rejection_does_not_publish_staged_bytes_or_advance_history_for(backend: Backend) {
    let path = StorePath::new(backend);
    let store = path.open();
    let birth = Head {
        index: 0,
        commit: [2; 32],
    };
    let content = [([2; 32], b"birth".to_vec())];
    store
        .apply(&Write {
            namespace: [1; 32],
            request: [3; 32],
            fingerprint: [4; 32],
            expected: None,
            head: birth,
            content: &content,
            claims: &[],
        })
        .unwrap();
    let changed = [([8; 32], b"staged".to_vec()), ([2; 32], b"forged".to_vec())];
    let attempt = Write {
        namespace: [1; 32],
        request: [5; 32],
        fingerprint: [6; 32],
        expected: Some(birth),
        head: Head {
            index: 1,
            commit: [8; 32],
        },
        content: &changed,
        claims: &[],
    };
    assert!(matches!(store.apply(&attempt), Err(Error::Conflict)));
    assert_eq!(store.head(&[1; 32]).unwrap(), Some(birth));
    assert!(store.content(&[8; 32], 16).unwrap().is_none());
    assert!(store.resolve(&[1; 32], &[5; 32]).unwrap().is_none());
}

fn head_races_and_changed_retries_are_conflicts_for(backend: Backend) {
    let path = StorePath::new(backend);
    let store = path.open();
    let content = [([2; 32], vec![1])];
    let mut write = Write {
        namespace: [1; 32],
        request: [3; 32],
        fingerprint: [4; 32],
        expected: None,
        head: Head {
            index: 0,
            commit: [2; 32],
        },
        content: &content,
        claims: &[],
    };
    store.apply(&write).unwrap();
    write.fingerprint = [5; 32];
    assert!(matches!(store.apply(&write), Err(Error::Conflict)));
    write.request = [6; 32];
    assert!(matches!(store.apply(&write), Err(Error::HeadMismatch)));
    assert!(matches!(store.content(&[2; 32], 0), Err(Error::Limit)));
    assert!(matches!(
        store.history(&[1; 32], None, usize::MAX),
        Err(Error::Limit)
    ));
}

fn unique_claims_cover_namespaces_and_rollback_with_rejected_content_for(backend: Backend) {
    let path = StorePath::new(backend);
    let store = path.open();
    let content = [([2; 32], b"birth".to_vec())];
    let claims = [([10; 32], [11; 32])];
    let mut write = Write {
        namespace: [1; 32],
        request: [3; 32],
        fingerprint: [4; 32],
        expected: None,
        head: Head {
            index: 0,
            commit: [2; 32],
        },
        content: &content,
        claims: &claims,
    };
    store.apply(&write).unwrap();
    let changed = [([10; 32], [12; 32])];
    write.namespace = [5; 32];
    write.claims = &changed;
    assert!(matches!(store.apply(&write), Err(Error::Conflict)));
    assert!(store.head(&[5; 32]).unwrap().is_none());
    let new_claim = [([20; 32], [21; 32])];
    write.claims = &new_claim;
    let forged = [([2; 32], b"forged".to_vec())];
    write.content = &forged;
    assert!(matches!(store.apply(&write), Err(Error::Conflict)));
    let different = [([20; 32], [22; 32])];
    write.content = &content;
    write.claims = &different;
    store.apply(&write).unwrap();
}

fn simultaneous_writers_cannot_both_advance_the_same_head_for(backend: Backend) {
    let path = StorePath::new(backend);
    let store = path.open();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let write = |n: u8| {
            barrier.wait();
            store.apply(&Write {
                namespace: [1; 32],
                request: [n; 32],
                fingerprint: [n; 32],
                expected: None,
                head: Head {
                    index: 0,
                    commit: [n; 32],
                },
                content: &[([n; 32], vec![n])],
                claims: &[],
            })
        };
        let a = scope.spawn(move || write(2));
        let b = scope.spawn(move || write(3));
        let results = [a.join().unwrap(), b.join().unwrap()];
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, Err(Error::HeadMismatch)))
                .count(),
            1
        );
    });
    assert_eq!(store.history(&[1; 32], None, 10).unwrap().len(), 1);
}

fn backends() -> Vec<Backend> {
    vec![
        #[cfg(feature = "backend-ssd")]
        Backend::Ssd,
        #[cfg(feature = "backend-hdd")]
        Backend::Hdd,
    ]
}

#[test]
fn committed_head_content_and_request_survive_reopen() {
    for backend in backends() {
        committed_head_content_and_request_survive_reopen_for(backend);
    }
}

#[test]
fn rejection_does_not_publish_staged_bytes_or_advance_history() {
    for backend in backends() {
        rejection_does_not_publish_staged_bytes_or_advance_history_for(backend);
    }
}

#[test]
fn head_races_and_changed_retries_are_conflicts() {
    for backend in backends() {
        head_races_and_changed_retries_are_conflicts_for(backend);
    }
}

#[test]
fn unique_claims_cover_namespaces_and_rollback_with_rejected_content() {
    for backend in backends() {
        unique_claims_cover_namespaces_and_rollback_with_rejected_content_for(backend);
    }
}

#[test]
fn simultaneous_writers_cannot_both_advance_the_same_head() {
    for backend in backends() {
        simultaneous_writers_cannot_both_advance_the_same_head_for(backend);
    }
}
