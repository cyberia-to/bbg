---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-18
---
# algebraic NMT — polynomial commitment replaces Merkle trees

replace 9 independent NMT trees with a unified polynomial commitment. a single cyberlink currently triggers 4-5 independent O(log n) hemera path updates costing ~94K constraints. algebraic NMT reduces this to ~3K constraints. the proposal is the single largest optimization available in bbg.

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

each NMT is a binary Merkle tree with hemera-2 hash nodes (32 bytes, 1 permutation per node). NMT internal nodes carry namespace min/max labels for completeness proofs.

## cost analysis: current NMT approach

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
  hemera calls per path: 32 (one per level, hemera-2: 1 perm/node)
  constraints per path: 32 × 736 = 23,552

  per cyberlink: ~4.5 paths × 23,552 = ~106,000 constraints
  (using 4.5 average paths; some cyberlinks touch fewer indexes)
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

### per-block with batched deduplication

many cyberlinks touch the same particles (power-law). deduplicating path updates:

```
1000 cyberlinks, 200 unique particles affected (80% overlap):
  deduplicated paths: 200 × 4 + 1000 × 1 = 1,800 paths (vs 4,500)
  hemera permutations: 1,800 × 32 = 57,600
  constraints: 57,600 × 736 = ~42M constraints
  savings vs naive: ~2.5×
```

### LogUp cross-index cost

axons_out, axons_in, and particles must stay consistent. LogUp proves set containment:

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
update one evaluation point:
  PCS update cost depends on scheme:

  WHIR (current):          NOT cheaply updatable — full recommit O(N log N)
  Brakedown (proposed):    O(√N) update cost
  Verkle-tree hybrid:      O(log N) update cost (polynomial at each node)

  with Verkle-tree approach at n = 2³²:
    update: O(32) field operations + O(32) PCS node updates
    each PCS node update: ~100 field ops (vs 736 for hemera path hash)
    total: ~3,200 field ops ≈ ~3,200 constraints

  savings vs NMT: 106,000 / 3,200 ≈ 33×
```

### per-block cost

```
block with 1000 cyberlinks:

  Verkle-tree approach:
    1000 × 4.5 updates × 32 depth × ~100 field ops = ~14.4M field ops
    with deduplication (200 unique particles): ~5.8M field ops
    + batch PCS recommit: O(N_changed × log N) ≈ ~2M field ops
    total: ~7.8M constraints

  vs current NMT: ~108M constraints
  savings: ~14×

  with full polynomial (Brakedown, incremental):
    1000 × 4.5 updates × O(√N) = 4,500 × ~65,000 ≈ ~290M field ops
    too expensive for per-update — batch at block boundary
    batch: 1 recommit of changed evaluations: O(changed × log N)
    at 4500 changes: ~4,500 × 32 × ~50 = ~7.2M field ops
    + 1 batch PCS opening for the block: ~50K constraints
    total: ~7.3M constraints

  savings: ~15×
```

### LogUp elimination

with unified polynomial, cross-index consistency is structural:

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

this is a pure win — LogUp cost drops to zero for cross-index consistency.

## comprehensive comparison

### per-cyberlink

```
                            NMT (current)    NMT + dedup    Algebraic NMT
hemera permutations:        ~144             ~58            0
field operations:           ~106K            ~43K           ~3.2K
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

### proof size impact

```
                            NMT              Algebraic NMT
inclusion proof:            O(log n) × 32B   O(1) PCS opening
  at n = 2³²:              32 × 32 = 1 KiB  ~200 bytes
completeness proof:         O(range × log n)  O(range) field ops
  for range = 100:          ~100 KiB          ~5 KiB

light client sync cost:     ~1 KiB per namespace  ~200 bytes per namespace
```

### storage per index

```
                            NMT              Algebraic NMT
internal nodes:             (2n-1) × 64B     0 (no tree)
  at n = 2³²:              ~550 GB per index  0
commitment overhead:        32 bytes (root)   32 bytes (PCS commitment)
total for 9 indexes:        ~5 TB             ~288 bytes (9 commitments)

node savings:               5 TB → 0 (internal nodes eliminated)
```

