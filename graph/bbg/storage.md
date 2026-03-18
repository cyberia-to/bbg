---
tags: cyber
alias: bbg storage, fjall storage, bbg disk layout
crystal-type: entity
crystal-domain: cyber
---
# storage

physical storage architecture for [[bbg]]. one storage engine ([[fjall]]) backs everything: particle data, directional indexes, cryptographic commitments, [[mutator set]] state, temporal indexes, and [[CozoDB]] query relations. validators and light clients use the same format — the difference is quantity of data, not how it is stored.

## why one engine

polynomial commitments are computed OVER data that lives on disk. the commitments are a cryptographic summary stored alongside the data they summarize. there is no separate "on-chain storage" vs "local storage" — a validator stores the full state, a light client stores synced namespaces, both in the same [[fjall]] keyspace.

storing data twice (once for proofs, once for queries) is wasteful and fragile. [[CozoDB]] uses [[fjall]] as its storage backend directly — it reads bbg partitions through fjall's sorted key-value interface. one write path, one source of truth, one thing to back up and replicate.

## why fjall

- pure [[Rust]], zero C dependencies (unlike RocksDB, LevelDB)
- LSM-tree architecture suits append-heavy workloads (content-addressed particles are write-once)
- partitions map to bbg's 13-root architecture
- range scans are the dominant access pattern for NMT recomputation and namespace sync — LSM-trees excel at sequential reads
- single-binary deployment, no external database process
- pluggable storage backend trait in [[CozoDB]] — fjall implements the same sorted key-value interface

## fjall keyspace layout

```
fjall keyspace: "bbg"

├── partition: "particles"
│   key:   CID                                 32 bytes
│   value: (energy, π*, axon fields)           particle/axon data
│   NMT leaf data: CID, energy, π*, axon fields
│   content-particles and axon-particles share namespace
│   axon-particles carry: weight A_{pq}, market state (s_YES, s_NO), meta-score
│
├── partition: "axons_out"
│   key:   (source_particle, axon_CID)         sorted by source
│   value: ()                                  presence (pointer to particles)
│   NMT by source — "all outgoing from p" is a single namespace proof
│
├── partition: "axons_in"
│   key:   (target_particle, axon_CID)         sorted by target
│   value: ()                                  presence (pointer to particles)
│   NMT by target — "all incoming to q" is a single namespace proof
│   LogUp proves consistency: every axon in axons_out/axons_in exists in particles
│
├── partition: "neurons"
│   key:   neuron_id                           hash of public key
│   value: (focus, karma, stake)               per-neuron aggregates
│   NMT over (neuron_id, focus, karma, stake)
│
├── partition: "locations"
│   key:   (neuron_id | validator_id)
│   value: (lat, lon, proof_data)              proof of location
│   NMT — enables spatial queries, geo-sharding, latency guarantees
│
├── partition: "coins"
│   key:   denomination_hash                   token type τ
│   value: (total_supply, params)              fungible token denominations
│   NMT over denominations
│
├── partition: "cards"
│   key:   card_CID                            content hash
│   value: (owner, metadata, name_binding)     non-fungible knowledge assets
│   NMT — names are cards bound to axon-particles (A6)
│   names resolve through card lookup, not a separate partition
│
├── partition: "files"
│   key:   CID                                 content hash
│   value: (availability_proof, DAS_metadata)  content availability records
│   NMT — proves content is retrievable, not just that CIDs exist
│
├── partition: "cyberlinks"
│   key:   leaf_index                          u64
│   value: commitment                          hemera-2 hash
│   AOCL (MMR) — private record commitments for all record types
│   append-only — never modified after write
│   MMR peaks stored as metadata entry
│
├── partition: "spent"
│   key:   chunk_index                         u64
│   value: MMR node hash                       archived consumption proofs
│   SWBF inactive archive MMR
│   old window chunks compacted here
│
├── partition: "balance"
│   key:   "active_window"                     single entry
│   value: bitmap                              2²⁰ bits (128 KB)
│   SWBF active window — directly accessible
│   committed as hemera-2(window_bits)
│   slides forward periodically: oldest chunk → spent partition
│
├── partition: "time"
│   key:   (namespace, boundary_index)         7 namespaces
│   value: BBG_root snapshot                   32 bytes
│   NMT with 7 namespaces: steps, seconds, hours, days, weeks, moons, years
│   one 32-byte hash per boundary — no full state duplication
│
├── partition: "signals"
│   key:   batch_index                         u64
│   value: (signal_batch, recursive_proof)     finalized signal batches
│   MMR — commits which batches were accepted and in what order
│
└── partition: "cozo_*"
    CozoDB internal relations via fjall backend trait
    ├── cozo relations (particles, axons, neurons, cards)
    ├── HNSW vector indices
    ├── PageRank cache
    └── derived aggregations
    NOT a copy of bbg data — different key ordering
    for different access patterns (Datalog joins vs range proofs)
```

