---
title: BBG persistence audit
tags: bbg, cybergraph, cyber, storage, audit
crystal-type: report
crystal-domain: cyber
date: 2026-09-11
status: partial
---

# BBG persistence audit

BBG already owns memory and disk storage implementations. Fjall 2.11.2 is
present behind backend-ssd; redb is present behind backend-hdd. The Cyber node
still uses an independent Soft3 frame log to rebuild an in-memory graph.
Completing the existing Cybergraph/BBG integration is the next delivery step.

## current paths

| path | implementation and boundary |
|---|---|
| cyber node → soft3::node → Cybergraph::new → Bbg | BbgState uses BTreeMap collections; no ShardStore is attached to Bbg or called by finalize_block |
| soft3 host persistence | appends encoded frames to home/log and replays into memory; errors/truncation are covered by the existing node audit |
| TieredStore | separate ShardStore composition with HOT memory, optional WARM and COLD; callers instantiate and populate it explicitly |
| FjallStore | 14 partitions; put stages RAM/dirty entries, commit inserts and calls SyncAll, owned load reads disk |
| RedbStore | 14 dimension tables; put stages entries, commit groups them in a write transaction, owned load reads disk |
| Cybergraph ApplicationGraph → BBG ApplicationStore | separate local application content/history/head/receipt path with one Immediate redb transaction |
| UnimemStore | Apple Silicon source exists; backend-unimem and its dependency are not wired in the BBG manifest |

FjallStore and RedbStore are available backend types. Enabling their features
does not automatically move the live BbgState maps to disk. ApplicationStore
receipts concern local application history; they do not publish a neuron
SignalChain, verify remote authorship or establish network finality.

Sources: [storage](../rs/src/storage/mod.rs), [tier routing](../rs/src/storage/tiered.rs),
[Fjall](../rs/src/storage/fjall.rs), [Redb](../rs/src/storage/redb.rs),
[application transactions](../rs/src/storage/application.rs),
[Cybergraph applications](../../cybergraph/src/application.rs),
[node host](../../soft3/crate/src/node.rs).

## reproduced behavior and repair

The new [storage persistence tests](../rs/tests/storage_persistence.rs) create
disposable directories and exercise actual Fjall/Redb databases. They use
backend load after reopening, independently of the earlier HOT cache.

| check | initial observation | disposition |
|---|---|---|
| write, overwrite, commit, drop and reopen | all 13 persistent dimensions return the final value 99 through load; EPHEMERAL is absent | passed |
| get and iter after reopen | both backends have cache_get=None and cache_iter=0 while disk load returns 99 | shared read/recovery integration remains open |
| HOT get_mut + mark_dirty + commit | both disk backends reopen with old value 10 instead of new value 99 | fixed: mark_dirty stages the current HOT value in WARM |
| evict with no WARM | the only value disappears from HOT | fixed: retain the last copy when WARM is absent |
| evict EPHEMERAL | the old source routed it through WARM | fixed: retain local-only values in HOT; regression passes |

Before the repair, the first regression run had three failures: the two
disk mutation tests and eviction without WARM. After the repair, all five
final persistence tests pass. The EPHEMERAL test was added from source review;
its pre-fix behavior was not separately executed. The repair preserves the
existing public signatures and changes tier routing only.

## unresolved storage guarantees

1. ShardStore::commit returns a change-set hash without Result. Fjall and the
   legacy RedbStore ignore I/O errors, then clear pending entries. The hash
   covers dirty keys, not values or a durable transaction receipt.
2. The borrowed get/iter APIs expose cached entries only. Concrete load returns
   owned disk values but maps read/decode errors to absence. A shared fallible,
   bounded read/scan and recovery interface remains necessary.
3. Fjall commit writes keys individually before SyncAll. The adapter does not
   couple state, native signal history and a stable request receipt atomically.
4. TieredStore commits HOT before WARM, and commit has no failure signal.
   Archive only commits an already populated COLD tier; it copies no live data
   there. Eviction stages a WARM copy, while durability occurs at commit.
5. Removal writes disk separately from the pending insertion batch; read errors,
   deletion crash consistency and malformed field encodings need explicit tests.
6. Cybergraph's native signal acceptance and BbgState still lack this durable
   publication boundary. Reuse BBG transactions/backend ownership and keep
   one coordinated Cybergraph writer per relevant history domain.

These are source findings unless a runtime observation is listed above.
No disk-full, fsync-failure, process-kill or power-loss matrix was run here.
Reopening a dropped database verifies process-local reopen behavior; it does
not establish crash consistency of the whole node.

## existing application transactions

ApplicationStore has an error-returning transaction API, Immediate durability,
conditional heads, payload fingerprints, unique claims and retained receipts.
Five existing tests pass: reopen, rollback on rejected content, conflicting
heads/retries, global claims and concurrent writers. Four Cybergraph application
tests pass: content encoding, reference closure, request binding and corrupt
content rejection. This is the reusable local application foundation.

Its BBG transaction identity assumes an authenticated caller/adapter. Native
network publication must preserve the separate signal, signature, proof,
sequence and network-domain contracts.

## validation

Executed on Darwin arm64 with existing local path dependencies:

```text
# from bbg
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test storage_persistence -- --nocapture
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test application_storage
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --lib storage::

# from cybergraph
cargo test --locked --features local-storage --test applications
```

Results: 5 persistence tests, 5 application-storage tests, 9 existing tier
unit tests and 4 Cybergraph application tests passed. This is a targeted
storage check; broader proof/state suites are outside its scope.
The two memory-only persistence tests also pass with no backend features.
Clippy for the persistence test target completes with eight existing library
warnings in pruning and unchanged storage/facade code; the repair adds none.

The source baseline is BBG c731055, Cybergraph 89eb69c and Soft3 9d1b75b,
plus tier-routing repair 3c8eb3d. The workspace also
contained concurrent BBG proof/state changes, which were neither committed
nor reverted by this task. Pinned hashes of the inspected storage and host
files are in [the source manifest](persistence-sources.json).

The ordered integration work is [Cyber A0–A5](../../cyber/roadmap/a-local-node.md).
