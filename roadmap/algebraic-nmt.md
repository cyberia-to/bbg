---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-18
diffusion: 0.00010722364868599256
springs: 0.00007019991600688145
heat: 0.00003419142694206788
focus: 0.00008151008453347325
gravity: 0
density: 0
---
# algebraic NMT — from hash trees to polynomial state

## rationale

the cybergraph is a knowledge structure at planetary scale. every edge (cyberlink), every node (particle), every agent (neuron) must be authenticated — the graph state is the shared truth of the network. the question is: what is the right mathematical object to commit to this state?

the current answer is Namespace Merkle Trees (NMTs). NMTs are hash trees where internal nodes carry namespace bounds, providing completeness proofs: if you ask "show me all edges from particle P," the tree's sorting invariant guarantees that nothing can be hidden. this is a strong property — it is structural, unconditional, and independent of any computational assumption.

but NMTs have a cost that scales with the structure they authenticate. every cyberlink touches 4-5 independent trees. every tree update rehashes a path from leaf to root. at 4 billion entries per index and 9 independent indexes, a single cyberlink costs ~106,000 constraints — and most of that cost is hemera hash recomputation along tree paths that share the same underlying data.

the fundamental observation: the 9 NMT trees contain the SAME information viewed from different angles. particles.root indexes by content. axons_out.root indexes by source. axons_in.root indexes by target. neurons.root indexes by agent. the data is one — the indexes are many. maintaining them as separate hash trees forces redundant computation proportional to the number of views, not the amount of change.

a polynomial commitment is a single mathematical object that can be evaluated at any point. committing to BBG_poly(index, key) creates ONE binding that simultaneously authenticates all 9 views. querying any view is a polynomial opening — a proof that the committed polynomial evaluates to the claimed value at the queried point. cross-index consistency becomes automatic: the polynomial is one object, so different views cannot disagree.

this is a shift from **authenticating structure** (hash tree over data) to **authenticating function** (polynomial over the evaluation domain). the data is the same. the proof mechanism changes. the cost drops from O(views × log n × hash) to O(update_count × field_ops).

## what is lost and what is gained

the shift is not free. NMTs provide structural completeness — the sorting invariant is a property of the data structure itself, not a cryptographic assumption. a tree with the NMT invariant cannot omit entries from a namespace query. this is law 3 of bbg: structural security.

polynomial completeness relies on the PCS (polynomial commitment scheme) being sound — a computational assumption. if the PCS breaks, the polynomial can lie. this is a weaker guarantee than structural.

but the shift buys something NMTs cannot provide: **unified state**. with 9 independent NMT trees, every cross-index operation requires a separate consistency proof (LogUp). with one polynomial, consistency is definitional — different evaluation dimensions of the same polynomial cannot disagree. LogUp becomes unnecessary. the consistency guarantee is not just cheaper — it is stronger, because it is structural within the algebraic framework.

the question resolves to: is PCS soundness (computational, well-studied, post-quantum for hash-based PCS) a sufficient foundation for the cybergraph's public state? the answer is yes, with a migration path that maintains NMT verification as a redundant safety check during the transition.

## the three cost centers

every state-modifying operation in bbg pays three costs:

```
1. state update:      modify the committed data structure
2. consistency:       prove cross-index agreement
3. completeness:      prove nothing was omitted from queries
```

| cost center | NMT approach | algebraic approach |
|-------------|-------------|-------------------|
| state update | O(views × log n) hemera hashes | O(views) field ops (or O(1) for unified poly) |
| consistency | LogUp argument (~500 constraints per lookup) | free (same polynomial) |
| completeness | structural (sorting invariant) | algebraic (LogUp range check or sorted evaluation) |

the cost reduction is multiplicative: fewer hash computations × eliminated LogUp × smaller proofs.

## current architecture

bbg maintains 9 public NMT indexes, each an independent hemera Merkle tree:

```
particles.root     NMT[CID → particle_record]        leaf: 48-96 bytes
axons_out.root     NMT[source_CID → axon_pointer]    leaf: 64 bytes
axons_in.root      NMT[target_CID → axon_pointer]    leaf: 64 bytes
neurons.root       NMT[neuron_id → neuron_record]     leaf: 56 bytes
locations.root     NMT[neuron_id → location_record]   leaf: 104 bytes
coins.root         NMT[denom_hash → supply_record]    leaf: 81 bytes
cards.root         NMT[card_id → card_record]         leaf: 104 bytes
files.root         NMT[CID → availability_record]     leaf: 76 bytes
time.root          NMT[time_ns → BBG_root_snapshot]   leaf: 72 bytes
```

each NMT is a binary Merkle tree with hemera hash nodes (32 bytes, 1 permutation per node). NMT internal nodes carry namespace min/max labels for completeness proofs.

