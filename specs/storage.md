---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
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

everything else is derivable: polynomial evaluation tables, particle data, axon aggregates, focus/φ* values — all reconstructible from signal replay.

## storage interface

the polynomial commitment provides authentication. the local data structure provides access. they are INDEPENDENT — the store doesn't know about polynomials, the polynomial doesn't know about storage. see [[data structures for polynomial state]] for the full theory.

```rust
trait ShardStore {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[FieldElement]>;
    fn put(&mut self, dimension: u8, key: &[u8; 32], value: &[FieldElement]);
    fn dirty_entries(&self) -> impl Iterator<Item = (u8, [u8; 32], &[FieldElement])>;
    fn commit(&mut self) -> [u8; 32];  // returns shard sub-commitment
}

/// Content retrieval from the network tier (L3). Injected by cybergraph;
/// BBG holds an optional reference and delegates on local miss. Transport
/// is not owned by BBG — this trait is the only network boundary BBG crosses.
trait NetworkStore: Send + Sync {
    /// Fetch raw particle content by particle. Returns None if unreachable.
    fn fetch(&self, particle: &[u8; 32]) -> Option<Vec<u8>>;
    /// DAS sample: open a Lens proof for a byte range within a particle.
    fn das_sample(&self, particle: &[u8; 32], offset: u64) -> Option<QueryProof>;
}
```

three implementations, selected by scale:

| backend | implementation | optimal for | local structure | latency | when to use |
|---|---|---|---|---|---|
| memory | `std::collections::HashMap` + `bitvec` | shard fits in RAM (≤ 64 GB) | flat array + HashMap + BitVec | 50 ns read | bostrom → city scale |
| unimem | `honeycrisp::unimem` (IOSurface-pinned) | polynomial eval + proof generation, Apple Silicon | IOSurface Blocks (Tape/Grid) | ~1 ns alloc, zero-copy CPU/AMX/GPU/ANE | Apple Silicon nodes (M-series) |
| ssd | `fjall` (LSM-tree, pure Rust) | shard exceeds RAM | LSM-tree with RAM-cached top levels | 20 μs read | nation → planet scale |
| hdd | `redb` (B-tree MVCC, pure Rust) | full history, cold | sorted log + NMT layout index | sequential 200 MB/s | deep replay, research |
| network | `NetworkStore` trait (injected by cybergraph) | L3 content on local miss, DAS sampling | φ*-weighted replication across peers | seconds (retrieval) | particle content not held locally |

`honeycrisp/unimem` is selected for Apple Silicon because IOSurface-backed pinned Blocks give a single physical allocation visible without copying to CPU, AMX matrix coprocessor, Metal GPU, and ANE. Polynomial evaluation tables (field element slices) allocated in a Block are consumed directly by Brakedown matrix ops on AMX and by GPU compute shaders — no memcpy at any stage. `fjall` is selected for SSD because its LSM compaction matches SSD sequential write patterns and high IOPS. `redb` is selected for HDD because its MVCC B-tree supports the namespace range scans needed for NMT layout reads on sequential spinning media. `NetworkStore` is a trait — BBG holds an optional injection from cybergraph; transport is not owned by BBG.

the trend: as storage gets faster, data structures get simpler. trees compensate for slow storage. when access is O(1) (RAM), the tree adds cost without benefit. with GFP (field ops in silicon) + RAM: the data structure disappears. bytes and math.

NMT survives — not for authentication (polynomial handles that) but for cold storage disk layout. sorted namespace = sequential reads = optimal for HDD.

## storage tiers

```
HOT (current state, RAM):
    three polynomial data structures, each with its own Lens commitment:
      BBG_poly (10 public evaluation dimensions) — evaluation table
      A(x) (commitment polynomial) — evaluation table
      N(x) (nullifier polynomial) — evaluation table
    BBG_root = H(Lens.commit(BBG_poly) ‖ Lens.commit(A) ‖ Lens.commit(N)), 32 bytes
    backend: memory (std HashMap, bitvec) or unimem (honeycrisp unimem on Apple Silicon)
    latency: 50 ns / ~1 ns alloc zero-copy
    unimem backend note: field element slices in IOSurface Blocks are read directly
    by AMX (Brakedown matrix ops), Metal GPU, and ANE — no copies between compute units

WARM (recent state, SSD):
    full particle/axon data indexed by particle
    particle energy, φ*, axon weights, market state
    neuron focus/karma/stake, coin/card metadata
    backend: ssd (fjall LSM-tree, RAM-cached top levels)
    latency: 20 μs

CONTENT (files, network):
    particle content (raw bytes), indexed by particle
    DAS availability proofs via files dimension of BBG_poly
    self-authenticating: H(content) = particle
    backend: network (NetworkStore trait, injected by cybergraph)
    latency: seconds (network retrieval)
    miss path: ShardStore miss → NetworkStore.fetch(particle) → cache to ssd

COLD (full history, HDD/network):
    historical state via BBG_poly time dimension
    queryable at any past t via polynomial evaluation
    backend: hdd (redb B-tree, sorted log + NMT layout index)
    latency: sequential 200 MB/s (HDD), minutes for network
```

