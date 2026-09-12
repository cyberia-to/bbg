#![cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
use bbg::storage::{
    ShardStore, StorageError,
    application::{ApplicationStore, Error, Head, Write},
    database::{Backend, Database},
};
use nebu::Goldilocks;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-shared-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("database")
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
fn write<'a>(content: &'a [([u8; 32], Vec<u8>)], index: u64) -> Write<'a> {
    Write {
        namespace: [1; 32],
        request: [index as u8 + 10; 32],
        fingerprint: [index as u8 + 20; 32],
        expected: (index > 0).then_some(Head {
            index: index.saturating_sub(1),
            commit: [index as u8 + 1; 32],
        }),
        head: Head {
            index,
            commit: [index as u8 + 2; 32],
        },
        content,
        claims: &[],
    }
}
fn open_app(path: &Path, backend: Backend) -> ApplicationStore {
    ApplicationStore::from_database(Database::open(path, backend).unwrap())
}

#[test]
fn applications_and_shards_share_one_owner_and_publication_boundary() {
    for backend in backends() {
        let temp = Temp::new();
        let db = Database::open(temp.path(), backend).unwrap();
        let mut shards = db.shards();
        shards.put(0, [1; 32], vec![Goldilocks::ONE]).unwrap();
        let old = shards.commit().unwrap();
        let app = ApplicationStore::from_database(db.clone());
        let content = [([2; 32], b"birth".to_vec())];
        let request = write(&content, 0);
        app.apply_with(&request, |tx| {
            assert_eq!(tx.read_shard(0, &[1; 32], 1)?, Some(vec![Goldilocks::ONE]));
            tx.remove_shard(0, &[1; 32])?;
            tx.put_shard(1, [2; 32], &[Goldilocks::new(99)])?;
            Ok(())
        })
        .unwrap();
        let combined = db.last_transaction().unwrap().unwrap();
        assert_ne!(combined, old);
        assert_eq!(shards.last_commit().unwrap(), Some(combined));
        assert_eq!(shards.read(0, &[1; 32], 1).unwrap(), None);
        assert_eq!(
            shards.read(1, &[2; 32], 1).unwrap(),
            Some(vec![Goldilocks::new(99)])
        );
        // The lock remains held by the final application view.
        drop(shards);
        drop(db);
        assert!(matches!(
            Database::open(temp.path(), backend),
            Err(StorageError::Busy)
        ));
        drop(app);
        let app = open_app(&temp.path(), backend);
        assert_eq!(app.head(&[1; 32]).unwrap(), Some(request.head));
        assert_eq!(
            app.database().shards().last_commit().unwrap(),
            Some(combined)
        );
        assert_eq!(
            app.resolve(&[1; 32], &request.request).unwrap(),
            Some((request.fingerprint, request.head))
        );
    }
}

#[test]
fn rejected_combined_transition_publishes_neither_domain() {
    for backend in backends() {
        let temp = Temp::new();
        let app = open_app(&temp.path(), backend);
        let content = [([2; 32], b"birth".to_vec())];
        let request = write(&content, 0);
        let error = app.apply_with(&request, |tx| {
            tx.put_shard(0, [1; 32], &[Goldilocks::ONE])?;
            Err(Error::Conflict)
        });
        assert!(matches!(error, Err(Error::Conflict)));
        assert!(app.head(&[1; 32]).unwrap().is_none());
        assert!(app.content(&[2; 32], 100).unwrap().is_none());
        assert!(app.resolve(&[1; 32], &request.request).unwrap().is_none());
        assert!(app.database().last_transaction().unwrap().is_none());
        assert!(
            app.database()
                .shards()
                .read(0, &[1; 32], 1)
                .unwrap()
                .is_none()
        );
        app.apply(&request).unwrap();
    }
}

#[test]
fn old_retry_after_successor_never_executes_shard_transition_again() {
    for backend in backends() {
        let temp = Temp::new();
        let app = open_app(&temp.path(), backend);
        let content = [([2; 32], vec![0])];
        let birth = write(&content, 0);
        app.apply_with(&birth, |tx| {
            tx.put_shard(0, [1; 32], &[Goldilocks::ONE])?;
            Ok(())
        })
        .unwrap();
        let next_content = [([3; 32], vec![1])];
        let successor = write(&next_content, 1);
        app.apply(&successor).unwrap();
        let marker = app.database().last_transaction().unwrap();
        assert_eq!(
            app.apply_with(&birth, |_| panic!("retry executed transition"))
                .unwrap(),
            birth.head
        );
        assert_eq!(app.head(&[1; 32]).unwrap(), Some(successor.head));
        assert_eq!(app.database().last_transaction().unwrap(), marker);
        assert_eq!(
            app.history(&[1; 32], Some(0), 1).unwrap(),
            vec![successor.head]
        );
        assert!(app.history(&[255; 32], None, 1).unwrap().is_empty());
    }
}