## cost analysis: NMT

### per-cyberlink update

a cyberlink (ν, p, q, τ, a, v, t) touches 4-5 indexes:

```
1. particles.root    energy update for source particle p      O(log n) path
2. particles.root    energy update for target particle q      O(log n) path
3. axons_out.root    insert/update axon H(p,q) under p       O(log n) path
4. axons_in.root     insert/update axon H(p,q) under q       O(log n) path
5. neurons.root      focus deduction for neuron ν             O(log n) path
```

each path update rehashes from leaf to root:

```
at n = 2³² (4 billion entries per index):
  tree depth: 32
  hemera calls per path: 32 (one per level, 1 perm/node)
  constraints per path: 32 × 736 = 23,552

  per cyberlink: ~4.5 paths × 23,552 = ~106,000 constraints
```

### per-block cost

```
block with 1000 cyberlinks:
  NMT path updates:    1000 × 4.5 × 32 = 144,000 hemera permutations
  constraints:         144,000 × 736 = ~106M constraints
  LogUp cross-index:   ~500 × K constraints (K = distinct axons)
    at K = 4000:       ~2M constraints
  total:               ~108M constraints per block
```

### with batched deduplication

many cyberlinks touch the same particles (power-law). deduplicating path updates:

```
1000 cyberlinks, 200 unique particles affected (80% overlap):
  deduplicated paths: 200 × 4 + 1000 × 1 = 1,800 paths (vs 4,500)
  hemera permutations: 1,800 × 32 = 57,600
  constraints: 57,600 × 736 = ~42M constraints
  savings vs naive: ~2.5×
```

### LogUp cross-index cost

```
per axon update: ~500 constraints
K = 4000 axons per block: ~2M constraints
prover: O(K log K) field operations
verifier: O(log K) field operations
```

## cost analysis: algebraic NMT

### polynomial representation

all 9 public indexes encoded as evaluations of a single multivariate polynomial:

```
BBG_poly(index, namespace, position) = value

where:
  index ∈ {0..8}          which of the 9 NMT indexes
  namespace ∈ F_p          namespace key (CID, neuron_id, etc.)
  position ∈ [0, n_i)     position within namespace

committed via zheng PCS: BBG_commitment = PCS.commit(BBG_poly)
```

### per-cyberlink update

```
with Verkle-tree approach at n = 2³²:
  update: O(32) field operations + O(32) PCS node updates
  each PCS node update: ~100 field ops (vs 736 for hemera path hash)
  total: ~3,200 constraints

savings vs NMT: 106,000 / 3,200 ≈ 33×
```

### per-block cost

```
block with 1000 cyberlinks:

  Verkle-tree approach:
    with deduplication (200 unique particles): ~5.8M field ops
    + batch PCS recommit: ~2M field ops
    total: ~7.8M constraints

  Brakedown flat polynomial:
    batch recommit of changed evaluations: ~7.2M field ops
    + batch PCS opening: ~50K constraints
    total: ~7.3M constraints

  vs current NMT: ~108M constraints
  savings: 14-15×
```

### LogUp elimination

```
current:
  axons_out.root ←LogUp→ axons_in.root ←LogUp→ particles.root
  cost: ~500 constraints per lookup × 3 lookups per cyberlink
  per block (K=4000): ~6M constraints

algebraic NMT:
  BBG_poly(axons_out, P, _) and BBG_poly(axons_in, P, _)
  are evaluations of the SAME committed polynomial
  consistency: free (same commitment, different evaluation dimensions)
  cost: 0 additional constraints
```

## comprehensive comparison

### per-cyberlink

```
                            NMT (current)    NMT + dedup    Algebraic NMT
hemera permutations:        ~144             ~58            0
constraints:                ~106K            ~43K           ~3.2K
LogUp constraints:          ~1.5K            ~1.5K          0
total constraints:          ~107.5K          ~44.5K         ~3.2K

improvement vs current:     1×               2.4×           33×
```

### per-block (1000 cyberlinks)

```
                            NMT (current)    NMT + dedup    Algebraic NMT
total constraints:          ~108M            ~44M           ~7.5M
hemera permutations:        144,000          57,600         0
LogUp constraints:          ~6M              ~6M            0
PCS recommit:               0                0              ~50K

improvement vs current:     1×               2.5×           14×
```

### proof size

```
                            NMT              Algebraic NMT
inclusion proof:            32 × 32B = 1 KiB  ~200 bytes (PCS opening)
completeness (range=100):   ~100 KiB          ~5 KiB
light client per namespace: ~1 KiB            ~200 bytes
```

### storage

