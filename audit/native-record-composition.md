---
title: native record transaction composition
tags: bbg, storage, audit
date: 2026-09-12
---
# native record transaction composition

The native state, history, request, metadata, balance, block and export record
domains share the existing Database transaction with typed shards and
ApplicationStore content, history, request receipts and claims. These tests
exercise the public RecordDomain/RecordLimits API added by the native acceptance
integration; the owning implementation and test-module wiring are maintained
in that integration change.

Validation on macOS arm64, Rust 1.95.0, using
`CARGO_TARGET_DIR=/tmp/bbg-migration-target` and `--features backend-ssd,backend-hdd`:

- `cargo test --manifest-path rs/Cargo.toml --locked --features
  backend-ssd,backend-hdd --test native_records`: 3 tests pass, each running
  against both Fjall and redb.
- `cargo test --manifest-path rs/Cargo.toml --locked --features
  backend-ssd,backend-hdd --lib storage::database::native_tests`: 4 tests pass.
- Scoped rustfmt/whitespace checks pass. Clippy for the library and
  native_records integration target succeeds with four existing BBG warnings
  and three existing vendor warnings; none originate in the added integration
  tests.

The integration cases verify all seven domains remain isolated despite sharing
keys; one successful application transaction updates/deletes native records
and shards alongside its receipt/content/claims; a rejected closure rolls every
domain back; committed changes and markers agree after reopen; identical retries
skip the transition; cloned application and shard views retain the exclusive
writer lock until the last owner drops. Native point reads, prefix scans,
exclusive cursors, byte budgets and staged-table scan rejection are exercised.

The fault tests wrap a real redb FileBackend with a one-shot failure and assert
the backend consumed it. They cover a physical read after closure staging but
before commit, a write failure, failure before synchronization, and failure
returning from a synchronization that already completed. Unknown commit outcomes
freeze native record reads/scans, transactions, application views and shard views.
Reopen checks all native records, native deletions, application history/content/
receipt, typed shards and commit markers as one old-or-new outcome. A completed
barrier with a failed acknowledgement recovers the new state, and retry does
not execute the accepted transition twice.

An observed redb behavior matters for recovery: a precommit read I/O failure
seals redb's handle and subsequent reads explicitly require reopen. That is a
known-aborted closure rather than CommitUnknown; the test reopens before checking
rollback. It does not assume that every known I/O failure leaves a usable handle.

These cases establish storage-component composition. They do not execute a
Cybergraph transition validator, simulate physical power loss, or qualify the
complete node acceptance path. Native coordination and node-process tests are
owned by the dependent repositories.