the polynomial commitment replaces the entire tree structure. leaf data still stored (same in both approaches).

## completeness: NMT structural vs algebraic

the deepest design question. bbg's three laws require structural security (law 3): guarantees from data structure invariants, not protocol correctness.

### NMT completeness (structural)

```
mechanism: sorting invariant at internal nodes
  each node carries [ns_min, ns_max] of its subtree
  parent: ns_min = min(left.ns_min, right.ns_min)
          ns_max = max(left.ns_max, right.ns_max)

completeness proof for namespace N:
  walk tree to find all leaves where ns_min ≤ N ≤ ns_max
  the sorting invariant guarantees: if a leaf with namespace N exists,
  it MUST appear in this walk. omission is structurally impossible.

strength: the tree CANNOT lie by construction
weakness: tree must be materialized (O(n) storage per index)
security: unconditional (information-theoretic)
```

### algebraic completeness (protocol)

two approaches:

**LogUp range check:**
```
mechanism: algebraic set identity
  for range [a, b]:
    Σ 1/(β - f_i) for all i in committed set where a ≤ ns_i ≤ b
    must equal
    Σ 1/(β - t_j) for the claimed complete subset

  missing entry → sums don't match → proof rejected

strength: no tree materialization needed
weakness: relies on PCS binding + Schwartz-Zippel over Goldilocks
security: computational (PCS soundness, field size)
  Schwartz-Zippel error: degree / |F_p| ≈ n / 2^64 ≈ 2^{-32} for n = 2^32
  with extension field F_{p²}: error ≈ 2^{-96} (sufficient)
```

**sorted polynomial evaluation:**
```
mechanism: committed sorted sequence
  polynomial encodes (namespace, value) pairs sorted by namespace
  range query [a, b]: open consecutive positions
  verifier checks: positions are consecutive, bounds correct, values match

strength: simple verification, no lookup argument
weakness: updates require re-sorting (expensive for unsorted insertions)
security: PCS binding (computational)
```

### comparison

| property | NMT (structural) | algebraic (LogUp) | algebraic (sorted) |
|----------|-------------------|--------------------|--------------------|
| completeness guarantee | unconditional | computational (PCS) | computational (PCS) |
| storage overhead | O(n) per index | 0 (polynomial) | 0 (polynomial) |
| update cost | O(log n) hemera | O(1)-O(√N) field ops | O(n) re-sort (amortized) |
| proof size | O(log n) × 32B | O(1) PCS opening | O(range) field ops |
| cross-index consistency | LogUp needed | free (same poly) | free (same poly) |
| law 3 compliance | yes (structural) | partial (computational) | partial (computational) |
| quantum resistance | yes | depends on PCS | depends on PCS |
| failure mode | tree corruption (detectable) | PCS break (catastrophic) | PCS break (catastrophic) |

### law 3 resolution

NMT completeness is structural — the data structure itself prevents omission. algebraic completeness relies on the PCS soundness assumption. this is a weaker guarantee.

mitigation: the hybrid migration path (phase 1-2) maintains NMTs as secondary verification. the polynomial commitment is the primary authority, but NMTs remain as an independent check. a PCS break would be caught by the NMT disagreement.

long-term (phase 3): if the PCS has survived years of deployment and cryptanalytic scrutiny, NMTs can be safely removed. this follows the same trust trajectory as any cryptographic primitive — initially verified by redundancy, eventually trusted independently.

## PCS backend analysis

which PCS works best for algebraic NMT?

### WHIR

```
strengths: existing zheng backend, hemera-native, transparent
weaknesses: NOT cheaply updatable (full recommit = O(N log N))
update strategy: batch all changes, recommit once per block
cost per block: O(N_changed × log N) for recommit
suitable: yes, but batched updates only (no per-cyberlink incremental)
```

### Brakedown