## tier routing: policy and mechanism

tier placement has two sides:

[[cyb/soma]] sets POLICY — what should live where, based on [[focus]], energy budget, and upcoming computation:

```
soma → bbg:
  pin(particle, tier)        high focus → keep in ram
  prefetch(particle)         upcoming order needs this
  demote(focus_threshold)    energy low → move cold data down
  flush()                    going to sleep → persist all
  budget(max_ram_bytes)      shrink ram usage to save energy
```

bbg executes MECHANISM — physically moves data between tiers, handles LRU eviction, frequency promotion, wear leveling. when soma hasn't expressed preference, bbg falls back to default optimization (access recency, frequency).

rationale for the split: bbg knows storage internals (capacity, IOPS, polynomial structure). soma knows semantics (focus = importance, energy = budget, scheduler = upcoming needs). neither alone makes optimal decisions. together: intelligent storage with semantic caching.

[[focus]] IS the cache priority. high focus = stay in ram. low focus = migrate to cold. the [[tri-kernel]] already computed importance — bbg uses it as the primary eviction signal when soma provides a focus threshold.

when a `look(namespace, key)` executes, bbg resolves the tier transparently — the caller says what it wants, bbg finds it. pricing per tier determined by local energy cost: [[cyb/hal]] exposes hardware reality (RAM capacity, SSD IOPS, HDD bandwidth, power cost) as field-denominated prices.

## storage proofs

six proof types ensure data retention across tiers:

| proof | what it guarantees | mechanism | constraints |
|---|---|---|---|
| storage proof | content bytes exist on node | periodic challenge: offset → chunk + Lens opening | ~5,000 |
| size proof | claimed size matches actual | hemera tree structure + padding check | ~2,000 |
| replication proof | k independent copies exist | challenge k nodes, verify uniqueness | ~5,000 × k |
| retrievability proof | content fetchable in bounded time | timed challenge-response | ~5,000 |
| DAS proof | block data published and accessible | algebraic DAS: erasure + Lens samples | ~3,000 |
| encoding fraud proof | erasure coding correct | decode k+1 cells vs polynomial commitment | O(k) field ops |

signal-first resolves STATE retention: prove signal availability → derive everything via replay. CONTENT retention requires storage proofs + φ*-weighted replication. see [[cyber/proofs]] for the full taxonomy.

## φ*-weighted replication

storage replication factor is proportional to φ* (cyberank). the network spends storage budget where attention goes.

```
replication_factor(particle) = max(R_min, R_base × φ*(particle) / π_median)

  R_min  = minimum replication (e.g., 3 — survival guarantee)
  R_base = baseline replication at median φ* (e.g., 10)

particle class        φ* estimate    replication factor
top-100 particle      ~10⁻²        ~1000 (effectively everywhere)
top-10K particle      ~10⁻⁴        ~100
median particle       ~10⁻⁶        10 (baseline)
tail particle         ~10⁻¹²       3 (minimum)
```

no explicit storage market needed. focus IS the storage payment. φ* IS the replication signal. the economics emerge from the graph topology.

DAS parameters scale with replication:

```
high-φ* particle (1000 replicas):
  base availability very high → fewer DAS samples needed
  5 samples sufficient for 99.99% confidence
  bandwidth: ~2.3 KiB

low-φ* particle (3 replicas):
  base availability minimal → standard sampling
  20 samples for 99.9999% confidence
  bandwidth: ~9 KiB
```

## fjall keyspace layout (ssd backend)

