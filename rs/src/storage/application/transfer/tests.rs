use super::*;
use crate::storage::application::{NamespaceMigration, Write};
use std::sync::atomic::{AtomicU64, Ordering};
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bbg-transfer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> ApplicationStore {
        ApplicationStore::open(self.0.join("db")).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn publish(store: &ApplicationStore, namespace: Particle, n: u8) -> Head {
    let expected = store.head(&namespace).unwrap();
    let head = Head {
        index: expected.map_or(0, |h| h.index + 1),
        commit: [n; 32],
    };
    store
        .apply(&Write {
            namespace,
            request: [n; 32],
            fingerprint: [n; 32],
            expected,
            head,
            content: &[(head.commit, vec![1, n])],
            claims: &[],
        })
        .unwrap()
}
#[test]
fn interrupted_validation_does_not_advance_cursor_or_copy_a_partial_page() {
    let source = Directory::new();
    let destination = Directory::new();
    {
        let db = source.open();
        publish(&db, [1; 32], 2);
        publish(&db, [1; 32], 3);
    }
    {
        let transfer = TransferSource::open(&source.0.join("db"), [9; 32], [8; 32]).unwrap();
        let db = destination.open();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| transfer.stage(
                &db,
                1,
                |_, _, _| panic!("crash before page commit")
            )))
            .is_err()
        );
        assert!(db.content(&[2; 32], 10).unwrap().is_none());
        let c = db
            .db
            .read(Table::Migration, &cursor_key(&transfer.manifest), 82)
            .unwrap()
            .unwrap();
        assert_eq!(Cursor::decode(&c).unwrap().rows, 0);
        let h = transfer.store.head(&[1; 32]).unwrap().unwrap();
        assert!(matches!(
            transfer.store.apply(&Write {
                namespace: [1; 32],
                request: [4; 32],
                fingerprint: [4; 32],
                expected: Some(h),
                head: Head {
                    index: h.index + 1,
                    commit: [4; 32]
                },
                content: &[([4; 32], vec![1, 4])],
                claims: &[]
            }),
            Err(Error::Fenced)
        ));
    }
    assert!(source.open_error());
    let transfer = TransferSource::open(&source.0.join("db"), [9; 32], [8; 32]).unwrap();
    let db = destination.open();
    let done = transfer.stage(&db, 32, |_, _, _| Ok(())).unwrap();
    assert!(done.complete);
    assert_eq!(
        db.head(&[1; 32]).unwrap(),
        transfer.store.head(&[1; 32]).unwrap()
    );
}
impl Directory {
    fn open_error(&self) -> bool {
        ApplicationStore::open(self.0.join("db")).is_err()
    }
}

#[test]
fn activation_requires_complete_transfer_and_the_pinned_target() {
    use crate::storage::database::ReaderGeneration;
    let source = Directory::new();
    let destination = Directory::new();
    let head = {
        let db = source.open();
        publish(&db, [1; 32], 2)
    };
    let transfer = TransferSource::open(&source.0.join("db"), [9; 32], [8; 32]).unwrap();
    let db = destination.open();
    db.db
        .require_reader_generation(ReaderGeneration::AuthenticatedV1)
        .unwrap();
    // Four tables copied/advanced, then selected head copied; final validation
    // has not run yet. Legacy reads may inspect it, but writers cannot activate.
    while db.head(&[1; 32]).unwrap().is_none() {
        transfer.stage(&db, 1, |_, _, _| Ok(())).unwrap();
    }
    assert_eq!(db.head(&[1; 32]).unwrap(), Some(head));
    let write = Write {
        namespace: [9; 32],
        request: [10; 32],
        fingerprint: [10; 32],
        expected: None,
        head: Head {
            index: 0,
            commit: [10; 32],
        },
        content: &[([10; 32], vec![1, 10])],
        claims: &[],
    };
    let migration = NamespaceMigration {
        manifest: [11; 32],
        sources: &[([1; 32], head)],
    };
    assert!(matches!(
        db.apply_migration(&write, &migration),
        Err(Error::Fenced)
    ));
    assert!(db.head(&[9; 32]).unwrap().is_none());
    assert!(transfer.stage(&db, 32, |_, _, _| Ok(())).unwrap().complete);
    let wrong = Write {
        namespace: [7; 32],
        ..write
    };
    assert!(matches!(
        db.apply_migration(&wrong, &migration),
        Err(Error::Conflict)
    ));
    db.apply_migration(&write, &migration).unwrap();
    assert_eq!(
        db.db.reader_generation().unwrap(),
        ReaderGeneration::AuthenticatedV1
    );
    assert_eq!(
        db.migration_target(&[1; 32]).unwrap().unwrap().namespace,
        [9; 32]
    );
}

