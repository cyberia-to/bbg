use super::*;

fn remove_receipts(path: &Path, keys: &[[u8; 64]]) {
    let db = LegacyDatabase::open(path).unwrap();
    let transaction = db.begin_write().unwrap();
    {
        let mut requests = transaction.open_table(REQUESTS).unwrap();
        for key in keys {
            assert!(requests.remove(key.as_slice()).unwrap().is_some());
        }
    }
    transaction.commit().unwrap();
}

fn assert_rejected_and_sealed(fixture: &Fixture) {
    assert!(matches!(
        ApplicationStore::migrate_redb(fixture.source(), fixture.destination()),
        Err(Error::Corrupt)
    ));
    assert!(guard_path(&fixture.destination()).is_file());
    assert!(matches!(
        ApplicationStore::open(fixture.destination()),
        Err(Error::Corrupt)
    ));
    assert_status(&fixture.destination(), b"copying");
    assert!(LegacyDatabase::open(fixture.source()).is_ok());
}

fn assert_no_temporary_receipt_index(path: &Path) {
    let keyspace = fjall::Config::new(path).open().unwrap();
    let metadata = keyspace
        .open_partition("bbg_import_v1", Default::default())
        .unwrap();
    assert_eq!(
        metadata.get(b"status").unwrap().unwrap().as_ref(),
        b"complete"
    );
    assert!(metadata.prefix(b"r").next().is_none());
}

#[test]
fn missing_all_or_one_receipt_prevents_migration_completion() {
    for all in [true, false] {
        let fixture = Fixture::new();
        create_legacy(&fixture.source(), &[(1, 2), (2, 2)]);
        let keys = if all {
            vec![
                request_key(1, 0),
                request_key(1, 1),
                request_key(2, 0),
                request_key(2, 1),
            ]
        } else {
            vec![request_key(1, 0)]
        };
        remove_receipts(&fixture.source(), &keys);
        assert_rejected_and_sealed(&fixture);
    }
}

#[test]
fn duplicate_receipts_cannot_cover_another_missing_history_entry() {
    for replace_existing in [true, false] {
        let fixture = Fixture::new();
        create_legacy(&fixture.source(), &[(1, 2), (2, 2)]);
        let mut duplicate = [0; 72];
        duplicate[..32].copy_from_slice(&id(0x80, 1, 1));
        duplicate[32..].copy_from_slice(&encode_head(head(1, 0)));
        // Replacement keeps the receipt count unchanged while leaving index1
        // uncovered. An extra row proves duplicate coverage itself is invalid.
        let key = request_key(1, if replace_existing { 1 } else { 99 });
        set_record(&fixture.source(), REQUESTS, &key, &duplicate);
        assert_rejected_and_sealed(&fixture);
    }
}

#[test]
fn an_empty_legacy_application_database_can_migrate_and_accept_its_first_write() {
    let fixture = Fixture::new();
    let source = LegacyDatabase::create(fixture.source()).unwrap();
    let transaction = source.begin_write().unwrap();
    for table in TABLES {
        transaction.open_table(table).unwrap();
    }
    transaction.commit().unwrap();
    drop(source);
    ApplicationStore::migrate_redb(fixture.source(), fixture.destination()).unwrap();
    assert!(!guard_path(&fixture.destination()).exists());
    assert_no_temporary_receipt_index(&fixture.destination());
    let migrated = ApplicationStore::open(fixture.destination()).unwrap();
    assert_eq!(migrated.head(&[1; 32]).unwrap(), None);
    assert!(migrated.history(&[1; 32], None, 4096).unwrap().is_empty());
    assert_eq!(migrated.resolve(&[1; 32], &id(0x70, 1, 0)).unwrap(), None);
    let write = Write {
        namespace: [1; 32],
        request: id(0x70, 1, 0),
        fingerprint: id(0x80, 1, 0),
        expected: None,
        head: head(1, 0),
        content: &[(head(1, 0).commit, content(1, 0))],
        claims: &[],
    };
    assert_eq!(migrated.apply(&write).unwrap(), head(1, 0));
}

#[test]
fn successful_receipt_validation_removes_its_temporary_reverse_index() {
    let fixture = Fixture::new();
    create_legacy(&fixture.source(), &[(1, 2), (2, 2)]);
    ApplicationStore::migrate_redb(fixture.source(), fixture.destination()).unwrap();
    assert_no_temporary_receipt_index(&fixture.destination());
    let migrated = ApplicationStore::open(fixture.destination()).unwrap();
    for namespace in [1, 2] {
        for index in [0, 1] {
            assert_eq!(
                migrated
                    .resolve(&[namespace; 32], &id(0x70, namespace, index))
                    .unwrap(),
                Some((id(0x80, namespace, index), head(namespace, index)))
            );
        }
    }
}
