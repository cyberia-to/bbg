---
title: native transition validation
tags: bbg, audit, storage
---
# native transition validation

Validated on 2026-09-12 against the current workspace, including its existing
uncommitted authenticated-query/state-certificate changes.

The native preparation borrows the BBG exclusively and records the initial
value of each touched address. Candidate changes are coalesced after all
events. Failed preparation, a failed caller commit callback and dropping an
unpublished guard restore graph records, time, signal headers, balances,
nullifiers, commitments, intents, reverse edges, pruning timestamps, height
and checkpoint. The root cache is recomputed after restoration.

Validation:

- `cargo test --manifest-path rs/Cargo.toml --offline --test native_transition --lib`:
  54 library tests and 13 native transition integration tests passed.
- Integration coverage includes standard finalization root equivalence,
  overlapping batches, later-event rollback, failed callback and retry,
  duplicate nullifiers, height/balance overflow, epoch pruning rollback,
  intent persistence, exact u64 storage, ordered iteration and request bounds.
- Unit checks cover unique undo-key limits, rejection before encoding oversized
  adjacency snapshots and bounded pruning candidate collection.
- `cargo clippy --manifest-path rs/Cargo.toml --offline --lib --test native_transition`:
  completed with four pre-existing warnings in query_auth.rs, prune.rs and
  lib.rs. No warnings in the new transition implementation or tests.

The typed NativeState records preserve exact integers, signatures and policy
state beyond the existing authenticated root's coverage. Intent records remain
outside that root. The preparation does not verify authorization or external
proofs and rejects checkpoint accumulators because this native profile has no
durable accumulator encoding.

Undo and final changes have independent 8 MiB/65536-key limits; batches have
at most 64 events. Root construction and pruning rank calculation retain their
existing full-state scan costs. The storage coordinator's commit, restart and
binary crash tests belong to the downstream durable-acceptance validation.