```
fjall keyspace: "bbg"

├── partition: "particles"
│   key:   particle (32 bytes) — hemera hash
│   value: (energy, φ*, axon fields)           particle/axon data
│   polynomial evaluation table for particles dimension
│   content-particles and axon-particles share namespace
│   axon-particles carry: weight A_{pq}, market state (s_YES, s_NO), meta-score
│
├── partition: "axons_out"
│   key:   (source_particle, axon_particle)     sorted by source
│   value: ()                                  presence (pointer to particles)
│   polynomial evaluation table for axons_out dimension
│   "all outgoing from p" = Lens batch opening over this dimension
│
├── partition: "axons_in"
│   key:   (target_particle, axon_particle)     sorted by target
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
│   key:   card_particle                       content hash
│   value: (owner, metadata, name_binding)     non-fungible knowledge assets
│   names resolve through card lookup, not a separate partition
│
├── partition: "files"
│   key:   particle (32 bytes) — hemera hash
│   value: (availability_proof, DAS_metadata)  content availability records
│   polynomial evaluation table for files dimension
│
├── partition: "commitments"
│   key:   commitment_point                    F_p
│   value: commitment_value                    F_p
│   independent polynomial A(x) evaluation table (NOT a BBG_poly dimension)
│   append-only — new records extend the polynomial
│   own Lens commitment: Lens.commit(A), 32 bytes
│
├── partition: "nullifiers"
│   key:   nullifier_point                     F_p
│   value: zero_marker                         indicates spent
│   independent polynomial N(x) evaluation table (NOT a BBG_poly dimension)
│   N(x) = ∏(x - n_i), own Lens commitment: Lens.commit(N), 32 bytes
│
├── partition: "time"
│   key:   (boundary_index)
│   value: BBG_poly evaluation snapshot        queryable at any past t
│   time is a native dimension of BBG_poly
│   any historical query = one Lens opening at (index, key, t_past)
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
| recommit BBG_poly | all polynomial partitions | batch read + Lens commit | validator (per block) |
| generate Lens opening | polynomial state | proof generation | validator (on demand) |
| verify Lens opening | — | proof verification | light client |
| namespace sync response | any partition | range scan | validator |
| namespace sync receive | partitions | batch write | light client |
| extend commitment polynomial | commitments | append | validator |
| extend nullifier polynomial | nullifiers | append | validator |
| Datalog query | cozo_* | CozoDB query plan | both |
| name resolution | cards | point lookup by name binding | both |
| temporal query | any dimension | Lens opening at (index, key, t_past) | both |
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
  private record exists in commitment polynomial (provable via Lens opening of A(c))
  public aggregates reflect the sum of all active private records

spending:
  1. neuron proves ownership of the private record (Lens opening of A(c) + secret)
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
7. recommit all three polynomials: BBG_poly, A(x), N(x) via Lens (batch evaluation changes, one recommitment each)
8. fold into [[zheng]]-2 accumulator (constant-size checkpoint)
9. emit changeset — CozoDB applies incremental updates

step 7 is the polynomial recommitment. batch all changed evaluations and recommit each polynomial (BBG_poly, A(x), N(x)) once per block. recompute BBG_root = H(Lens.commit(BBG_poly) ‖ Lens.commit(A) ‖ Lens.commit(N)). cost: O(|changes|) field operations — no tree path rehashing.

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

## owner store (personal BBG)

the chain always holds encrypted values. a personal BBG node holds plaintext for its own UTXOs.

```
chain BBG:    commitments partition   key = c, value = commit_jali(v, ρ)   RLWE ciphertext (n field elements)
personal BBG: owner partition        key = c, value = (v, ρ)               plaintext amount + randomness
```

the owner needs `ρ` to reconstruct future spends and re-prove ownership. storing only `v` is insufficient.

two operations bridge the layers:

```
submit (plaintext → chain):
  (v, ρ) → commit_jali(v, ρ)   encrypt at signal construction time
  ciphertext published to A(x), plaintext stays local

scan (chain → plaintext):
  commit_jali(v, ρ) → v        decrypt with owner's ρ, derived from key
  plaintext written to owner partition for local computation
```

privacy is not optional at the protocol level — chain state is always encrypted. the personal store is a local decrypted index over the owner's own UTXOs. validators never see plaintext amounts.

## validator vs light client

| | validator | light client |
|---|---|---|
| particles | all | synced namespaces only |
| axons_out, axons_in | full | synced ranges |
| neurons | full | full (public data) |
| locations | full | synced ranges |
| coins, cards, files | full | synced namespaces |
| commitments (A(x)) | full | own Lens proofs only |
| nullifiers (N(x)) | full | own Lens proofs only |
| time | full (all evaluations) | queried via Lens openings |
| signals | full | headers + verified proofs |
| cozo_* | full materialized view | partial view over synced data |
| storage keyspace | same layout (memory/ssd/hdd) | same layout, less data |

one codebase, one storage format, one API. `bbg::open(path)` returns the same interface regardless of role. the sync protocol fills in whatever is missing.

## CozoDB integration

[[CozoDB]] uses fjall as its storage backend via a pluggable backend trait. both CozoDB and bbg read/write the same fjall instance. CozoDB's relations are different sort orders over the same underlying data — not copies.

```
Ask query:  "all axons where source = X"
  → CozoDB translates to fjall range scan on axons_out
  → returns results immediately (local, trusted)

network query:  "prove all axons where source = X"
  → same fjall range scan on axons_out
  → Lens batch opening proof generation from BBG_poly
  → returns results + proof (trustless)
