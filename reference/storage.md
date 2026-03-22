---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.00013329872967701346
heat: 0.00013619145900959717
focus: 0.00012083973504801819
gravity: 0
density: 0.43
---
# storage

physical storage architecture for [[bbg]]. one storage engine ([[fjall]]) backs everything: particle data, directional indexes, cryptographic commitments, [[mutator set]] state, temporal indexes, and [[CozoDB]] query relations. validators and light clients use the same format — the difference is quantity of data, not how it is stored.

## why fjall

- pure [[Rust]], zero C dependencies (unlike RocksDB, LevelDB)
- LSM-tree architecture suits append-heavy workloads (content-addressed particles are write-once)
- partitions map to bbg's 13-root architecture
- range scans are the dominant access pattern for NMT recomputation and namespace sync — LSM-trees excel at sequential reads
- single-binary deployment, no external database process
- pluggable storage backend trait in [[CozoDB]] — fjall implements the same sorted key-value interface

## storage tiers

```
L1: Hot state
    NMT roots, aggregate data, mutator set state
    contents: 13 sub-roots (32 bytes each), active SWBF window (128 KB),
              MMR peaks for cyberlinks.root and spent.root
    size: O(roots + SWBF_window) — kilobytes to megabytes
    latency: sub-millisecond (in-memory)

L2: Particle data
    full particle and axon data, indexed by CID
    contents: particle energy, π* values, axon weights, market state,
              neuron focus/karma/stake, coin/card metadata
    size: O(particles + axons + neurons) — gigabytes to terabytes
    latency: milliseconds (SSD)
    content-addressed, append-mostly (axon weights update on new cyberlinks)

L3: Content store
    particle content (files), indexed by CID
    contents: raw content bytes for each particle
    size: unbounded — petabytes across network
    latency: seconds (network retrieval)
    DAS availability proofs via files.root
    self-authenticating: H(content) = CID

L4: Archival
    historical state via time.root snapshots
    contents: BBG_root snapshots at epoch boundaries
    size: unbounded
    latency: minutes to hours
    DAS ensures availability during active window
```

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

## private record lifecycle

individual cyberlinks exist only as private records in the mutator set. the public layer never sees the 7-tuple.

```
creation:
  1. neuron constructs cyberlink c = (ν, p, q, τ, a, v, t)
  2. addition record ar = H_commit(c ‖ ρ) appended to cyberlinks.root (AOCL)
  3. public aggregates updated:
     - axon H(p,q) weight incremented by a in particles.root
     - axon entry updated in axons_out.root and axons_in.root
     - neuron ν focus decremented in neurons.root
     - particle p and q energy updated in particles.root
  4. LogUp proof: aggregate deltas consistent across all three NMTs

active:
  private record exists in mutator set (provable via AOCL membership)
  public aggregates reflect the sum of all active private records

spending:
  1. neuron proves ownership of the private record (AOCL membership + secret)
  2. nullifier bits set in SWBF (balance.root)
  3. public aggregates decremented accordingly
  4. double-spend = all SWBF bits already set = structural rejection
```

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

## storage reclamation

when an axon's aggregate weight decays below threshold ε (see [[temporal]]):

```
1. axon-particle removed from particles.root
2. axon entry removed from axons_out.root and axons_in.root
3. LogUp proof of consistent removal across all three NMTs
4. if particle has zero remaining energy (no other axons reference it),
   particle eligible for L3 content reclamation
5. L4 archival snapshots remain valid at their height
```

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

different algebras produce different memory access patterns on the same noun store:

- **field programs** (Tri, Wav, Ten): dense trees with F_p leaves. sequential access — matrix ops produce predictable axis paths. cache-friendly. dominated by fma-pattern workloads.
- **binary programs** (Bt): ultra-compact trees with bit-sized leaves. very large trees (a SHA-256 circuit is millions of gates). bandwidth-bound.
- **graph programs** (Arc): sparse trees with hash-type leaves (CIDs pointing to other nouns). random access patterns. latency-bound.
- **mixed programs** (Rs): trees with both field and word leaves. irregular access.

the existing keyspace layout handles this naturally — particles are content-addressed by hash regardless of leaf type. hot-path optimization should consider which partitions are accessed by which algebra patterns. cache eviction policy, prefetch strategy, and fjall block size all benefit from knowing the dominant algebra in the current workload.

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

## the bbg / ask boundary

bbg answers: is this data authentic? (proofs)
[[Ask]] answers: what does this data mean? (queries)

every query CAN become a proof — [[Ask]] formulates the Datalog query, bbg proves the result via [[zheng]]. the boundary is not about what is provable, but about responsibility:

- bbg stores particles, maintains NMT indexes, runs the [[mutator set]], commits signal batches, computes the 13-root BBG_root, generates and verifies proofs, serves namespace sync. it is the authenticated storage engine.
- [[Ask]] compiles Datalog, optimizes query plans, runs graph algorithms (PageRank, Dijkstra, Louvain), manages HNSW vector indices, bridges interactive queries to provable queries. it is the reasoning engine.

bbg does not know what a query means. [[Ask]] does not know how a proof works. when a provable query is requested, [[Ask]] formulates it and hands the execution plan to bbg, which generates the proof via [[zheng]].

see [[architecture]] for the layer model, [[cross-index]] for LogUp consistency, [[temporal]] for axon decay, [[indexes]] for NMT leaf structures