#[test]
fn broken_receipt_coverage_never_completes() {
    let source = Directory::new();
    let destination = Directory::new();
    {
        let db = source.open();
        publish(&db, [1; 32], 2);
        publish(&db, [1; 32], 3);
        db.db
            .transaction::<_, Error>(|tx| {
                tx.remove(Table::History, &super::super::history_key(&[1; 32], 0))?;
                Ok(())
            })
            .unwrap();
    }
    let transfer = TransferSource::open(&source.0.join("db"), [9; 32], [8; 32]).unwrap();
    let db = destination.open();
    assert!(matches!(
        transfer.stage(&db, 32, |_, _, _| Ok(())),
        Err(Error::Corrupt)
    ));
    let c = db
        .db
        .read(Table::Migration, &cursor_key(&transfer.manifest), 82)
        .unwrap()
        .unwrap();
    assert_ne!(Cursor::decode(&c).unwrap().table, 5);
}

#[test]
fn archive_inspection_is_read_only_bounded_and_survives_the_seal() {
    let source = Directory::new();
    let before;
    {
        let store = source.open();
        publish(&store, [1; 32], 2);
        publish(&store, [1; 32], 3);
        before = store.db.last_transaction().unwrap();
    }
    let archive = ApplicationArchive::open(&source.0.join("db")).unwrap();
    assert_eq!(archive.last_transaction(), before);
    assert!(archive.seal().is_none());
    assert!(source.open_result().is_err());
    let check = |_: Particle, bytes: &[u8], _: &ApplicationArchive| {
        if bytes.len() == 2 {
            Ok(())
        } else {
            Err(Error::Corrupt)
        }
    };
    let summary = archive.inspect(100, 1_000_000, check).unwrap();
    assert_eq!(summary.tables, [2, 0, 2, 2, 1]);
    assert_eq!(summary.rows, 7);
    assert_eq!(archive.store.db.last_transaction().unwrap(), before);
    assert!(matches!(
        archive.inspect(1, 1_000_000, check),
        Err(Error::Limit)
    ));
    assert!(matches!(archive.inspect(100, 1, check), Err(Error::Limit)));
    assert!(matches!(
        archive.inspect(100, 1_000_000, |_, _, _| Err(Error::Corrupt)),
        Err(Error::Corrupt)
    ));
    assert_eq!(archive.store.db.last_transaction().unwrap(), before);
    let transfer = TransferSource::from_archive(archive, [9; 32], [8; 32]).unwrap();
    let manifest = transfer.manifest();
    drop(transfer);
    assert!(source.open_result().is_err());
    let archive = ApplicationArchive::open(&source.0.join("db")).unwrap();
    let sealed = archive.last_transaction();
    assert_eq!(archive.seal().unwrap().manifest, manifest);
    assert_eq!(archive.seal().unwrap().prior, before.unwrap());
    assert_eq!(archive.inspect(100, 1_000_000, check).unwrap(), summary);
    assert_eq!(archive.store.db.last_transaction().unwrap(), sealed);
}

impl Directory {
    fn open_result(&self) -> Result<ApplicationStore, Error> {
        ApplicationStore::open(self.0.join("db"))
    }
}

