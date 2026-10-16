// Copyright (c) 2026, Cyber contributors
// This source code is licensed under both the Apache 2.0 and MIT License
// (found in the LICENSE-* files in the repository)

use crate::{Config, Error, PersistMode};
use std::sync::atomic::Ordering;

fn partial_journal_failure(mode: Option<PersistMode>) -> crate::Result<()> {
    let folder = tempfile::tempdir()?;
    {
        let keyspace = Config::new(&folder)
            .manual_journal_persist(true)
            .flush_workers(0)
            .compaction_workers(0)
            .open()?;
        let first = keyspace.open_partition("first", Default::default())?;
        let second = keyspace.open_partition("second", Default::default())?;

        let mut initial = keyspace.batch().durability(Some(PersistMode::SyncAll));
        initial.insert(&first, "stable", "old");
        initial.insert(&second, "retained", "old");
        initial.commit()?;
        let visible = keyspace.visible_seqno.load(Ordering::Acquire);
        let buffered = keyspace.write_buffer_size();

        keyspace.journal.get_writer().fail_batch_after_items = Some(1);
        let mut broken = keyspace.batch().durability(mode);
        broken.insert(&first, "partial-item", "never-published");
        broken.remove(&second, "retained");
        let result = broken.commit();
        assert!(matches!(result, Err(Error::Io(_))), "{result:?}");
        assert!(keyspace.is_poisoned.load(Ordering::Acquire));
        assert_eq!(keyspace.visible_seqno.load(Ordering::Acquire), visible);
        assert_eq!(keyspace.write_buffer_size(), buffered);
        assert_eq!(first.get("partial-item")?, None);
        assert_eq!(first.get("stable")?.as_deref(), Some(b"old".as_slice()));
        assert_eq!(second.get("retained")?.as_deref(), Some(b"old".as_slice()));

        // This fault is one-shot: without the poison guard, these would append
        // valid batches behind the damaged tail and be lost during recovery.
        let mut later = keyspace.batch().durability(Some(PersistMode::SyncAll));
        later.insert(&second, "later", "must-not-be-acknowledged");
        assert!(matches!(later.commit(), Err(Error::Poisoned)));
        assert!(matches!(
            first.insert("direct", "later"),
            Err(Error::Poisoned)
        ));
        assert!(matches!(second.remove("retained"), Err(Error::Poisoned)));
        assert!(matches!(
            keyspace.persist(PersistMode::SyncAll),
            Err(Error::Poisoned)
        ));
        assert_eq!(first.get("direct")?, None);
        assert_eq!(second.get("later")?, None);

        // The test writes a real partial journal, rather than failing before I/O.
        let bytes = std::fs::read(keyspace.journal.path())?;
        assert!(bytes
            .windows(b"partial-item".len())
            .any(|w| w == b"partial-item"));
    }

    {
        let keyspace = Config::new(&folder).open()?;
        let first = keyspace.open_partition("first", Default::default())?;
        let second = keyspace.open_partition("second", Default::default())?;
        assert_eq!(first.get("stable")?.as_deref(), Some(b"old".as_slice()));
        assert_eq!(second.get("retained")?.as_deref(), Some(b"old".as_slice()));
        assert_eq!(first.get("partial-item")?, None);
        assert_eq!(first.get("direct")?, None);
        assert_eq!(second.get("later")?, None);

        // Recovery discards the incomplete tail and permits a fresh transaction.
        let mut recovered = keyspace.batch().durability(Some(PersistMode::SyncAll));
        recovered.insert(&first, "recovered", "yes");
        recovered.remove(&second, "retained");
        recovered.commit()?;
    }
    let keyspace = Config::new(&folder).open()?;
    let first = keyspace.open_partition("first", Default::default())?;
    let second = keyspace.open_partition("second", Default::default())?;
    assert_eq!(first.get("recovered")?.as_deref(), Some(b"yes".as_slice()));
    assert_eq!(second.get("retained")?, None);
    Ok(())
}

#[test]
fn partial_batch_write_without_flush_returns_error_and_poisons() -> crate::Result<()> {
    partial_journal_failure(None)
}

#[test]
fn partial_batch_write_before_sync_all_returns_error_and_poisons() -> crate::Result<()> {
    partial_journal_failure(Some(PersistMode::SyncAll))
}
