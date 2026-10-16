---
title: durable shard storage implementation
tags: bbg, storage, fjall, redb, audit
crystal-type: entity
crystal-domain: cyber
date: 2026-09-11
status: component-validation
---

# Durable shard storage implementation

The shared ShardStore API now reports storage failures and exposes persisted
reads through trait objects. Fjall and redb atomically commit updates, deletions
and a recovery marker before reporting disk commit success. This implements
D1 of the [P0 roadmap](../roadmap/storage-reliability.md) and the first D2 tier
consistency item. Native graph publication, node wiring and archival recovery
remain open.

The [storage specification](../specs/storage.md) defines the contract; the
[API reference](../docs/api/storage.md) documents its use. This report records
the implementation and executed evidence, superseding the shared-interface
failure findings in the earlier [persistence audit](persistence.md).
The storage implementation is recorded in BBG commit `011e5ab`, following the
vendored Fjall repair `3a08ea2`; Evy's consumer migration is `c29fed9`.
The [source hash manifest](durable-shard-storage-sources.json) identifies the
reviewed implementation and regression files.

## Implementation reviewed

- [Shared interface](../rs/src/storage/mod.rs) and
  [access types](../rs/src/storage/access.rs): fallible writes, dirty marking,
  deletion and commit; bounded owned read and cursor scan; distinct absence,
  malformed data, I/O, lock conflict and unknown commit outcomes.
- [Write buffer](../rs/src/storage/buffer.rs): bounded pending keys/elements,
  coalesced overwrites, staged deletions and a canonical identity binding final
  operations and values. Failed commit retains pending state. An unknown
  outcome retains its identity and freezes the handle until reopen.
- [Fjall adapter](../rs/src/storage/fjall.rs): one repaired Fjall 2.11.2 batch
  contains updates, deletions and `last_commit`, with `PersistMode::SyncAll`.
  An OS file lock excludes competing BBG writers. Store creation requires an
  existing parent directory.
- [Redb adapter](../rs/src/storage/redb.rs): one `Immediate` transaction contains
  all affected tables and the marker. Precommit errors propagate without
  clearing pending changes; transaction commit errors retain an unknown outcome.
- [Memory store](../rs/src/storage/mem.rs) and
  [tier routing](../rs/src/storage/tiered.rs): bounded/coalesced HOT pending
  writes, WARM-first staging and commit, fallible dirty propagation, last-copy
  eviction protection, and explicit failure after WARM succeeds but HOT fails.
  Disk WARM attachment requires memory HOT with no persistent contents or
  pending changes; WARM absence overrides stale COLD data. Invalid input
  rejected before staging leaves the store usable.

Disk borrowed caches are read-only. Owned reads distinguish a missing record
from a failed read and see staged mutations. Tiered reads use healthy HOT cache
hits without disk I/O, while a poisoned WARM still surfaces its unresolved
outcome. Disk scans require a clean pending batch and decode bounded pages in
key order. Decoding rejects malformed field bytes, noncanonical limbs and
malformed keys, including keys before the smallest valid 32-byte key.
EPHEMERAL remains local memory. Existing shard names and canonical field
encoding are retained; versioned marker metadata is added on the first
nonempty successful commit.

## Executed validation

Commands run from `bbg` on macOS arm64, with both disk features:

```sh
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test storage_contract
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test storage_persistence
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test storage_tier_failures
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --lib storage::
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test application_storage
```

| Target | Passing tests | Evidence |
|---|---:|---|
| `storage_contract` | 13 | Reopened trait reads/scans, all persistent dimensions, bounded access, deletion/update batches, identities, malformed values/keys, exclusive writers, HOT attachment, pending budgets, coalescing and rejected inputs, process termination |
| `storage_persistence` | 5 | Existing persistence regressions against real backend files |
| `storage_tier_failures` | 2 | Actual Fjall and redb WARM commit succeeds, injected HOT commit fails; returned unknown identity resolves to recovered values/deletion |
| library `storage::` | 14 | Nine tier regressions and five redb I/O fault tests |
| `application_storage` | 5 | Existing application transaction regressions |