## access patterns

| operation | partition | access type | who does it |
|-----------|-----------|-------------|-------------|
| store new particle | particles | point write | validator |
| update directional index | axons_out, axons_in | sorted insert | validator |
| recompute NMT root | particles, axons_*, neurons, etc. | full range scan | validator (per block) |
| generate NMT namespace proof | any NMT partition | range scan + path | validator (on demand) |
| verify NMT proof | — | proof verification | light client |
| namespace sync response | any NMT partition | range scan | validator |
| namespace sync receive | NMT partitions | batch write | light client |
| append private record | cyberlinks | append | validator |
| set removal bits | balance, spent | point write + append | validator |
| Datalog query | cozo_* | CozoDB query plan | both |
| name resolution | cards | point lookup by name binding | both |
| temporal query | time | range scan within namespace | both |
| signal finalization | signals | append | validator |

## state transitions

signals arrive in batches. each batch triggers:

1. verify recursive [[zheng]]-2 proof covering the signal batch
2. for each cyberlink in the batch: append private record commitment to "cyberlinks" (AOCL)
3. update public aggregates: "particles" (energy), "axons_out", "axons_in" (directional indexes)
4. update "neurons" (focus, karma, stake)
5. process spending: set removal bits in "balance" (SWBF active window), update "spent" (inactive archive)
6. update "coins", "cards", "files", "locations" as needed
7. recompute NMT roots for all affected partitions
8. compute new BBG_root = H(all 13 sub-roots)
9. fold into [[zheng]]-2 accumulator (constant-size checkpoint)
10. emit changeset — CozoDB applies incremental updates

step 7 is the expensive one. for incremental efficiency, NMT updates touch only the changed leaves and their paths to the root — O(log n) per affected leaf.

## validator vs light client

| | validator | light client |
|---|---|---|
| particles | all | synced namespaces only |
| axons_out, axons_in | full | synced ranges |
| neurons | full | full (public data) |
| locations | full | synced ranges |
| coins, cards, files | full | synced namespaces |
| cyberlinks (AOCL) | full | headers + own paths |
| spent, balance (SWBF) | full | headers + own paths |
| time | full | queried ranges |
| signals | full | headers + verified proofs |
| cozo_* | full materialized view | partial view over synced data |
| fjall keyspace | same layout | same layout, less data |

one codebase, one storage format, one API. `bbg::open(path)` returns the same interface regardless of role. the sync protocol fills in whatever is missing.

## CozoDB integration

[[CozoDB]] uses fjall as its storage backend via a pluggable backend trait. both CozoDB and bbg read/write the same fjall instance. CozoDB's relations are different sort orders over the same underlying data — not copies.

```
Ask query:  "all axons where source = X"
  → CozoDB translates to fjall range scan on axons_out
  → returns results immediately (local, trusted)

network query:  "prove all axons where source = X"
  → same fjall range scan on axons_out
  → NMT namespace proof generation from particles + axons_out
  → returns results + proof (trustless)
```

same data, same storage, two access modes. interactive queries go through CozoDB/[[Ask]]. provable queries go through [[zheng]]/NMT. both read from fjall.

## algebra-adaptive storage

the noun store holds trees with different-sized leaves depending on the algebra. the tree structure (cells as pairs) is universal — only leaf sizes differ.

| atom type | width | algebra | notes |
|-----------|-------|---------|-------|
| F₂ | 1 bit | Bt programs | compact, massive trees |
| F_p | 64 bits / 8 bytes | field programs | standard |
| word | 32 bits / 4 bytes | word-type | fits in F_p |
| hash | 256 bits / 32 bytes | 4 × F_p identity | CIDs, content addresses (hemera-2) |

