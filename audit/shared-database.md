---
title: shared application and shard database
tags: bbg, storage, fjall, redb, audit
date: 2026-09-12
status: component-validation
---
# Shared application and shard database

ApplicationStore's direct redb ownership has been removed. It now implements
application semantics over the same Database transaction owner used by both
disk ShardStore adapters. Default application opens select Fjall. A caller can
open Database once, select its backend, and attach application and shard views
without opening a second database or filesystem lock.

The contracts are [Database](../specs/database.md),
[application storage](../specs/application-storage.md) and
[storage](../specs/storage.md). The [API guide](../docs/api/storage.md) records
construction, combined transactions and explicit legacy import.

## Implementation

- `storage/database` owns the physical engine, shared serialization lock,
  bounded raw-byte transaction overlay, canonical change identity and shared
  unknown-outcome latch. Condition checks and commit hold the same lock.
- `storage/disk.rs` is the shared typed shard adapter. FjallStore and RedbStore
  retain their existing constructors, while Database::shards attaches to an
  existing owner. Pending shard mutations remain private until their commit.
- ApplicationStore no longer imports redb in its operational implementation.
  Content, ordered history, conditional heads, immutable global claims and
  historical request receipts retain their semantics. `apply_with` adds typed
  shard mutations to that same transaction and skips execution on a recorded
  retry. Its fingerprint contract binds the transition inputs.
- Each disk batch uses the selected engine's durability barrier: repaired
  Fjall SyncAll or redb Immediate. Final commit errors freeze every shared view.
  The memory store remains independent and reports memory durability.
- Disk identities now use `bbg/database-batch/v1`, binding sorted table names
  and final operations. Existing shard tables/encodings/markers remain readable.
  New combined transactions update both last_transaction and the shard marker;
  application-only transactions preserve the last shard marker.

New Fjall directories are 0700 and new redb files are 0600 on Unix. The physical
writer lock outlives all cloned views. Default production Cybergraph/Cell
dependency trees include Fjall and exclude redb. Redb is explicitly selected
for its HDD profile or enabled for legacy import.

## Legacy import and review fixes

The optional migration reads all five legacy application tables from an
exclusively opened source. The source is retained. Bounded pages populate a
fresh Fjall destination, with record validation and source/destination content
hash comparison. Link validation checks contiguous history, content/head links
and exactly one receipt per accepted history entry. A temporary disk reverse
index bounds receipt-coverage memory; completion removes that index.

A durable sibling guard exists before destination creation, and a database
copying marker protects the directory during and after copying. Normal opens
reject incomplete imports. Review and regression work identified and fixed:

- unknown redb multimap tables were omitted by ordinary table enumeration;
  migration now rejects both unsupported table families;
- alternate destination path spellings could bypass the sibling guard before
  the first database marker; guard identity now normalizes the physical path;
- reopening an existing directory after an absence check permitted a competing
  destination to be overwritten; import now creates its directory exclusively,
  and ordinary open rechecks the guard after acquiring the database owner;
- automatic recovery markers exceeded the staging budget at its boundary;
  staging now reserves their two keys and 91 bytes within the total limits;
- checking only existing receipts missed absent or duplicate receipt coverage;
  the temporary reverse index enforces one receipt per history entry.

An incomplete destination is retained for diagnosis and requires a fresh target
on retry. Existing destinations are never overwritten. Cell's optional
`migrate-redb SOURCE DESTINATION` command forwards this operation through
Cybergraph before any normal graph open. Its default is the `bbg` directory;
an existing `cell.redb` requires explicit migration or store selection.

## Executed validation

Validation used the actual repository sources and local backend files on macOS
arm64. Both disk features were exercised together and the application/shared
transaction tests also ran with each backend feature alone. Test counts below
count Rust test functions; backend loops exercise both implementations.

| Target | Evidence |
|---|---|
| `application_storage` | Five existing application-contract tests, now run on both selected engines, including concurrent head races and rollback |
| `shared_application` | Seven checks plus one inert subprocess helper: common owner, mixed commit, rollback, historical retry, global claim race, conflicting duplicate content, opaque blobs and process termination |
| `storage_contract` | Twelve existing checks plus its inert subprocess helper, retaining bounded reads/scans, canonical encoding, deletion/update recovery and writer exclusion |
| `storage_persistence` | Five existing persistence/tier regressions |
| `storage_tier_failures` | Two actual WARM backend commits followed by injected HOT failure |
| library `storage::` | 23 tests: tier tests, physical-engine tests, seven redb I/O/failure tests and deterministic migration-open/budget regressions |
| `application_migration` | 13 tests: paged legacy import, malformed records, source/destination ownership, guard aliases and receipt coverage |

The shared subprocess test kills a child while its combined transaction is
staged and after apply_with returns. Reopening verifies shards, deletes,
application content, history, receipt, claims and markers together. This tests
four backend/phase cases, separately from the previous shard-only crash test.
Redb fault injection uses an actual file backend and covers failed read, write,
sync and loss of the sync return value. Combined-transaction failures freeze
application clones and shard views, and reopened receipts determine whether
the transition must run. Fjall uses the previously repaired and tested partial
journal-write path documented in [its audit](fjall-batch-write.md).

The principal migration fixture has 4103 history/content/request records across
two namespaces, crossing the page boundary. It preserves historical retry
results, immutable claims and source usability. Malformed records, dangling
links and unsupported tables are rejected, with incomplete destinations sealed.

Commands from this repository:

```sh
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --lib storage::
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test application_storage --test shared_application --test storage_contract --test storage_persistence --test storage_tier_failures
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --test application_migration
cargo test --manifest-path rs/Cargo.toml --locked --features backend-ssd --test application_storage --test shared_application
cargo test --manifest-path rs/Cargo.toml --locked --features backend-hdd --test application_storage --test shared_application
cargo clippy --manifest-path rs/Cargo.toml --locked --features backend-ssd,backend-hdd --lib --tests --no-deps
```

BBG Clippy retains four existing warnings in query_auth, prune (two) and lib,
plus three existing vendor warnings. No warning was emitted for this change.
Scoped formatting and Markdown link validation were performed. Pre-existing
BBG proof/state changes and package-version edits were preserved and excluded
from this implementation's commits.

[Cybergraph](../../cybergraph/audit/shared-bbg-database.md) passed five application
tests in both feature configurations. [Cell](../../cell/audit/shared-bbg-database.md)
passed 15 default workspace tests and 14 selected node/CLI tests with migration
enabled; strict Cell Clippy passed. Cybergraph retains two existing API warnings.
The Cyber consumer also compiled. Required downstream path version constraints
and locks were aligned with the already changed sibling package versions;
the Fjall/redb dependency versions were not changed.

## Boundary still open

This change removes the independent application engine path and supplies an
atomic application/shard composition API. Native Cyber/Soft3 signal acceptance,
chain/economic state publication and client acknowledgement are not wired by
this change. External HOT caches still require coordinated publication and
invalidation. COLD population and resumable archive transfer remain open.

Physical power loss, actual disk-full conditions and arbitrary mid-instruction
commit kills were not tested. Import interruption coverage simulates its
persistent guard/marker states and uses deterministic ownership regressions;
it is separate from the process-kill tests of committed application transactions.
The storage reliability P0 remains active for those native and archive gates.
