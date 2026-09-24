#![cfg(all(feature = "backend-ssd", feature = "backend-hdd"))]

use bbg::storage::application::{ApplicationStore, Error, Head, Write};
use bbg::storage::database::{Backend, Database};
use redb::{Database as LegacyDatabase, Durability, MultimapTableDefinition, TableDefinition};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type BytesTable = TableDefinition<'static, &'static [u8], &'static [u8]>;
const CONTENT: BytesTable = TableDefinition::new("application_content");
const HEADS: BytesTable = TableDefinition::new("application_heads");
const HISTORY: BytesTable = TableDefinition::new("application_history");
const REQUESTS: BytesTable = TableDefinition::new("application_requests");
const CLAIMS: BytesTable = TableDefinition::new("application_unique_claims");
const TABLES: [BytesTable; 5] = [CONTENT, HEADS, HISTORY, REQUESTS, CLAIMS];

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bbg-migration-{}-{timestamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self) -> PathBuf {
        self.0.join("legacy.redb")
    }
    fn destination(&self) -> PathBuf {
        self.0.join("working")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn id(kind: u8, namespace: u8, index: u64) -> [u8; 32] {
    let mut result = [kind; 32];
    result[1] = namespace;
    result[24..].copy_from_slice(&index.to_be_bytes());
    result
}
fn head(namespace: u8, index: u64) -> Head {
    Head {
        index,
        commit: id(0x40, namespace, index),
    }
}
fn content(namespace: u8, index: u64) -> Vec<u8> {
    let mut bytes = vec![0, 0xff, namespace];
    bytes.extend_from_slice(&index.to_le_bytes());
    bytes
}
fn encode_head(head: Head) -> [u8; 40] {
    let mut result = [0; 40];
    result[..8].copy_from_slice(&head.index.to_le_bytes());
    result[8..].copy_from_slice(&head.commit);
    result
}
fn history_key(namespace: u8, index: u64) -> [u8; 40] {
    let mut result = [namespace; 40];
    result[32..].copy_from_slice(&index.to_be_bytes());
    result
}
fn request_key(namespace: u8, index: u64) -> [u8; 64] {
    let mut result = [namespace; 64];
    result[32..].copy_from_slice(&id(0x70, namespace, index));
    result
}
fn guard_path(path: &Path) -> PathBuf {
    let mut result = path.as_os_str().to_owned();
    result.push(".bbg-importing");
    result.into()
}

/// Write the old wire format directly, without using the replacement API.
fn create_legacy(path: &Path, counts: &[(u8, u64)]) {
    let database = LegacyDatabase::create(path).unwrap();
    let mut transaction = database.begin_write().unwrap();
    transaction.set_durability(Durability::Immediate);
    {
        let mut contents = transaction.open_table(CONTENT).unwrap();
        let mut heads = transaction.open_table(HEADS).unwrap();
        let mut history = transaction.open_table(HISTORY).unwrap();
        let mut requests = transaction.open_table(REQUESTS).unwrap();
        for &(namespace, count) in counts {
            assert!(count > 0);
            for index in 0..count {
                let selected = head(namespace, index);
                contents
                    .insert(
                        selected.commit.as_slice(),
                        content(namespace, index).as_slice(),
                    )
                    .unwrap();
                history
                    .insert(
                        history_key(namespace, index).as_slice(),
                        selected.commit.as_slice(),
                    )
                    .unwrap();
                let mut receipt = [0; 72];
                receipt[..32].copy_from_slice(&id(0x80, namespace, index));
                receipt[32..].copy_from_slice(&encode_head(selected));
                requests
                    .insert(request_key(namespace, index).as_slice(), receipt.as_slice())
                    .unwrap();
            }
            heads
                .insert(
                    [namespace; 32].as_slice(),
                    encode_head(head(namespace, count - 1)).as_slice(),
                )
                .unwrap();
        }
        transaction
            .open_table(CLAIMS)
            .unwrap()
            .insert([0x51; 32].as_slice(), [0x61; 32].as_slice())
            .unwrap();
    }
    transaction.commit().unwrap();
}

fn set_record(path: &Path, table: BytesTable, key: &[u8], value: &[u8]) {
    let db = LegacyDatabase::open(path).unwrap();
    let transaction = db.begin_write().unwrap();
    transaction
        .open_table(table)
        .unwrap()
        .insert(key, value)
        .unwrap();
    transaction.commit().unwrap();
}

fn assert_status(path: &Path, expected: &[u8]) {
    let keyspace = fjall::Config::new(path).open().unwrap();
    let metadata = keyspace
        .open_partition("bbg_import_v1", Default::default())
        .unwrap();
    assert_eq!(metadata.get(b"status").unwrap().unwrap().as_ref(), expected);
}

#[test]
fn migration_preserves_paged_history_retries_claims_and_the_usable_source() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let destination = fixture.destination();
    create_legacy(&source, &[(1, 4101), (2, 2)]);
    ApplicationStore::migrate_redb(&source, &destination).unwrap();
    assert!(source.is_file());
    assert!(!guard_path(&destination).exists());
    assert_status(&destination, b"complete");
    let migrated = ApplicationStore::open(&destination).unwrap();
    assert_eq!(migrated.head(&[1; 32]).unwrap(), Some(head(1, 4100)));
    assert_eq!(migrated.head(&[2; 32]).unwrap(), Some(head(2, 1)));
    let first = migrated.history(&[1; 32], None, 4096).unwrap();
    assert_eq!(first, (0..4096).map(|i| head(1, i)).collect::<Vec<_>>());
    assert_eq!(
        migrated.history(&[1; 32], Some(4095), 4096).unwrap(),
        (4096..4101).map(|i| head(1, i)).collect::<Vec<_>>()
    );
    assert_eq!(
        migrated.history(&[2; 32], None, 4096).unwrap(),
        vec![head(2, 0), head(2, 1)]
    );
    for (namespace, index) in [(1, 0), (1, 4095), (1, 4096), (1, 4100), (2, 0), (2, 1)] {
        assert_eq!(
            migrated
                .content(&head(namespace, index).commit, 11)
                .unwrap(),
            Some(content(namespace, index))
        );
        assert_eq!(
            migrated
                .resolve(&[namespace; 32], &id(0x70, namespace, index))
                .unwrap(),
            Some((id(0x80, namespace, index), head(namespace, index)))
        );
    }

    let original_content = [(head(1, 0).commit, content(1, 0))];
    let retry = Write {
        namespace: [1; 32],
        request: id(0x70, 1, 0),
        fingerprint: id(0x80, 1, 0),
        expected: None,
        head: head(1, 0),
        content: &original_content,
        claims: &[],
    };
    assert_eq!(
        migrated
            .apply_with(&retry, |_| panic!("a migrated retry must not run twice"))
            .unwrap(),
        head(1, 0)
    );
    assert_eq!(migrated.head(&[1; 32]).unwrap(), Some(head(1, 4100)));
    let changed_retry = Write {
        fingerprint: [0x99; 32],
        ..retry
    };
    assert!(matches!(
        migrated.apply(&changed_retry),
        Err(Error::Conflict)
    ));

    let new_content = [(head(3, 0).commit, content(3, 0))];
    let mut birth = Write {
        namespace: [3; 32],
        request: id(0x70, 3, 0),
        fingerprint: id(0x80, 3, 0),
        expected: None,
        head: head(3, 0),
        content: &new_content,
        claims: &[([0x51; 32], [0x62; 32])],
    };
    assert!(matches!(migrated.apply(&birth), Err(Error::Conflict)));
    assert_eq!(migrated.head(&[3; 32]).unwrap(), None);
    assert_eq!(migrated.content(&head(3, 0).commit, 11).unwrap(), None);
    birth.claims = &[([0x51; 32], [0x61; 32])];
    let forged_content = [(head(1, 0).commit, vec![0x99]), new_content[0].clone()];
    birth.content = &forged_content;
    assert!(matches!(migrated.apply(&birth), Err(Error::Conflict)));
    assert_eq!(
        migrated.content(&head(1, 0).commit, 11).unwrap(),
        Some(content(1, 0))
    );
    birth.content = &new_content;
    migrated.apply(&birth).unwrap();

    let legacy = ApplicationStore::from_database(Database::open(&source, Backend::Hdd).unwrap());
    assert_eq!(legacy.head(&[1; 32]).unwrap(), Some(head(1, 4100)));
    assert_eq!(legacy.head(&[3; 32]).unwrap(), None);
    legacy.apply(&birth).unwrap();
    drop(legacy);
    let reopened = ApplicationStore::from_database(Database::open(&source, Backend::Hdd).unwrap());
    assert_eq!(reopened.head(&[3; 32]).unwrap(), Some(head(3, 0)));
}

