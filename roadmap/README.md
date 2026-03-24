# bbg roadmap

design proposals for bbg authenticated state layer. proposals compose with [[zheng]] proof architecture and [[hemera]] hash. see [[full-pipeline]] for how all proposals integrate into one continuous fold from device to light client.

## status: in reference = proposal is now the canonical spec

## integrated architecture

| proposal | in reference? | target |
|----------|--------------|--------|
| [[full-pipeline]] | **yes** → reference/architecture.md (pipeline section) | end-to-end: signal creation → light client in one continuous fold |

## polynomial state

| proposal | in reference? | target |
|----------|--------------|--------|
| [[algebraic-nmt]] | **yes** → reference/indexes.md, reference/state.md | 9 NMTs → 1 polynomial, 33× fewer constraints per cyberlink |
| [[mutator-set-polynomial]] | **yes** → reference/privacy.md | SWBF + MMR → polynomial, O(1) non-membership proofs |
| [[temporal-polynomial]] | **yes** → reference/temporal.md | continuous time queries, time.root eliminated |
| [[unified-polynomial-state]] | **yes** → reference/state.md, reference/architecture.md | 13 sub-roots → 1 polynomial, LogUp eliminated |

## storage and replication

| proposal | in reference? | target |
|----------|--------------|--------|
| [[signal-first]] | **yes** → reference/sync.md, reference/storage.md | bbg state as materialized view over signal log |
| [[algebraic-das]] | **yes** → reference/data-availability.md | PCS openings replace NMT paths: 157× fewer constraints |
| [[pi-weighted-replication]] | **yes** → reference/storage.md | replication ∝ π, storage budget follows attention |
| [[storage-proofs]] | **resolved** by signal-first | proving data retention at all 4 tiers |

## query

| proposal | in reference? | target |
|----------|--------------|--------|
| [[verifiable-query]] | **yes** → reference/query.md | arbitrary CozoDB queries → polynomial opening proofs |

## structural sync mapping

| layer | property | proposals |
|-------|----------|-----------|
| 1. validity | state transition correct | (served by zheng) |
| 3. completeness | nothing omitted | [[algebraic-nmt]], [[unified-polynomial-state]], [[verifiable-query]] |
| 4. availability | data physically exists | [[signal-first]], [[algebraic-das]], [[pi-weighted-replication]] |
| temporal | state over time | [[temporal-polynomial]] |
| privacy | private state | [[mutator-set-polynomial]] |

## lifecycle

| status | meaning |
|--------|---------|
| **in reference** | merged into canonical spec — this is the architecture |
| draft | idea captured, open for discussion |
| resolved | addressed by another proposal |