#[test]
fn archive_rejects_missing_receipt_and_empty_path_without_a_source_transaction() {
    let source = Directory::new();
    let empty = source.0.join("empty");
    std::fs::create_dir(&empty).unwrap();
    assert!(ApplicationArchive::open(&empty).is_err());
    assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
    let absent = source.0.join("absent");
    assert!(ApplicationArchive::open(&absent).is_err());
    assert!(!absent.exists());
    let before;
    {
        let store = source.open();
        publish(&store, [1; 32], 2);
        publish(&store, [1; 32], 3);
        store
            .db
            .transaction::<_, Error>(|tx| {
                tx.remove(Table::Requests, &[vec![1; 32], vec![3; 32]].concat())?;
                Ok(())
            })
            .unwrap();
        before = store.db.last_transaction().unwrap();
    }
    let archive = ApplicationArchive::open(&source.0.join("db")).unwrap();
    assert!(matches!(
        archive.inspect(100, 1_000_000, |_, _, _| Ok(())),
        Err(Error::Corrupt)
    ));
    assert_eq!(archive.store.db.last_transaction().unwrap(), before);
    assert!(archive.seal().is_none());
}

fn kill_ready() {
    use std::io::Write;
    println!("BBG_TRANSFER_KILL_READY");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn transfer_child() {
    let Ok(source) = std::env::var("BBG_TRANSFER_SOURCE") else {
        return;
    };
    let target = std::env::var("BBG_TRANSFER_TARGET").unwrap();
    let phase = std::env::var("BBG_TRANSFER_PHASE").unwrap();
    let transfer = TransferSource::open(Path::new(&source), [9; 32], [8; 32]).unwrap();
    if phase == "sealed" {
        kill_ready();
    }
    let target = ApplicationStore::open(target).unwrap();
    let progress = transfer
        .stage(
            &target,
            if phase == "complete" { 32 } else { 1 },
            |_, _, _| {
                if phase == "prepared" {
                    kill_ready();
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(progress.complete, phase == "complete");
    kill_ready();
}

#[test]
fn killed_transfer_resumes_source_seal_target_reservation_pages_and_completion() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    for phase in ["sealed", "prepared", "page", "complete"] {
        let source = Directory::new();
        let target = Directory::new();
        let head;
        {
            let store = source.open();
            publish(&store, [1; 32], 2);
            head = publish(&store, [1; 32], 3);
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::application::transfer::tests::transfer_child",
                "--nocapture",
            ])
            .env("BBG_TRANSFER_SOURCE", source.0.join("db"))
            .env("BBG_TRANSFER_TARGET", target.0.join("db"))
            .env("BBG_TRANSFER_PHASE", phase)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let ready = BufReader::new(stdout)
                .lines()
                .any(|line| line.is_ok_and(|line| line.contains("BBG_TRANSFER_KILL_READY")));
            let _ = send.send(ready);
        });
        let ready = receive.recv_timeout(std::time::Duration::from_secs(20));
        let _ = child.kill();
        let status = child.wait().unwrap();
        reader.join().unwrap();
        assert_eq!(ready, Ok(true), "child did not reach {phase}");
        assert!(!status.success());
        assert!(source.open_result().is_err());
        let transfer = TransferSource::open(&source.0.join("db"), [9; 32], [8; 32]).unwrap();
        assert_eq!(transfer.sources(), &[([1; 32], head)]);
        let target = target.open();
        let cursor = target
            .db
            .read(Table::Migration, &cursor_key(&transfer.manifest()), 82)
            .unwrap();
        if phase == "sealed" {
            assert!(cursor.is_none());
        } else {
            let cursor = Cursor::decode(&cursor.unwrap()).unwrap();
            assert_eq!(cursor.table == 5, phase == "complete");
            assert_eq!(cursor.rows == 0, phase == "prepared");
        }
        assert_eq!(
            target.head(&[1; 32]).unwrap().is_some(),
            phase == "complete"
        );
        let result = transfer.stage(&target, 32, |_, _, _| Ok(())).unwrap();
        assert!(result.complete);
        assert_eq!(
            transfer.stage(&target, 32, |_, _, _| Ok(())).unwrap(),
            result
        );
        assert_eq!(target.head(&[1; 32]).unwrap(), Some(head));
        assert_eq!(target.content(&[2; 32], 10).unwrap(), Some(vec![1, 2]));
        assert_eq!(target.resolve(&[1; 32], &[3; 32]).unwrap().unwrap().1, head);
        assert_eq!(transfer.store().head(&[1; 32]).unwrap(), Some(head));
    }
}