```

same data, same storage, two access modes. interactive queries go through CozoDB/[[Ask]]. provable queries go through [[zheng]]/Lens. both read from fjall.

## polynomial particle storage

particle storage = polynomial evaluation table storage. when particles are polynomial nouns, the content store holds evaluation tables of particle polynomials. ShardStore serves polynomial nouns natively: `get(dimension, key)` returns field elements that are polynomial evaluations.

the same backend stores BBG_poly evaluation tables (aggregate state: energy, pi-star, axon weights) AND individual particle polynomials (content data). the fjall "particles" partition holds both: the BBG_poly dimension entries for aggregate queries, and the particle's own polynomial evaluation table for content access.

```
"particles" partition serves two polynomial levels:
  BBG_poly(particles, particle, t) → aggregate state (energy, φ*, axon fields)
  particle_poly(particle, position) → content bytes at any offset

both are polynomial evaluations. both use Lens openings for proofs.
both live in the same fjall partition, keyed by particle.
```

Lens.open on BBG_poly answers "what is the energy of particle P?" Lens.open on the particle's own polynomial answers "what are bytes 1024..2048 of particle P?" same mechanism, same proof format, same verification.

## algebra-adaptive storage

the noun store holds trees with different-sized leaves depending on the algebra. the tree structure (cells as pairs) is universal — only leaf sizes differ.

| atom type | width | algebra | notes |
|-----------|-------|---------|-------|
| F₂ | 1 bit | Bt programs | compact, massive trees |
| F_p | 64 bits / 8 bytes | field programs | standard |
| word | 32 bits / 4 bytes | word-type | fits in F_p |
| hash | 256 bits / 32 bytes | 4 × F_p identity | particles, content addresses (hemera) |

the content-addressed store handles all leaf widths. `H(noun)` hashes the canonical serialization regardless of leaf size — a noun with bit-leaves and a noun with field-leaves both live in the same "particles" partition, keyed by their hash.

different algebras produce different memory access patterns on the same noun store:

- **field programs** (Tri, Wav, Ten): dense trees with F_p leaves. sequential access — matrix ops produce predictable axis paths. cache-friendly. dominated by fma-pattern workloads.
- **binary programs** (Bt): ultra-compact trees with bit-sized leaves. very large trees (a SHA-256 circuit is millions of gates). bandwidth-bound.
- **graph programs** (Arc): sparse trees with hash-type leaves (particles pointing to other nouns). random access patterns. latency-bound.
- **mixed programs** (Rs): trees with both field and word leaves. irregular access.

the existing keyspace layout handles this naturally — particles are content-addressed by hash regardless of leaf type. hot-path optimization should consider which partitions are accessed by which algebra patterns. cache eviction policy, prefetch strategy, and fjall block size all benefit from knowing the dominant algebra in the current workload.

## dependency graph

```
redb (hdd)   fjall (ssd)   HashMap (memory)   unimem (Apple Silicon)   NetworkStore (trait)
      ↑             ↑              ↑                    ↑                      ↑
                              bbg (authenticated state logic)
                                  ↑               ↑
                               CozoDB           zheng
                              (queries)        (proofs)

NetworkStore impl lives in cybergraph/radio — injected into bbg at init.
BBG owns tier routing; does not own transport.
```

bbg owns the fjall keyspace. CozoDB and zheng are consumers — CozoDB for interactive Datalog queries, zheng for proof generation and verification. neither knows about the other. bbg mediates.

## the bbg / ask boundary

bbg answers: is this data authentic? (proofs)
[[Ask]] answers: what does this data mean? (queries)

every query CAN become a proof — [[Ask]] formulates the Datalog query, bbg proves the result via [[zheng]]. the boundary is not about what is provable, but about responsibility:

- bbg stores particles, maintains polynomial evaluation tables, runs the polynomial [[mutator set]], commits signal batches, computes BBG_root = H(Lens.commit(BBG_poly) ‖ Lens.commit(A) ‖ Lens.commit(N)), generates and verifies proofs, serves namespace sync. it is the authenticated storage engine.
- [[Ask]] compiles Datalog, optimizes query plans, runs graph algorithms (PageRank, Dijkstra, Louvain), manages HNSW vector indices, bridges interactive queries to provable queries. it is the reasoning engine.

bbg does not know what a query means. [[Ask]] does not know how a proof works. when a provable query is requested, [[Ask]] formulates it and hands the execution plan to bbg, which generates the proof via [[zheng]].

see [[architecture]] for the layer model, [[cross-index]] for why LogUp is eliminated, [[temporal]] for axon decay, [[indexes]] for polynomial evaluation dimensions
