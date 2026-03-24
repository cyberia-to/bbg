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

physical storage architecture for [[bbg]]. the signal log is the primary data — all state is derived from deterministic replay. one storage engine ([[fjall]]) backs everything: particle data, directional indexes, polynomial evaluation tables, [[mutator set]] polynomials, and [[CozoDB]] query relations. validators and light clients use the same format — the difference is quantity of data, not how it is stored.

## signal-first model

bbg state is a deterministic function of the signal log. signals are append-only and self-certifying. the entire L1-L3 state is a materialized view, not primary data.

```
BBG_state(h) = fold(genesis, signals[0..h])

for any height h:
  replay signals[0..h] → deterministic BBG_root
  compare with claimed BBG_root → fraud detection
```

the irreducible minimum per node:
- signal log: append-only, DAS-protected, completeness-proved
- latest checkpoint: ~232 bytes (BBG_root + universal accumulator + height)

everything else is derivable: polynomial evaluation tables, particle data, axon aggregates, focus/π values — all reconstructible from signal replay.

## storage interface

the polynomial commitment provides authentication. the local data structure provides access. they are INDEPENDENT — the store doesn't know about polynomials, the polynomial doesn't know about storage. see [[data structures for polynomial state]] for the full theory.

```rust
trait ShardStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[FieldElement]>;
    fn put(&mut self, dimension: u8, key: &[u8; 32], value: &[FieldElement]);
    fn dirty_entries(&self) -> impl Iterator<Item = (u8, [u8; 32], &[FieldElement])>;
    fn commit(&mut self) -> [u8; 32];  // returns shard sub-commitment
}
```

three implementations, selected by scale:

| backend | optimal for | local structure | latency | when to use |
|---|---|---|---|---|
| inmem | shard fits in RAM (≤ 64 GB) | flat array + HashMap + BitVec | 50 ns read | bostrom → city scale |
| ssd (B+ tree) | shard exceeds RAM | B+ tree with RAM-cached top levels | 20 μs read | nation → planet scale |
| archival | full history, cold | sorted log + NMT layout index | sequential 200 MB/s | deep replay, research |

the trend: as storage gets faster, data structures get simpler. trees compensate for slow storage. when access is O(1) (RAM), the tree adds cost without benefit. with GFP (field ops in silicon) + RAM: the data structure disappears. bytes and math.

NMT survives — not for authentication (polynomial handles that) but for cold storage disk layout. sorted namespace = sequential reads = optimal for HDD.

## storage tiers

```
HOT (current state, RAM):
    polynomial evaluation tables for all dimensions
    commitment polynomial A(x) state, nullifier polynomial N(x) state
    BBG_root (32 bytes), shard sub-commitments
    backend: inmem (flat array)
    latency: 50 ns

WARM (recent state, SSD):
    full particle/axon data indexed by CID
    particle energy, π*, axon weights, market state
    neuron focus/karma/stake, coin/card metadata
    backend: B+ tree with RAM cache (top 3-4 levels pinned)
    latency: 20 μs

CONTENT (files, network):
    particle content (raw bytes), indexed by CID
    DAS availability proofs via files dimension of BBG_poly
    self-authenticating: H(content) = CID
    backend: distributed (π-weighted replication)
    latency: seconds (network retrieval)

COLD (full history, HDD/network):
    historical state via BBG_poly time dimension
    queryable at any past t via polynomial evaluation
    backend: archival (sorted log + NMT layout index)
    latency: sequential 200 MB/s (HDD), minutes for network
```

## storage proofs

six proof types ensure data retention across tiers:

| proof | what it guarantees | mechanism | constraints |
|---|---|---|---|
| storage proof | content bytes exist on node | periodic challenge: offset → chunk + PCS opening | ~5,000 |
| size proof | claimed size matches actual | hemera tree structure + padding check | ~2,000 |
| replication proof | k independent copies exist | challenge k nodes, verify uniqueness | ~5,000 × k |
| retrievability proof | content fetchable in bounded time | timed challenge-response | ~5,000 |
| DAS proof | block data published and accessible | algebraic DAS: erasure + PCS samples | ~3,000 |
| encoding fraud proof | erasure coding correct | decode k+1 cells vs polynomial commitment | O(k) field ops |

signal-first resolves STATE retention: prove signal availability → derive everything via replay. CONTENT retention requires storage proofs + π-weighted replication. see [[cyber/proofs]] for the full taxonomy.

## π-weighted replication

storage replication factor is proportional to π (cyberank). the network spends storage budget where attention goes.

