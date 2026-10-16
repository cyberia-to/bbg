# Local patches to Fjall 2.11.2

This directory vendors the published Fjall 2.11.2 source under its original
MIT OR Apache-2.0 license. Both license files are included. `UPSTREAM.json`
records the crate archive checksum, upstream revision and original file hashes.
The crate version, dependencies and on-disk format are unchanged.

`src/batch/mod.rs` now propagates journal batch-write errors and poisons the
keyspace while holding the journal writer lock. It returns before changing
memtables, visible sequence number or write-buffer accounting. Subsequent writes
are refused until the keyspace is reopened and recovery has handled its tail.

`src/journal/writer.rs` adds a `cfg(test)` one-shot fault after writing a partial
batch prefix to the real journal. `src/batch/tests.rs` checks error propagation,
unchanged in-memory state, refusal of later writes and recovery across two
reopens. Test fault code is absent from normal dependency builds.

The upstream reports describe the corresponding failure modes in Fjall 3.x:
[ignored batch write errors](https://github.com/fjall-rs/fjall/issues/304) and
[writes after a damaged journal tail](https://github.com/fjall-rs/fjall/issues/308).
The same relevant code was independently inspected and tested in this 2.11.2
source. This patch does not import a 3.x format or API.

The scoped validation and remaining boundaries are recorded in
[BBG's audit](../../../audit/fjall-batch-write.md).