```
                            NMT              Algebraic NMT
internal nodes per index:   (2n-1) × 64B     0 (no tree)
  at n = 2³²:              ~550 GB           0
total for 9 indexes:        ~5 TB             ~288 bytes (9 commitments)
```

leaf data is the same in both approaches. the polynomial replaces the tree structure, not the data.

## completeness: structural vs algebraic

### NMT completeness (structural, unconditional)

```
mechanism: sorting invariant at internal nodes
  each node carries [ns_min, ns_max] of its subtree
  parent.ns_min = min(left.ns_min, right.ns_min)
  parent.ns_max = max(left.ns_max, right.ns_max)

completeness proof for namespace N:
  walk tree, collect all leaves where ns_min ≤ N ≤ ns_max
  the sorting invariant guarantees: if a leaf with namespace N exists,
  it MUST appear in this walk. omission is structurally impossible.

strength: the tree CANNOT lie by construction
weakness: tree must be materialized (O(n) storage per index)
security: unconditional (information-theoretic)
```

### algebraic completeness (computational)

**LogUp range check:**
```
for range [a, b]:
  Σ 1/(β - f_i) for all i in committed set where a ≤ ns_i ≤ b
  must equal
  Σ 1/(β - t_j) for the claimed complete subset

missing entry → sums don't match → proof rejected

Schwartz-Zippel error: degree / |F_p| ≈ n / 2^64 ≈ 2^{-32} for n = 2^32
with extension field F_{p²}: error ≈ 2^{-96}
```

**sorted polynomial evaluation:**
```
polynomial encodes (namespace, value) pairs sorted by namespace
range query [a, b]: open consecutive positions
verifier checks: consecutive positions, correct bounds, matching values

simple verification, no lookup argument
updates require maintaining sort order
```

### comparison

| property | NMT (structural) | algebraic (LogUp) | algebraic (sorted) |
|----------|-------------------|--------------------|--------------------|
| completeness | unconditional | computational (PCS) | computational (PCS) |
| storage overhead | O(n) per index | 0 | 0 |
| update cost | O(log n) hemera | O(1)-O(√N) field | O(n) re-sort |
| proof size | O(log n) × 32B | O(1) PCS opening | O(range) field |
| cross-index | LogUp needed | free | free |
| law 3 | yes (structural) | partial | partial |
| quantum | yes | post-quantum (hash-PCS) | post-quantum (hash-PCS) |
| failure mode | corruption (detectable) | PCS break (catastrophic) | PCS break (catastrophic) |

### resolving law 3

NMT completeness is structural. algebraic completeness is computational. this is a weaker guarantee.

the resolution: completeness shifts from being a property of the DATA STRUCTURE to a property of the COMMITMENT SCHEME. the commitment scheme (WHIR, Brakedown) is itself hash-based — it relies on hemera's collision resistance. hemera's security is the same foundation that NMTs rest on (hemera hashes the NMT nodes). the trust root is the same: hemera.

the difference: NMTs use hemera structurally (the tree shape encodes completeness). polynomial commitments use hemera algebraically (the polynomial evaluation encodes completeness). both ultimately trust hemera. the algebraic path is more efficient because it expresses the same guarantee with fewer hash calls.

the hybrid migration path provides defense in depth: during transition, both NMT and polynomial agree on every query. divergence indicates either a bug or a fundamental break. years of agreement build confidence for removing the NMT layer.

## BBG_root evolution

```
current (13 sub-roots):
  BBG_root = H(particles.root ‖ axons_out.root ‖ ... ‖ time.root ‖
               cyberlinks.root ‖ spent.root ‖ balance.root ‖ signals.root)
           = H(416 bytes)

phase 1 (algebraic public + hash private):
  BBG_root = H(BBG_commitment ‖ cyberlinks.root ‖ spent.root ‖ balance.root ‖ signals.root)
           = H(160 bytes)
  9 NMT sub-roots → 1 PCS commitment (32 bytes)

phase 3 (endgame, see [[unified-polynomial-state]]):
  BBG_root = PCS.commit(BBG_poly)
           = 32 bytes
  everything → 1 polynomial
```

## PCS backend analysis

### WHIR

```
strengths: existing zheng backend, hemera-native, transparent, post-quantum
weaknesses: NOT cheaply updatable (full recommit = O(N log N))
update strategy: batch all changes, recommit once per block
cost per block: O(N_changed × log N) for recommit
verdict: viable for batched block updates, not for incremental
```

### Brakedown

```
strengths: Merkle-free, O(√N) proof size, linear-time commitment, post-quantum
weaknesses: larger proofs than WHIR, expander graph construction
update strategy: linear-time recommit
cost per block: O(N) for full recommit
verdict: best for flat polynomial, full-block batch updates
```

### Verkle-tree hybrid