#[test]
fn existing_destinations_are_refused_and_legacy_open_preserves_file_bytes() {
    let fixture = Fixture::new();
    let source = fixture.source();
    create_legacy(&source, &[(1, 2)]);
    let before = std::fs::read(&source).unwrap();
    assert!(matches!(
        ApplicationStore::open(&source),
        Err(Error::Storage(_))
    ));
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert!(source.is_file());
    let destination = fixture.destination();
    let store = ApplicationStore::open(&destination).unwrap();
    store
        .apply(&Write {
            namespace: [9; 32],
            request: [8; 32],
            fingerprint: [7; 32],
            expected: None,
            head: head(9, 0),
            content: &[(head(9, 0).commit, b"keep me".to_vec())],
            claims: &[],
        })
        .unwrap();
    drop(store);
    assert!(ApplicationStore::migrate_redb(&source, &destination).is_err());
    assert!(!guard_path(&destination).exists());
    let reopened = ApplicationStore::open(&destination).unwrap();
    assert_eq!(reopened.head(&[9; 32]).unwrap(), Some(head(9, 0)));
    assert_eq!(
        reopened.content(&head(9, 0).commit, 7).unwrap(),
        Some(b"keep me".to_vec())
    );
    assert_eq!(reopened.head(&[1; 32]).unwrap(), None);
    let existing_file = fixture.0.join("occupied-file");
    std::fs::write(&existing_file, b"preserve this file").unwrap();
    assert!(ApplicationStore::migrate_redb(&source, &existing_file).is_err());
    assert_eq!(std::fs::read(existing_file).unwrap(), b"preserve this file");
}