the content-addressed store handles all leaf widths. `H(noun)` hashes the canonical serialization regardless of leaf size — a noun with bit-leaves and a noun with field-leaves both live in the same "particles" partition, keyed by their hash.

### algebra-dependent access patterns

different algebras produce different memory access patterns on the same noun store:

- **field programs** (Tri, Wav, Ten): dense trees with F_p leaves. sequential access — matrix ops produce predictable axis paths. cache-friendly. dominated by fma-pattern workloads.
- **binary programs** (Bt): ultra-compact trees with bit-sized leaves. very large trees (a SHA-256 circuit is millions of gates). bandwidth-bound.
- **graph programs** (Arc): sparse trees with hash-type leaves (CIDs pointing to other nouns). random access patterns. latency-bound.
- **mixed programs** (Rs): trees with both field and word leaves. irregular access.

### storage is wiring

in a content-addressed system, the noun tree topology IS the connectivity between operations and data. `axis(s, 2)` means "follow this wire to the left child." changing the algebra = changing the tree structure = changing the content addresses = changing the storage layout.

optimizing the memory system (content-addressed lookup, tree traversal, noun caching) accelerates every algebra simultaneously. a faster fjall read path speeds up field programs, binary programs, and graph programs equally — because all of them resolve to `H(noun) → noun` lookups and axis traversals over the same store.

### implications for fjall partitions

the existing keyspace layout handles this naturally — particles are content-addressed by hash regardless of leaf type. the "particles" partition does not care whether a noun contains bit-leaves or field-leaves; the key is the CID, the value is the serialized data.

hot-path optimization should consider which partitions are accessed by which algebra patterns. field programs hammer "particles" with sequential scans (dense noun trees, predictable traversal). graph programs hammer "particles" with random point reads (hash leaves → follow CID → another point read). binary programs produce long sequential reads over very large nouns. cache eviction policy, prefetch strategy, and fjall block size all benefit from knowing the dominant algebra in the current workload.

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

## names are cards

a [[name]] is a [[card]] bound to an axon-particle (A6: axons are particles, names are human-readable identifiers for those particles). names are stored in the "cards" partition as non-fungible knowledge assets. every name update creates a new private record in the AOCL and updates the card in the cards.root NMT.

```
~mastercyb/blog → QmXyz

stored as card in cards partition (NMT leaf):
  card_CID:     H("~mastercyb/blog")
  owner:        mastercyb
  name_binding: ("blog", QmXyz)
  metadata:     (time, version)

private record in cyberlinks (AOCL, append-only):
  ar = H_commit(cyberlink_record ‖ ρ)

resolution:
  cards partition point lookup → O(1)

history:
  AOCL contains all historical records
  provable via mutator set membership
```

names and the "cards" partition are not separate concepts — one is the protocol-level [[name]] (a [[card]] binding a human-readable identifier to an axon-particle), the other is the storage-level NMT that makes resolution fast and provable. the AOCL is the log. the cards partition is the current state.

## the bbg / ask boundary

bbg answers: is this data authentic? (proofs)
[[Ask]] answers: what does this data mean? (queries)

every query CAN become a proof — [[Ask]] formulates the Datalog query, bbg proves the result via [[zheng]]. the boundary is not about what is provable, but about responsibility:

- bbg stores particles, maintains NMT indexes (axons_out, axons_in, neurons, locations, coins, cards, files, time), runs the [[mutator set]] (cyberlinks, spent, balance), commits signal batches, computes the 13-root BBG_root, generates and verifies proofs, serves namespace sync. it is the authenticated storage engine.
- [[Ask]] compiles Datalog, optimizes query plans, runs graph algorithms (PageRank, Dijkstra, Louvain), manages HNSW vector indices, bridges interactive queries to provable queries. it is the reasoning engine.

bbg does not know what a query means. [[Ask]] does not know how a proof works. when a provable query is requested, [[Ask]] formulates it and hands the execution plan to bbg, which generates the proof via [[zheng]]. clean separation, composable.

```
neuron asks: "all axons where source = X, proved"
  → Ask: compile Datalog → query plan
  → bbg: execute plan against fjall → result set
  → bbg: generate NMT namespace proof over axons_out
  → return: result + proof
```

see [[bbg]] for the cryptographic structure, [[architecture]] for the layer model, [[privacy]] for mutator set, [[architecture-overview]] for overview