#[test]
fn conflicting_claims_across_namespaces_serialize_and_rollback() {
    for backend in backends() {
        let temp = Temp::new();
        let app = open_app(&temp.path(), backend);
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let attempt = |n: u8| {
                barrier.wait();
                let content = [([2; 32], vec![0])];
                let claims = [([99; 32], [n; 32])];
                let mut request = write(&content, 0);
                request.namespace = [n; 32];
                request.claims = &claims;
                app.apply_with(&request, |tx| {
                    tx.put_shard(0, [n; 32], &[Goldilocks::ONE])?;
                    Ok(())
                })
            };
            let a = scope.spawn(move || attempt(1));
            let b = scope.spawn(move || attempt(2));
            let results = [a.join().unwrap(), b.join().unwrap()];
            assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
            assert_eq!(
                results
                    .iter()
                    .filter(|r| matches!(r, Err(Error::Conflict)))
                    .count(),
                1
            );
        });
        for n in [1, 2] {
            assert_eq!(
                app.head(&[n; 32]).unwrap().is_some(),
                app.database()
                    .shards()
                    .read(0, &[n; 32], 1)
                    .unwrap()
                    .is_some()
            );
        }
    }
}

#[test]
fn duplicate_conflicting_content_in_one_batch_is_rejected() {
    for backend in backends() {
        let temp = Temp::new();
        let app = open_app(&temp.path(), backend);
        let content = [([2; 32], vec![1]), ([2; 32], vec![2])];
        assert!(matches!(
            app.apply(&write(&content, 0)),
            Err(Error::Conflict)
        ));
        assert!(app.content(&[2; 32], 10).unwrap().is_none());
    }
}

#[test]
fn opaque_application_values_exceed_shard_value_size_without_field_conversion() {
    for backend in backends() {
        let temp = Temp::new();
        let app = open_app(&temp.path(), backend);
        let content = [([2; 32], vec![255; 2 * 1024 * 1024 + 3])];
        let request = write(&content, 0);
        app.apply(&request).unwrap();
        assert_eq!(
            app.content(&[2; 32], content[0].1.len()).unwrap(),
            Some(content[0].1.clone())
        );
        assert!(matches!(app.content(&[2; 32], 1024), Err(Error::Limit)));
    }
}

fn wait_for_kill() {
    use std::io::Write;
    println!("BBG_SHARED_KILL_READY");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn application_child() {
    let Ok(path) = std::env::var("BBG_SHARED_CHILD_PATH") else {
        return;
    };
    let backend = if std::env::var("BBG_SHARED_CHILD_BACKEND").unwrap() == "ssd" {
        Backend::Ssd
    } else {
        Backend::Hdd
    };
    let phase = std::env::var("BBG_SHARED_CHILD_PHASE").unwrap();
    let app = open_app(Path::new(&path), backend);
    let content = [([2; 32], b"birth".to_vec())];
    let claims = [([90; 32], [91; 32])];
    let mut request = write(&content, 0);
    request.claims = &claims;
    app.apply_with(&request, |tx| {
        tx.remove_shard(0, &[1; 32])?;
        tx.put_shard(1, [2; 32], &[Goldilocks::new(99)])?;
        if phase == "staged" {
            wait_for_kill();
        }
        Ok(())
    })
    .unwrap();
    wait_for_kill();
}

#[test]
fn killed_process_recovers_shards_history_claims_and_receipt_together() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    for backend in backends() {
        for phase in ["staged", "committed"] {
            let temp = Temp::new();
            let old = {
                let db = Database::open(temp.path(), backend).unwrap();
                let mut shards = db.shards();
                shards.put(0, [1; 32], vec![Goldilocks::ONE]).unwrap();
                shards.commit().unwrap()
            };
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "application_child", "--nocapture"])
                .env("BBG_SHARED_CHILD_PATH", temp.path())
                .env(
                    "BBG_SHARED_CHILD_BACKEND",
                    if backend == Backend::Ssd {
                        "ssd"
                    } else {
                        "hdd"
                    },
                )
                .env("BBG_SHARED_CHILD_PHASE", phase)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (send, receive) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                let ready = BufReader::new(stdout)
                    .lines()
                    .any(|line| line.is_ok_and(|line| line.contains("BBG_SHARED_KILL_READY")));
                let _ = send.send(ready);
            });
            let ready = receive.recv_timeout(std::time::Duration::from_secs(20));
            let _ = child.kill();
            assert!(!child.wait().unwrap().success());
            reader.join().unwrap();
            assert_eq!(ready, Ok(true));
            let app = open_app(&temp.path(), backend);
            let committed = phase == "committed";
            assert_eq!(app.head(&[1; 32]).unwrap().is_some(), committed);
            assert_eq!(app.content(&[2; 32], 100).unwrap().is_some(), committed);
            assert_eq!(
                app.resolve(&[1; 32], &[10; 32]).unwrap().is_some(),
                committed
            );
            assert_eq!(
                app.history(&[1; 32], None, 10).unwrap().len(),
                usize::from(committed)
            );
            let shards = app.database().shards();
            assert_eq!(shards.read(0, &[1; 32], 1).unwrap().is_none(), committed);
            assert_eq!(shards.read(1, &[2; 32], 1).unwrap().is_some(), committed);
            assert_eq!(shards.last_commit().unwrap() != Some(old), committed);
            let content = [([3; 32], vec![1])];
            let claims = [([90; 32], [92; 32])];
            let mut competitor = write(&content, 0);
            competitor.namespace = [5; 32];
            competitor.head.commit = [3; 32];
            competitor.claims = &claims;
            assert_eq!(
                matches!(app.apply(&competitor), Err(Error::Conflict)),
                committed
            );
        }
    }
}
