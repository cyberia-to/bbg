---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-18
---
# storage proofs

six proof types ensuring data retention across all storage tiers. these are bbg protocol requirements — every node must support generating and verifying these proofs.

## proof types

| proof | what it guarantees | mechanism | constraints |
|---|---|---|---|
| storage proof | content bytes exist on specific node | periodic challenge: random offset → return chunk + Lens opening | ~5,000 |
| size proof | claimed content size matches actual bytes | hemera tree structure + padding check | ~2,000 |
| replication proof | k independent copies exist | challenge k distinct nodes, verify uniqueness via nonce | ~5,000 × k |
| retrievability proof | content fetchable within bounded time | timed challenge-response with latency bound | ~5,000 |
| data availability proof (DAS) | block data published and accessible | algebraic DAS: erasure coding + Lens opening samples | ~3,000 (20 samples) |
| encoding fraud proof | erasure coding done correctly | decode k+1 cells, compare against polynomial commitment | O(k) field ops |

see [[cyber/proofs]] for the full proof taxonomy.

## resolution via signal-first + algebraic DAS

the signal-first architecture ([[sync]], [[storage]]) resolves the retention question for STATE data: prove signal availability → derive all state via deterministic replay.

the remaining open problem: CONTENT availability. signals carry cyberlink CIDs but not the content behind them. content availability requires:
- DAS over files dimension of BBG_poly
- π-weighted replication determines storage budget
- storage proofs verify individual nodes actually hold their claimed data

## in reference?

**partial** — DAS proofs are specified in reference/data-availability.md. π-weighted replication in reference/storage.md. the per-node storage/size/replication/retrievability proof mechanisms need specification in reference/storage.md.
