---
title: Fjall batch journal-write failure repair
tags: bbg, storage, fjall, audit
date: 2026-09-11
status: verified-scoped-repair
---

# Fjall batch journal-write failure repair

BBG's SSD backend resolves Fjall 2.11.2 from
[`rs/vendor/fjall`](../rs/vendor/fjall/PATCHES.md). This is the published 2.11.2
source with one production error-path repair and a test-only fault injector.
The major version, on-disk format and dependency requirements are unchanged.
Both upstream licenses and original source hashes are retained in the vendor
directory. The crates.io archive SHA-256 is
`0b25ad44cd4360a0448a9b5a0a6f1c7a621101cca4578706d43c9a821418aebc`;
the archive identifies upstream revision
`39113bf79dcc36890693d99c0006d8f8d484d51c`.

## Observed failure

The original `Batch::commit` discarded the result of
`journal_writer.write_batch(...)`, then optionally persisted and published the
batch in its memtables. A mid-batch write error could therefore return success
even with `SyncAll`. A successful flush cannot complete a record whose remaining
items and end marker were never written.

Returning the write error alone is insufficient. The partial journal tail must
also stop later writes: recovery can discard a valid later batch along with the
earlier unterminated record. Upstream reports
[#304](https://github.com/fjall-rs/fjall/issues/304) and
[#308](https://github.com/fjall-rs/fjall/issues/308) describe these failure modes
in 3.x; the relevant 2.11.2 implementation was inspected and reproduced locally.

The repair sets the shared poisoned flag under the journal writer lock and
returns the original error before applying any memtable mutation, advancing the
visible sequence number or updating write-buffer accounting. Other writers check
this same flag under the same lock. The existing persist-error path is retained.

## Regression evidence

The two new `batch::tests::partial_batch_write_*` tests execute the real keyspace,
partitions, journal encoding and recovery. A `cfg(test)` one-shot writer fault
flushes a Start marker and one Item to the actual journal file, then returns an
I/O error before writing the remaining item and End marker. The fault clears
itself so a subsequent write would succeed without the poisoned flag.

Both variants first failed against the unpatched production code: `commit`
returned `Ok(())`. Both pass with the repair, for durability `None` and `SyncAll`.
Each checks all of the following:

- A batch touching two partitions returns the original I/O error.
- Neither its insertion nor deletion becomes visible. The last accepted values,
  visible sequence number and write-buffer accounting remain unchanged.
- A later batch, direct insert, direct delete and persist all return `Poisoned`.
- The partial item is present in the actual journal bytes, proving the fault was
  after a write rather than before any I/O.
- Reopening discards the partial tail, preserves the earlier durable batch and
  reveals none of the rejected changes.
- A new insert/delete transaction succeeds after recovery and survives another
  reopen.

Commands from `bbg/rs/vendor/fjall`:

```sh
cargo test --locked --lib batch::tests -- --nocapture
cargo test --locked
cargo test --locked --no-default-features --lib batch::tests
```

The full vendored suite passed: 31 unit tests and 66 doctests on macOS arm64.
The unit total includes the two new regressions. Three pre-existing compiler
warnings remain: unused recovery-mode field, unused flush-statistics method and
an elided transaction lifetime. The tests used the vendored crate's original
lockfile. `cargo metadata --manifest-path rs/Cargo.toml --locked --no-deps`
also confirmed that BBG resolves the local Fjall path with requirement `^2`.
The two regressions also passed with default features disabled (four existing
dead-code warnings in that configuration). Rustfmt checks for the changed source
files and provenance/relative-link checks passed.

## Boundaries

This is deterministic internal write-failure injection with a real partial
journal and recovery. It is not an OS disk-full injection, power-loss test or
claim of full node durability. A commit failure still requires explicit outcome
resolution by the caller.

The production change covers `Batch::commit`. Upstream direct
`PartitionHandle::insert` and `remove` still propagate their own `write_raw`
failures without poisoning. They do refuse writes after a batch has poisoned the
keyspace, as tested here. BBG's durable multi-record path must use the repaired
batch interface; this report makes no broader repair claim for direct writes.

Normal shutdown is used in these regressions. Process-kill tests and node
integration remain separate gates in the
[P0 storage roadmap](../roadmap/storage-reliability.md).