```
replication_factor(particle) = max(R_min, R_base × π(particle) / π_median)

  R_min  = minimum replication (e.g., 3 — survival guarantee)
  R_base = baseline replication at median π (e.g., 10)

particle class        π estimate    replication factor
top-100 particle      ~10⁻²        ~1000 (effectively everywhere)
top-10K particle      ~10⁻⁴        ~100
median particle       ~10⁻⁶        10 (baseline)
tail particle         ~10⁻¹²       3 (minimum)
```

no explicit storage market needed. focus IS the storage payment. π IS the replication signal. the economics emerge from the graph topology.

DAS parameters scale with replication:

```
high-π particle (1000 replicas):
  base availability very high → fewer DAS samples needed
  5 samples sufficient for 99.99% confidence
  bandwidth: ~2.3 KiB

low-π particle (3 replicas):
  base availability minimal → standard sampling
  20 samples for 99.9999% confidence
  bandwidth: ~9 KiB
```

## fjall keyspace layout

```
fjall keyspace: "bbg"

├── partition: "particles"
│   key:   CID                                 32 bytes
│   value: (energy, π*, axon fields)           particle/axon data
│   polynomial evaluation table for particles dimension
│   content-particles and axon-particles share namespace
│   axon-particles carry: weight A_{pq}, market state (s_YES, s_NO), meta-score
│
├── partition: "axons_out"
│   key:   (source_particle, axon_CID)         sorted by source
│   value: ()                                  presence (pointer to particles)
│   polynomial evaluation table for axons_out dimension
│   "all outgoing from p" = PCS batch opening over this dimension
│
├── partition: "axons_in"
│   key:   (target_particle, axon_CID)         sorted by target
│   value: ()                                  presence (pointer to particles)
│   polynomial evaluation table for axons_in dimension
│   cross-index consistency: structural (same polynomial, no LogUp)
│
├── partition: "neurons"
│   key:   neuron_id                           hash of public key
│   value: (focus, karma, stake)               per-neuron aggregates
│   polynomial evaluation table for neurons dimension
│
├── partition: "locations"
│   key:   (neuron_id | validator_id)
│   value: (lat, lon, proof_data)              proof of location
│   polynomial evaluation table for locations dimension
│
├── partition: "coins"
│   key:   denomination_hash                   token type τ
│   value: (total_supply, params)              fungible token denominations
│   polynomial evaluation table for coins dimension
│
├── partition: "cards"
│   key:   card_CID                            content hash
│   value: (owner, metadata, name_binding)     non-fungible knowledge assets
│   names resolve through card lookup, not a separate partition
│
├── partition: "files"
│   key:   CID                                 content hash
│   value: (availability_proof, DAS_metadata)  content availability records
│   polynomial evaluation table for files dimension
│
├── partition: "commitments"
│   key:   commitment_point                    F_p
│   value: commitment_value                    F_p
│   commitment polynomial A(x) evaluation table
│   append-only — new records extend the polynomial
│   PCS commitment stored as metadata entry
│
├── partition: "nullifiers"
│   key:   nullifier_point                     F_p
│   value: zero_marker                         indicates spent
│   nullifier polynomial N(x) evaluation table
│   N(x) = ∏(x - n_i), committed via PCS
│   PCS commitment stored as metadata entry
│
├── partition: "time"
│   key:   (boundary_index)
│   value: BBG_poly evaluation snapshot        queryable at any past t
│   time is a native dimension of BBG_poly
│   any historical query = one PCS opening at (index, key, t_past)
│
├── partition: "signals"
│   key:   batch_index                         u64
│   value: (signal_batch, recursive_proof)     finalized signal batches
│   the primary data — all other state is derived from signals
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
| update polynomial evaluation | affected partitions | point write | validator (per block) |
| recommit BBG_poly | all polynomial partitions | batch read + PCS commit | validator (per block) |
| generate PCS opening | polynomial state | proof generation | validator (on demand) |
| verify PCS opening | — | proof verification | light client |
| namespace sync response | any partition | range scan | validator |
| namespace sync receive | partitions | batch write | light client |
| extend commitment polynomial | commitments | append | validator |
| extend nullifier polynomial | nullifiers | append | validator |
| Datalog query | cozo_* | CozoDB query plan | both |
| name resolution | cards | point lookup by name binding | both |
| temporal query | any dimension | PCS opening at (index, key, t_past) | both |
| signal finalization | signals | append | validator |

## private record lifecycle

individual cyberlinks exist only as private records in the polynomial mutator set. the public layer never sees the 7-tuple.

```
creation:
  1. neuron constructs cyberlink c = (ν, p, q, τ, a, v, t)
  2. commitment added to A(x): A'(c) = v (O(1) polynomial extension)
  3. public aggregates updated in BBG_poly:
     - BBG_poly(particles, H(p,q), t): axon weight incremented by a
     - BBG_poly(axons_out, p, t): outgoing index updated
     - BBG_poly(axons_in, q, t): incoming index updated
     - BBG_poly(neurons, ν, t): focus decremented
     - BBG_poly(particles, q, t): energy updated
  4. cross-index consistency: structural (same polynomial, no separate proof)