#[test]
fn malformed_records_leave_a_sealed_destination() {
    let cases = [
        (CONTENT, vec![0; 31], vec![0]),
        (HEADS, vec![1; 32], vec![0; 39]),
        (HISTORY, vec![0; 39], vec![0; 32]),
        (HISTORY, history_key(1, 0).to_vec(), vec![0; 31]),
        (REQUESTS, vec![0; 63], vec![0; 72]),
        (REQUESTS, request_key(1, 0).to_vec(), vec![0; 71]),
        (CLAIMS, vec![0; 31], vec![0; 32]),
        (CLAIMS, vec![0; 32], vec![0; 31]),
    ];
    for (table, key, value) in cases {
        let fixture = Fixture::new();
        create_legacy(&fixture.source(), &[(1, 2)]);
        set_record(&fixture.source(), table, &key, &value);
        assert!(matches!(
            ApplicationStore::migrate_redb(fixture.source(), fixture.destination()),
            Err(Error::Corrupt)
        ));
        assert!(fixture.destination().is_dir());
        assert!(guard_path(&fixture.destination()).is_file());
        assert!(matches!(
            ApplicationStore::open(fixture.destination()),
            Err(Error::Corrupt)
        ));
        assert_status(&fixture.destination(), b"copying");
        // The persistent marker also rejects an import whose sibling guard is lost.
        std::fs::remove_file(guard_path(&fixture.destination())).unwrap();
        assert!(matches!(
            ApplicationStore::open(fixture.destination()),
            Err(Error::Corrupt)
        ));
        assert!(LegacyDatabase::open(fixture.source()).is_ok());
    }
}

#[test]
fn dangling_history_and_receipts_cannot_complete_migration() {
    for (table, key, value) in [
        (HISTORY, history_key(1, 1).to_vec(), vec![0x99; 32]),
        (
            REQUESTS,
            request_key(1, 0).to_vec(),
            [vec![0; 32], encode_head(head(1, 7)).to_vec()].concat(),
        ),
    ] {
        let fixture = Fixture::new();
        create_legacy(&fixture.source(), &[(1, 2)]);
        set_record(&fixture.source(), table, &key, &value);
        assert!(matches!(
            ApplicationStore::migrate_redb(fixture.source(), fixture.destination()),
            Err(Error::Corrupt)
        ));
        assert!(matches!(
            ApplicationStore::open(fixture.destination()),
            Err(Error::Corrupt)
        ));
        assert_status(&fixture.destination(), b"copying");
    }
}

