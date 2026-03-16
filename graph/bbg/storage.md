---
tags: cyber
alias: bbg storage, fjall storage, bbg disk layout
crystal-type: entity
crystal-domain: cyber
---
# storage

physical storage architecture for [[bbg]]. one storage engine ([[fjall]]) backs everything: edge data, polynomial indexes, cryptographic commitments, [[mutator set]] state, and [[CozoDB]] query relations. validators and light clients use the same format — the difference is quantity of data, not how it is stored.

## why one engine

polynomial commitments are computed OVER data that lives on disk. the commitments are a cryptographic summary stored alongside the data they summarize. there is no separate "on-chain storage" vs "local storage" — a validator stores the full state, a light client stores synced namespaces, both in the same [[fjall]] keyspace.

storing data twice (once for proofs, once for queries) is wasteful and fragile. [[CozoDB]] uses [[fjall]] as its storage backend directly — it reads bbg partitions through fjall's sorted key-value interface. one write path, one source of truth, one thing to back up and replicate.

## why fjall

- pure [[Rust]], zero C dependencies (unlike RocksDB, LevelDB)
- LSM-tree architecture suits append-heavy workloads (content-addressed edges are write-once)
- partitions map to bbg's logical separation (edges, indexes, commitments)
- range scans are the dominant access pattern for polynomial commitment recomputation and namespace sync — LSM-trees excel at sequential reads
- single-binary deployment, no external database process
- pluggable storage backend trait in [[CozoDB]] — fjall implements the same sorted key-value interface

## fjall keyspace layout

```
fjall keyspace: "bbg"

├── partition: "edges"
│   key:   H(edge)                              32 bytes
│   value: (neuron, from, to, weight, time)     edge data
│   write-once, content-addressed, immutable
│
├── partition: "idx_neuron"
│   key:   (neuron_id, edge_hash)               sorted
│   value: ()                                   presence
│   polynomial commitment computed OVER this sorted data
│   range scan → all edges by creator
│
├── partition: "idx_particle"
│   key:   (particle_hash, edge_hash)           sorted
│   value: ()                                   presence
│   range scan → all edges touching a particle
│
├── partition: "focus"
│   key:   neuron_id
│   value: F_p                                  focus score
│   polynomial commitment over (neuron, score) pairs
│
├── partition: "balance"
│   key:   neuron_id
│   value: F_p                                  token balance
│   polynomial commitment over (neuron, balance) pairs
│
├── partition: "aocl"
│   key:   leaf_index                           u64
│   value: commitment                           Hemera hash
│   MMR peaks stored as metadata entry
│   append-only — never modified after write
│
├── partition: "swbf"
│   key:   chunk_index
│   value: bloom filter bits
│   active window: 128 KB directly accessible
│   inactive chunks: compacted into MMR
│
├── partition: "commitments"
│   key:   (index_kind, epoch)
│   value: WHIR commitment data
│   polynomial coefficients and evaluation points
│   recomputed on state transitions via range scan of source partition
│
├── partition: "refs"
│   key:   ref_name (UTF-8)
│   value: (Hash, sequence: u64, author)
│   mutable name resolution (file roots, PIDs, agent entry points)
│
├── partition: "refs_log"
│   key:   (ref_name, sequence)
│   value: (Hash, author)
│   append-only history — enables time travel and sync protocol
│
└── partition: "cozo_*"
    CozoDB internal relations via fjall backend trait
    ├── cozo relations (cyberlinks, particles, properties)
    ├── HNSW vector indices
    ├── PageRank cache
    └── derived aggregations
    NOT a copy of bbg data — different key ordering
    for different access patterns (Datalog joins vs range proofs)
```

## access patterns

| operation | partition | access type | who does it |
|-----------|-----------|-------------|-------------|
| store new edge | edges | point write | validator |
| update index | idx_neuron, idx_particle | sorted insert | validator |
| recompute commitment | idx_* → commitments | full range scan + NTT | validator (per block) |
| generate WHIR proof | idx_* + commitments | range scan + point reads | validator (on demand) |
| verify WHIR proof | commitments | point read | light client |
| namespace sync response | idx_* + edges | range scan | validator |
| namespace sync receive | edges, idx_* | batch write | light client |
| Datalog query | cozo_* + idx_* | CozoDB query plan | both |
| time travel | refs_log | range scan by sequence | both |

## state transitions

edges are appended in batches (per block). each batch triggers:

1. write edges to "edges" partition (content-addressed, write-once)
2. insert into idx_neuron and idx_particle (sorted, for each edge)
3. update focus and balance partitions
4. update aocl (append new commitment) and swbf (set removal bits)
5. range scan updated indexes → recompute polynomial commitments
6. compute new BBG_root = H(all commitment roots)
7. generate [[stark]] proof that state transition is valid
8. emit changeset → CozoDB applies incremental updates

step 5 is the expensive one. for incremental efficiency, commitments can be structured hierarchically: block-level sub-commitments merge into epoch-level commitments merge into the global root. each level is a polynomial over a manageable number of points. recomputation touches only the changed sub-tree.

## validator vs light client

| | validator | light client |
|---|---|---|
| edges | all | synced namespaces only |
| idx_neuron | full | synced ranges |
| idx_particle | full | synced ranges |
| focus, balance | full | full (public data) |
| aocl, swbf | full | headers + own paths |
| commitments | computes them | receives and verifies |
| CozoDB | full materialized view | partial view over synced data |
| fjall keyspace | same layout | same layout, less data |

one codebase, one storage format, one API. `bbg::open(path)` returns the same interface regardless of role. the sync protocol fills in whatever is missing.

## CozoDB integration

[[CozoDB]] uses fjall as its storage backend via a pluggable backend trait. both CozoDB and bbg read/write the same fjall instance. CozoDB's relations are different sort orders over the same underlying data — not copies.

```
Ask query:  "all edges where neuron = X"
  → CozoDB translates to fjall range scan on idx_neuron
  → returns results immediately (local, trusted)

network query:  "prove all edges where neuron = X"
  → same fjall range scan on idx_neuron
  → WHIR proof generation from commitment + sorted data
  → returns results + proof (trustless)
```

same data, same storage, two access modes. interactive queries go through CozoDB/[[Ask]]. provable queries go through [[zheng]]/[[WHIR]]. both read from fjall.

## dependency graph

```
fjall (disk storage)
  ↑
bbg (authenticated state logic)
  ↑         ↑
CozoDB    zheng
(queries) (proofs)
```

bbg owns the fjall keyspace. CozoDB and zheng are consumers — CozoDB for interactive Datalog queries, zheng for proof generation and verification. neither knows about the other. bbg mediates.

see [[bbg]] for the cryptographic structure, [[data structure for superintelligence]] for full specification, [[mutator set]] for UTXO privacy