```
strengths: O(log N) incremental updates, familiar tree structure, migration-friendly
weaknesses: still a tree (O(n) storage for nodes), more complex
update strategy: update path from leaf to root, O(log N) PCS node updates
cost per update: O(log N) × ~100 field ops ≈ ~3,200 constraints
verdict: best for phase 1 — preserves tree structure, 33× cheaper than NMT
```

### recommended sequence

```
phase 1: Verkle-tree with WHIR at nodes
  tree structure preserved → easy migration from NMT
  per-update: ~3,200 constraints (vs ~106K for NMT)
  NMT retained as secondary verification

phase 2: Brakedown flat polynomial
  tree eliminated → 0 storage overhead
  per-block batch: O(N) recommit
  completeness: LogUp range check over F_{p²}
  NMT removed after sufficient deployment confidence

phase 3: unified polynomial state
  all 9 indexes → one polynomial (see [[unified-polynomial-state]])
  cross-index: structural (same polynomial)
  LogUp: eliminated
```

## near-term stepping stone: batched deduplication

before algebraic NMT, a zero-architecture-change optimization:

```
mechanism: dirty-set tracking per block
  accumulate leaf changes during block processing
  deduplicate by (index, namespace) key
  recompute hemera paths once at block boundary

1000 cyberlinks, 200 unique particles (80% overlap):
  naive:    ~106M constraints
  batched:  ~42M constraints (2.5×)

hot particle (100 updates in one block):
  naive:    ~2.4M constraints
  batched:  ~23.5K constraints (100×)

implementation: BoundedMap for dirty-set. hemera remains sole mechanism. no PCS.
```

## risk analysis

| risk | severity | mitigation |
|------|----------|------------|
| PCS soundness break | catastrophic | hybrid NMT verification in phase 1-2 |
| algebraic completeness weaker than structural | high | LogUp over F_{p²} (2^{-96} error) |
| Verkle-tree complexity | medium | leverage existing NMT tree structure |
| WHIR non-updatable | medium | batch at block boundary, migrate to Brakedown |
| quantum threat to PCS | low | WHIR/Brakedown are hash-based (post-quantum) |
| polynomial degree (9 × 2³²) | medium | sparse representation, per-index in phase 1 |

## structural sync: layer 3 evolution

algebraic NMT is the evolution of [[structural sync|structural-sync]] layer 3 (completeness). the completeness PROPERTY is preserved — the MECHANISM changes:

```
NMT (current):        structural completeness — sorting invariant prevents omission
algebraic NMT:        algebraic completeness — PCS binding prevents omission
```

this has a cascade effect on layer 4 (availability). DAS currently uses NMT inclusion proofs for sampling verification. with algebraic NMT, DAS should use polynomial openings:

```
current DAS:          sample cell → NMT inclusion proof (~1 KiB per sample)
algebraic DAS:        sample cell → PCS opening (~200 bytes per sample)
verification:         O(log n) hemera hashes → O(1) field ops per sample

20 samples for 99.9999% confidence:
  current:   20 × 1 KiB = 20 KiB bandwidth, 20 × O(log n) hemera verifications
  algebraic: 20 × 200B = 4 KiB bandwidth, 20 × O(1) field verifications
```

algebraic DAS is not a separate proposal — it is a natural consequence of algebraic NMT applied to layer 4. when the completeness layer becomes algebraic, the availability layer inherits the efficiency.

with batched-proving ([[batched-proving]]), the remaining hemera costs (content identity, private records) are batched: 1000 hemera calls per block → 1 sumcheck. the combination: algebraic NMT eliminates hemera from state verification, batched-proving compresses hemera for identity operations.

## open questions

1. **F_{p²} readiness**: LogUp challenge over extension field gives 2^{-96} Schwartz-Zippel error. nebu needs F_{p²} arithmetic. cost of extension field operations?
2. **Verkle branching factor**: binary (like NMT) or wider (arity 256)? wider = shallower tree = fewer PCS updates per path, but more expensive per-node commitment
3. **dimension encoding**: separate polynomial per index (9 commitments, simple) vs unified polynomial with index selector (1 commitment, LogUp-free). phase 1 uses per-index; phase 3 unifies
4. **update clustering**: power-law access patterns suggest high clustering for top particles. empirical analysis on real cyberlink workloads determines deduplication effectiveness
5. **algebraic DAS integration**: when NMTs → polynomials, DAS proofs should use PCS openings. does the existing 2D Reed-Solomon grid structure compose naturally with multivariate polynomial commitment? or does the erasure coding scheme need adaptation?

see [[unified-polynomial-state]] for the endgame, [[cross-index]] for current LogUp, [[indexes]] for NMT structure, [[structural-sync]] for the five-layer framework