The contract count contains 12 checks and `storage_child`, an inert helper in
normal execution which the parent crash test invokes in a subprocess.
Featureless `storage_persistence` also passed its two applicable tests.
Library/tests clippy exited successfully with four existing BBG warnings
(two in prune, one in query_auth and one in lib) and three vendor warnings.

Downstream Cybergraph's application target passed four tests, and
`cargo check --locked` succeeded in Cyber. These establish consumer compilation
and existing application behavior; they do not test native node crash recovery.

The [vendored Fjall repair](fjall-batch-write.md) passed 31 unit tests and 66
doctests with `cargo test --manifest-path rs/vendor/fjall/Cargo.toml --locked`.
Its two partial-write regressions also passed with default features disabled.
Original source provenance and both licenses are retained with the vendor.

The [Evy consumer migration](../../evy/audit/fallible-storage-consumer.md) passed
56 tests across storage, dispatch and core, and compiled the hello example
through an isolated workspace pointing at the exact repository source files.
The native Evy workspace remains blocked during resolution by its existing
forwarding of unavailable `bbg/backend-unimem`; the isolated harness omits that
edge in temporary manifests. This does not establish full workspace or unimem
validation. Existing compiler warnings remain documented in those audits.

## Failure boundaries exercised

[Redb fault tests](../rs/src/storage/redb_tests.rs) wrap an actual file backend
and deterministically fail a read, a write, a sync before its barrier, or the
return path after the underlying sync completed. A zero-sized database cache
forces the read regression through the injected backend. Tests verify the
fault was consumed, errors remain explicit, pending puts/deletes survive, and
reopening resolves to a complete old or complete new batch consistent with
the marker. The fifth test checks failed WARM commit retains HOT pending data
and blocks publication.

[Tier failure tests](../rs/tests/storage_tier_failures.rs) use a failing HOT
wrapper with each real disk WARM backend. They verify `CommitUnknown` contains
the committed WARM identity, subsequent access stays frozen, and reopen
recovers the full accepted update/delete batch.

[Process termination](../rs/tests/storage_contract.rs) starts a separate test
process against each backend and kills it at two explicit boundaries: after
staging an update/delete batch, and after `commit()` returns. A stdout marker
synchronizes the kill; the child parks without normal teardown. On the tested
Unix platform, `Child::kill` sends SIGKILL. Reopening checks the complete old or
new state, marker and released writer lock. This covers four backend/phase
cases. It does not interrupt arbitrary instructions inside a commit or exercise
a native node client's acknowledgement protocol.

The Fjall tests inject failure after writing part of an actual journal batch,
before the End marker, then use normal close/reopen. They establish the repaired
error path and recovery of an incomplete tail. Their scope is separate from
the subprocess kill tests.

## Remaining acceptance work

The tested profile is MemStore HOT with Fjall or redb WARM on a local macOS
filesystem. Memory-only commit supplies a change identity without disk
durability. The adapters use backend durability barriers and sync the store's
parent directory; callers own creation and durability of ancestor directories.
No physical power interruption, OS disk-full condition, filesystem fault
campaign or device write-cache validation was performed. Injected I/O failures
and SIGKILL preserve different parts of the operating system and cannot replace
power-loss evidence. Backend decoding bounds the owned output; the database
may allocate an encoded record before BBG can reject its length.

Recovery retains only the latest shard batch marker. Native request identities,
history positions, state roots, economic transitions and conflict-aware retries
need their own atomic publication boundary. The in-memory `Bbg` facade and
native Cyber/Soft3 acceptance path have not been wired to it by this change.
ApplicationStore remains the separate local application-history transaction
API. No pinned Cyber binary recovery scenario was run for this implementation.

COLD population and resumable archival progress remain open. `archive()` only
commits entries already staged in that tier. Unimem feature wiring and durable
profile/format qualification also remain open. The comprehensive failure matrix
and D3–D4 node work therefore keep the P0 roadmap active.