#[test]
fn missing_or_unknown_tables_are_rejected_before_destination_creation() {
    for kind in ["missing", "unknown", "multimap"] {
        let fixture = Fixture::new();
        let database = LegacyDatabase::create(fixture.source()).unwrap();
        let transaction = database.begin_write().unwrap();
        for table in TABLES.iter().take(if kind == "missing" { 4 } else { 5 }) {
            transaction.open_table(*table).unwrap();
        }
        if kind == "unknown" {
            transaction
                .open_table(TableDefinition::<&[u8], &[u8]>::new("unknown_table"))
                .unwrap()
                .insert(b"legacy".as_slice(), b"unhandled".as_slice())
                .unwrap();
        }
        if kind == "multimap" {
            transaction
                .open_multimap_table(MultimapTableDefinition::<&[u8], &[u8]>::new(
                    "unknown_multimap",
                ))
                .unwrap()
                .insert(b"legacy".as_slice(), b"unhandled".as_slice())
                .unwrap();
        }
        transaction.commit().unwrap();
        drop(database);
        assert!(
            ApplicationStore::migrate_redb(fixture.source(), fixture.destination()).is_err(),
            "unsupported source: {kind}"
        );
        assert!(
            !fixture.destination().exists(),
            "destination created for {kind}"
        );
        assert!(
            !guard_path(&fixture.destination()).exists(),
            "guard created for {kind}"
        );
    }
    let fixture = Fixture::new();
    assert!(ApplicationStore::migrate_redb(fixture.source(), fixture.destination()).is_err());
    assert!(!fixture.source().exists());
    assert!(!fixture.destination().exists());
}

#[test]
fn an_import_guard_blocks_open_even_before_the_destination_exists() {
    let fixture = Fixture::new();
    let guard = guard_path(&fixture.destination());
    std::fs::File::create(&guard).unwrap().sync_all().unwrap();
    assert!(matches!(
        ApplicationStore::open(fixture.destination()),
        Err(Error::Corrupt)
    ));
    assert!(!fixture.destination().exists());
    assert!(guard.is_file());
}

#[test]
fn migration_requires_exclusive_access_to_the_source() {
    let fixture = Fixture::new();
    create_legacy(&fixture.source(), &[(1, 2)]);
    let owner = LegacyDatabase::open(fixture.source()).unwrap();
    assert!(ApplicationStore::migrate_redb(fixture.source(), fixture.destination()).is_err());
    assert!(!fixture.destination().exists());
    assert!(!guard_path(&fixture.destination()).exists());
    drop(owner);
    ApplicationStore::migrate_redb(fixture.source(), fixture.destination()).unwrap();
    let migrated = ApplicationStore::open(fixture.destination()).unwrap();
    assert_eq!(migrated.head(&[1; 32]).unwrap(), Some(head(1, 1)));
}

#[test]
fn path_aliases_cannot_bypass_a_guard_before_the_first_database_marker() {
    let fixture = Fixture::new();
    let destination = fixture.destination();
    std::fs::create_dir(&destination).unwrap();
    std::fs::create_dir(fixture.0.join("bridge")).unwrap();
    std::fs::File::create(guard_path(&destination))
        .unwrap()
        .sync_all()
        .unwrap();
    let mut aliases = vec![
        PathBuf::from(format!("{}/", destination.display())),
        fixture.0.join("bridge/../working"),
    ];
    aliases.push(destination.join("."));
    #[cfg(unix)]
    {
        let alias = fixture.0.join("working-link");
        std::os::unix::fs::symlink(&destination, &alias).unwrap();
        aliases.push(alias);
    }
    for alias in aliases {
        assert!(
            matches!(ApplicationStore::open(&alias), Err(Error::Corrupt)),
            "guard bypass: {}",
            alias.display()
        );
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 0);
    }
}

#[test]
fn parent_aliases_check_the_same_guard_for_an_absent_destination() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.0.join("bridge")).unwrap();
    let guard = guard_path(&fixture.destination());
    std::fs::File::create(&guard).unwrap().sync_all().unwrap();
    let mut aliases = vec![fixture.0.join("bridge/../working")];
    aliases.push(fixture.0.join("./working"));
    #[cfg(unix)]
    {
        let alias = fixture.0.join("parent-link");
        std::os::unix::fs::symlink(&fixture.0, &alias).unwrap();
        aliases.push(alias.join("working"));
    }
    for alias in aliases {
        assert!(
            matches!(ApplicationStore::open(&alias), Err(Error::Corrupt)),
            "guard bypass: {}",
            alias.display()
        );
        assert!(!fixture.destination().exists());
        assert!(guard.is_file());
    }
}

#[path = "application_migration/integrity.rs"]
mod integrity;
