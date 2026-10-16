---
title: BBG delivery roadmap
tags: bbg, cyber, roadmap, storage
crystal-type: plan
crystal-domain: cyber
status: active
---

# bbg roadmap

## P0: storage reliability

The owner assigned highest delivery priority to
[durable storage for the Cyber node](storage-reliability.md) on 2026-09-11.
BBG owns the storage contract and its Fjall/redb implementations. Cybergraph
integrates native graph acceptance; Cyber/Soft3 consumes that path.

Close this work before claiming a reliable node or advancing the dependent
[Cyber A1 acceptance gate](../../cyber/roadmap/a-local-node.md#a1-complete-the-existing-storage-contract).
It takes precedence over storage optimizations and new storage features.
Independent proof research can continue alongside it; delivery resources
follow P0 first. Backend availability alone does not satisfy this gate.

The P0 document defines ordered tasks, owners and executable exit criteria.
Keep contracts in `specs/`, rationale in `docs/explanation/`, and observed
results with source revisions in `audit/`.

## other design tracks

| track | contract | scope |
|---|---|---|
| [storage proofs](storage-proofs.md) | [storage](../specs/storage.md), [data availability](../specs/data-availability.md) | per-node retention, size, replication and retrievability proofs |
| [verifiable query](verifiable-query.md) | [query](../specs/query.md) | query compilation and proof construction |
| [evy ShardStore](evy-shardstore.md) | [storage](../specs/storage.md) | ECS storage API and Unimem compatibility to preserve during P0 changes |

## earlier proposals (contracts in specs, rationale in explanation)

| former proposal | specs | explanation |
|---|---|---|
| algebraic-nmt | indexes.md, state.md, architecture.md | why-polynomial-state.md |
| unified-polynomial-state | state.md, architecture.md | why-polynomial-state.md |
| mutator-set-polynomial | privacy.md | polynomial-privacy.md |
| signal-first | sync.md, storage.md | why-signal-first.md |
| algebraic-das | data-availability.md | data-availability.md |
| full-pipeline | architecture.md (pipeline section) | architecture-overview.md |
| temporal-polynomial | temporal.md | (absorbed into reference) |
| pi-weighted-replication | storage.md | (absorbed into reference) |
