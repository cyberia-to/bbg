---
title: local streamed content validation
tags: bbg, audit, storage
status: partial
---
# local streamed content validation

BBG revision `36cc420b48c5af8be5afaa3679d271c7b90155e3` implements the local
portion of [[soft3/roadmap/storage/README|S2/S3/S4]] under the existing Database
owner. [Source closure and environment](sources.json), [commands and log
checksums](checks.json), and [dependency lock](dependencies.lock) bind this
receipt. Commands ran against the source committed at that revision.

## executed checks

Run from the BBG root with
`CARGO_TARGET_DIR=/tmp/cyber-content-storage-20260924/target`.
The complete executable commands are in [checks.json](checks.json).

| command after `cargo` | outcome | evidence |
|---|---|---|
| `test --release --manifest-path rs/Cargo.toml --features backend-ssd,backend-hdd --lib storage:: --offline` | 45 passed | [unit log](logs/bbg-storage-unit.log) |
| `test --release --manifest-path rs/Cargo.toml --features backend-ssd,backend-hdd --test content_storage --test application_storage --test shared_application --test application_migration --test storage_persistence --test storage_contract --test native_records --offline` | 55 passed | [integration log](logs/bbg-storage-integration.log) |
| `check --manifest-path rs/Cargo.toml --no-default-features --offline` | pass | [base log](logs/bbg-check-base.log) |
| `check --manifest-path rs/Cargo.toml --no-default-features --features backend-hdd --offline` | pass | [HDD-only log](logs/bbg-check-hdd.log) |

These commands emitted no compiler warnings. Test counts are harness entries;
subprocess helper entries are included, and parameter loops exercise additional
scenarios. This is the affected storage suite, not every BBG arithmetic/proof test.

## observed behavior

The content tests in the commands above exercise both Fjall/SSD and redb/HDD:

- Out-of-order arrivals, durable resume, identical and conflicting retries,
  rejected incomplete/wrong-identity files, empty content and duplicate uploads
  with different physical part sizes.
- Paged upload/coverage queries, cancellation tombstones and bounded reclaim of
  a sparse upload whose declared length is `u64::MAX`. This declares a range;
  it does not allocate or write that many bytes.
- Application head, retention and retry receipt publish together; rejected
  transitions leave all unchanged. Canonical sealed content cannot be cancelled.
- A child exits without destructors after a part, all parts with verification
  unfinished, seal, and application publication. Reopen checks acknowledged
  state and resumes unfinished work. These are durable-boundary process cuts.
- Deliberately altered stored bytes fail checksum verification and sealed reads.
- Export, transfer staging and namespace fences reject content mutations.
  Application-only migration refuses staging/sealed content; importing an old
  archive cannot fence uploads already present at its destination.

`storage::content::tests::faults::uncertain_part_seal_and_head_commits_freeze_every_view_and_recover_atomically`
wraps a real redb FileBackend. The unit command above runs nine combinations:
part/seal/head publication against physical-write rejection, failure before
sync, and failure after sync succeeds. Known precommit rejection restores the
prior transaction; CommitUnknown poisons the shared owner. Reopen checks that
parts/checksums/progress, descriptor/sealed state and head/retention each recover
together. A completed durability barrier survives the injected failed reply.

## review and limits

The host supplies namespace authorization and the verifier; BBG does not sandbox
arbitrary verifier implementations. Private records remain in namespace-scoped
content tables, outside public native graph projections. Internal part keys bind
namespace and request; descriptor identity remains the supplied file particle.
This code review establishes neither remote authorization nor a privacy audit.

Sealed content stays protected indefinitely. Release, leases, GC and content-aware
device migration remain open. Verification restarts from the beginning after
interruption. Local checksums do not provide authenticated network range proofs.

Both backend profiles ran on the local APFS solid-state volume recorded in
sources.json; the HDD label selects redb, not a physical rotating-disk test.
No physical power-loss, disk-full, performance, peak-memory or scaling qualification
was performed. Content-specific I/O fault injection covers redb; the process-cut
tests cover both profiles. Radio, FS names and product consumers remain pending.

To reproduce, arrange the repositories at sources.json revisions as siblings,
copy dependencies.lock to `rs/Cargo.lock` in an isolated checkout, prepare the
Cargo cache, and run checks.json commands. The contract is
[content storage](../../../specs/content-storage.md); the
[[soft3/roadmap/storage/acceptance|complete acceptance rows]] remain open.