```
strengths: Merkle-free, O(√N) proof size, linear-time commitment
weaknesses: larger proofs than WHIR, expander graph construction
update strategy: linear-time recommit — faster than WHIR for large N
cost per block: O(N) for full recommit (linear)
suitable: yes, best for full-block batch updates
```

### Verkle-tree hybrid

```
strengths: O(log N) incremental updates, familiar tree structure
weaknesses: still a tree (O(n) storage for nodes), more complex
update strategy: update path from leaf to root, O(log N) PCS node updates
cost per update: O(log N) × PCS_node_cost ≈ O(log N) × 100 field ops
suitable: best for per-cyberlink incremental updates

this is the recommended approach for phase 1-2:
  tree structure preserved (easy migration from NMT)
  per-update cost: ~3,200 constraints (vs ~106K for NMT)
  storage: similar to NMT (tree nodes exist, but with polynomial commitments)
```

### recommendation

```
phase 1: Verkle-tree with WHIR at nodes
  - tree structure preserved → easy migration from NMT
  - per-update: O(log n) × ~100 = ~3,200 constraints
  - completeness: sorted leaves + polynomial openings
  - NMT retained as secondary verification

phase 2: Brakedown flat polynomial
  - tree eliminated → O(1) storage overhead
  - per-block batch: O(N) recommit (linear)
  - completeness: LogUp range check
  - NMT removed after sufficient confidence

phase 3 (endgame): unified polynomial state
  - all 9 indexes + time → one polynomial (see [[unified-polynomial-state]])
  - per-query: O(1) PCS opening
  - cross-index: structural (same polynomial)
```

## near-term: batched deduplication

before algebraic NMT, a stepping stone captures intra-block savings with zero architectural change:

```
mechanism: dirty-set tracking per block
  accumulate leaf changes during block processing
  deduplicate by (index, namespace) key
  recompute hemera paths once at block boundary

scenario: 1000 cyberlinks, 200 unique particles (80% overlap)
  naive:    4,500 path updates × 32 depth × 736 = ~106M constraints
  batched:  1,800 path updates × 32 depth × 736 = ~42M constraints
  savings:  ~2.5×

hot particle (updated 100× in one block):
  naive:    100 path updates = 100 × 32 × 736 = ~2.4M constraints
  batched:  1 path update = 32 × 736 = ~23.5K constraints
  savings:  100×
```

implementation complexity: low (dirty-set is a BoundedMap). hemera remains sole commitment mechanism. no PCS dependency.

## risk analysis

| risk | severity | mitigation |
|------|----------|------------|
| PCS soundness break | catastrophic | hybrid NMT verification in phase 1-2 |
| algebraic completeness weaker than structural | high | LogUp over extension field F_{p²} (2^{-96} error) |
| Verkle-tree implementation complexity | medium | leverage existing NMT tree structure |
| WHIR non-updatable | medium | batch at block boundary, migrate to Brakedown |
| quantum threat to PCS | low-medium | WHIR/Brakedown are post-quantum (hash-based) |
| polynomial degree explosion (9 indexes × 2³²) | medium | sparse representation, per-index polynomials in phase 1 |

## open questions

1. **extension field for LogUp**: using F_{p²} for the LogUp challenge β gives Schwartz-Zippel error 2^{-96}. is nebu ready for F_{p²} arithmetic? cost of extension field operations?
2. **Verkle branching factor**: binary Verkle (like NMT) or wider (arity 256)? wider = shallower tree = fewer PCS node updates per path. tradeoff: wider nodes = more expensive per-node commitment
3. **cross-index dimension encoding**: how to encode 9 indexes as dimensions of one polynomial? separate polynomial per index (simple, 9 commitments) vs one polynomial with index selector (complex, 1 commitment, LogUp-free)
4. **deduplication clustering**: empirical analysis needed — how clustered are cyberlink updates in practice? power-law suggests high clustering for top particles. the deduplication savings depend on this distribution

see [[unified-polynomial-state]] for the endgame (all 13 sub-roots → 1 polynomial), [[cross-index]] for current LogUp protocol, [[indexes]] for NMT structure
