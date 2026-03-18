---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
---
# algebraic NMT — shared sub-tree deduplication

replace independent NMT recomputation across bbg's 9 public indexes with a unified polynomial representation. a single cyberlink currently triggers 4-5 independent O(log n) hemera path updates. algebraic NMT reduces this to one polynomial update.

## current cost

a cyberlink touches:

```
1. particles.root    (energy update)             O(log n) hemera calls
2. axons_out.root    (source index update)       O(log n) hemera calls
3. axons_in.root     (target index update)       O(log n) hemera calls
4. neurons.root      (focus deduction)           O(log n) hemera calls
5. LogUp cross-index (consistency check)         O(k log k) constraints

total per cyberlink: ~4 × O(log n) hemera hashes + LogUp
at n = 2³²: ~4 × 32 = 128 hemera permutations = ~94,000 constraints (hemera-2)
```

the same particle appears in particles.root AND as a namespace in axons_out/axons_in. the same neuron appears in neurons.root AND as a namespace in particles.root. indexes share structure but recompute independently.

## the idea

represent all 9 public NMT indexes as evaluations of a single multivariate polynomial committed via zheng PCS (WHIR or Brakedown):

```
current:
  BBG_root = H(particles.root ‖ axons_out.root ‖ ... ‖ time.root)
  9 independent NMT trees, 9 independent root hashes
  update one → rehash one tree → recompute BBG_root

algebraic:
  BBG_poly(x, index) = multivariate polynomial
  x encodes the namespace key
  index ∈ {particles, axons_out, axons_in, neurons, ...}
  BBG_commitment = PCS.commit(BBG_poly)

  update one leaf → update polynomial evaluation at one point
  → O(1) or O(log n) field ops depending on PCS
  → NO tree rehashing
```

## completeness guarantee

NMTs provide structural completeness: the sorting invariant (min/max namespace labels at internal nodes) prevents omission. if a leaf exists in the range, the tree cannot hide it.

algebraic NMT must replicate this guarantee. two approaches:

### LogUp range check

prove that the committed set of (namespace, value) pairs is complete for any queried range:

```
for range [a, b]:
  LogUp proves: Σ 1/(β - f_i) for all i where a ≤ namespace_i ≤ b
  equals:       Σ 1/(β - t_j) for the expected complete set

  missing entry → sums don't match → proof fails
  cost: O(k) constraints where k = number of entries in range
```

this is algebraic completeness — the polynomial structure prevents omission, analogous to the NMT sorting invariant.

### sorted polynomial evaluation

commit to (namespace, value) pairs sorted by namespace. a range query [a, b] opens the polynomial at consecutive positions. the verifier checks:
1. first position ≥ a (lower bound)
2. last position ≤ b (upper bound)
3. positions are consecutive (no gaps)
4. values match claimed data

cost: O(range_size) field ops + one PCS opening. no hemera tree traversal.

## per-cyberlink savings

```
current:     4 NMT path updates = ~128 hemera permutations = ~94,000 constraints
algebraic:   1 polynomial update = O(log n) field ops = ~3,000 constraints (estimated)

savings:     ~30× fewer constraints per cyberlink
```

the savings compound with block size. a block with 1000 cyberlinks:

```
current:     1000 × 128 = 128,000 hemera permutations ≈ 94M constraints
algebraic:   1000 × O(log n) field ops + 1 batch PCS update ≈ 3M constraints

savings:     ~30× per block
```

## BBG_root unification

currently 13 sub-roots concatenated and hashed:

```
BBG_root = H(13 × 32-byte sub-roots) = H(416 bytes)
```

with algebraic NMT, the BBG commitment is a single PCS commitment:

```
BBG_commitment = PCS.commit(BBG_poly)
BBG_poly encodes ALL 9 public indexes as one multivariate polynomial
private state (AOCL, SWBF, balance) remains hash-committed (hemera)
signals.root remains hash-committed (MMR)

BBG_root = H(BBG_commitment ‖ cyberlinks.root ‖ spent.root ‖ balance.root ‖ signals.root)
         = H(32 + 32 + 32 + 32 + 32) = H(160 bytes)
```

public state: one polynomial commitment (algebraic completeness).
private state: hash commitments (hemera structural security).
hybrid: best of both worlds.

## migration path

1. **phase 1**: keep NMTs, add polynomial commitment as redundant index. verify both agree. zero risk — NMTs remain the authority
2. **phase 2**: switch authority to polynomial commitment for public indexes. NMTs become secondary verification
3. **phase 3**: remove NMTs for public indexes. algebraic completeness is the sole mechanism. private state stays hash-committed

## near-term: batched deduplication

before full algebraic NMT, a simpler optimization captures most intra-block savings. track which leaves changed during a block and recompute hemera paths only for unique changed positions at block end:

```
scenario: 1000 cyberlinks, 200 unique particles affected
  current: 1000 × 4 path updates = 4000 × O(log n) hashes
  batched: 200 × 4 path updates = 800 × O(log n) hashes

  savings: ~5× (from deduplication alone)
```

for high-traffic particles updated many times within one block:

```
scenario: hot particle updated 100 times in one block
  current: 100 path updates = 100 × O(log n) hashes
  batched: 1 path update at block end = 1 × O(log n) hashes

  savings: 100× for that leaf
```

implementation: dirty-set tracking per block. accumulate leaf changes, deduplicate, recompute paths once at block boundary. hemera remains the sole commitment mechanism — no additional hash needed.

this is a stepping stone: implement batched deduplication now while NMTs remain primary, transition to fully algebraic NMT later where polynomial commitments make updates naturally O(1).

## open questions

1. **NMT structural security vs algebraic completeness**: NMT's sorting invariant is structural — the tree cannot lie by construction (law 3). algebraic completeness depends on the PCS soundness — a protocol guarantee, not a structural one. does this violate law 3?
2. **multivariate polynomial degree**: 9 indexes × 2³² entries each → total degree. the polynomial must be manageable for PCS commitment. sparse representation may be required
3. **cross-index consistency**: currently LogUp proves axons_out ↔ axons_in ↔ particles.root consistency. with algebraic NMT, cross-index consistency may become automatic (same polynomial, different evaluation dimensions)
4. **incremental updates**: WHIR commitment is not cheaply updatable (full recomputation). Brakedown may support incremental updates more naturally. Verkle-tree approach (polynomial commitment at each node) is a middle ground
5. **deduplication clustering**: empirical analysis of cyberlink patterns needed — how clustered are updates within a block? power-law access patterns suggest high clustering for top particles

see [[cross-index]] for LogUp consistency, [[indexes]] for NMT structure, [[design-principles]] for the three laws