active:
  private record exists in commitment polynomial (provable via PCS opening of A(c))
  public aggregates reflect the sum of all active private records

spending:
  1. neuron proves ownership of the private record (PCS opening of A(c) + secret)
  2. nullifier added to N(x): N'(x) = N(x) × (x - n) (O(1) polynomial extension)
  3. public aggregates decremented accordingly
  4. double-spend = N(n) = 0 = structural rejection
```

## state transitions

signals arrive in batches. each batch triggers:

1. verify recursive [[zheng]]-2 proof covering the signal batch
2. for each cyberlink in the batch: extend commitment polynomial A(x) at new point
3. update public aggregates in BBG_poly: particles (energy), axons_out, axons_in (directional indexes)
4. update BBG_poly(neurons): focus, karma, stake
5. process spending: extend nullifier polynomial N(x) for spent records
6. update BBG_poly for coins, cards, files, locations as needed
7. recommit BBG_poly via PCS (batch all evaluation changes, one recommitment)
8. fold into [[zheng]]-2 accumulator (constant-size checkpoint)
9. emit changeset — CozoDB applies incremental updates

step 7 is the polynomial recommitment. batch all changed evaluations and recommit once per block. cost: O(|changes|) field operations — no tree path rehashing.

## storage reclamation

when an axon's aggregate weight decays below threshold ε (see [[temporal]]):

```
1. prove w_eff < ε (~20 constraints for decay calculation)
2. update BBG_poly(particles, axon, t): remove axon-particle
3. update BBG_poly(axons_out, source, t): remove entry
4. update BBG_poly(axons_in, target, t): remove entry
5. return decayed weight to decay pool
6. consistency: structural (same polynomial, no separate LogUp proof)
7. if particle has zero remaining energy (no other axons reference it),
   particle eligible for L3 content reclamation
8. historical state preserved in time dimension — past queries still work
```

## validator vs light client

| | validator | light client |
|---|---|---|
| particles | all | synced namespaces only |
| axons_out, axons_in | full | synced ranges |
| neurons | full | full (public data) |
| locations | full | synced ranges |
| coins, cards, files | full | synced namespaces |
| commitments (A(x)) | full | own PCS proofs only |
| nullifiers (N(x)) | full | own PCS proofs only |
| time | full (all evaluations) | queried via PCS openings |
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
  → PCS batch opening proof generation from BBG_poly
  → returns results + proof (trustless)
```

same data, same storage, two access modes. interactive queries go through CozoDB/[[Ask]]. provable queries go through [[zheng]]/PCS. both read from fjall.

## algebra-adaptive storage

the noun store holds trees with different-sized leaves depending on the algebra. the tree structure (cells as pairs) is universal — only leaf sizes differ.

| atom type | width | algebra | notes |
|-----------|-------|---------|-------|
| F₂ | 1 bit | Bt programs | compact, massive trees |
| F_p | 64 bits / 8 bytes | field programs | standard |
| word | 32 bits / 4 bytes | word-type | fits in F_p |
| hash | 256 bits / 32 bytes | 4 × F_p identity | CIDs, content addresses (hemera) |

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

- bbg stores particles, maintains polynomial evaluation tables, runs the polynomial [[mutator set]], commits signal batches, computes BBG_root = PCS.commit(BBG_poly), generates and verifies proofs, serves namespace sync. it is the authenticated storage engine.
- [[Ask]] compiles Datalog, optimizes query plans, runs graph algorithms (PageRank, Dijkstra, Louvain), manages HNSW vector indices, bridges interactive queries to provable queries. it is the reasoning engine.

bbg does not know what a query means. [[Ask]] does not know how a proof works. when a provable query is requested, [[Ask]] formulates it and hands the execution plan to bbg, which generates the proof via [[zheng]].

see [[architecture]] for the layer model, [[cross-index]] for why LogUp is eliminated, [[temporal]] for axon decay, [[indexes]] for polynomial evaluation dimensions
