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
# verifiable query compilation

compile arbitrary CozoDB queries into polynomial opening proofs. NMTs prove completeness for namespace queries. complex queries (range, top-k, multi-hop, temporal) need a general mechanism.

## the gap

bbg provides three proof types:

```
1. NMT completeness: "all items in namespace X"         — structural
2. polynomial opening: "value at position x is v"       — algebraic
3. LogUp consistency: "indexes agree on shared data"     — cross-index
```

queries users actually want:

```
"top 100 particles by π"                      → not a single namespace
"all cyberlinks from neuron N after time T"    → cross-index + temporal
"path from particle A to particle B"           → multi-hop traversal
"neurons with focus > F and karma > K"         → range query on two dimensions
"aggregate valence for axon (P, Q)"            → aggregation over private data
```

each of these currently requires custom proof construction. a verifiable query engine automates this.

## the construction

```
query:    Q = CozoDB query expression
data:     D = BBG state (committed as polynomial or NMT)
result:   R = query result
proof:    π = zheng proof that R = Q(D)

verifier checks:
  1. D is committed in BBG_root (one PCS opening or NMT root check)
  2. π proves Q was correctly evaluated on D
  3. R matches the claimed result
```

the query compiler translates CozoDB relational algebra into arithmetic circuit constraints:

```
CozoDB operation     → circuit encoding
─────────────────────────────────────────
select (filter)      → range check constraints
project              → polynomial evaluation at subset
join                 → LogUp lookup argument
sort                 → permutation argument
aggregate (sum, max) → running accumulator constraints
limit (top-k)        → comparison chain + truncation
```

## query compilation pipeline

```
CozoDB query
    ↓
relational algebra (logical plan)
    ↓
circuit plan (arithmetic operations over committed polynomials)
    ↓
CCS instance (zheng-2 constraint system)
    ↓
proof via SuperSpartan + sumcheck
    ↓
verifiable result + proof
```

the query plan determines which BBG polynomials/NMTs to open. the circuit encodes the relational operations. the proof covers both the openings and the computation.

## example: top-k by π

```
query: SELECT particle_id, pi FROM particles ORDER BY pi DESC LIMIT 100

compilation:
  1. open particles polynomial at all evaluation points       (batch opening)
  2. prove sort: permutation argument that output is sorted   (~N constraints)
  3. prove truncation: output[100].pi ≤ output[99].pi        (1 comparison)
  4. prove completeness: no particle with π > output[100].pi  (range check)

proof: batch PCS opening + permutation argument + range check
verify: one zheng verification (10-50 μs)
result: 100 (particle_id, π) pairs with cryptographic guarantee of correctness
```

## example: multi-hop path

```
query: path from particle A to particle B, max 3 hops

compilation:
  1. open axons_out for A: get neighbors N₁              (NMT completeness)
  2. for each n ∈ N₁: open axons_out for n: get N₂       (NMT completeness)
  3. for each n ∈ N₂: open axons_out for n: get N₃       (NMT completeness)
  4. check B ∈ N₁ ∪ N₂ ∪ N₃                              (membership)

proof: 3 levels of NMT completeness proofs + membership check
verify: one zheng verification (folded)
```

with folding-first ([[folding-first]]), each hop folds into the accumulator. the verifier checks one decider regardless of path length.

## example: temporal range

```
query: all cyberlinks from neuron N between time t₁ and t₂

with temporal polynomial ([[temporal-polynomial]]):
  1. open BBG_poly(x_N, t₁) and BBG_poly(x_N, t₂)       (2 polynomial openings)
  2. diff: Δ = state(t₂) - state(t₁)                     (field subtraction)
  3. prove Δ decomposes into individual cyberlinks          (batch opening)

proof: 2 temporal openings + batch decomposition
verify: one zheng verification
```

## query cost model

```
operation          constraint cost    proof contribution
────────────────────────────────────────────────────────
polynomial open    ~3K constraints    1 PCS opening per polynomial
NMT walk           ~94K constraints   O(log n) hemera hashes
  (→ ~3K with algebraic NMT)
LogUp lookup       ~500 constraints   per-lookup
range check        ~64 constraints    per-comparison
sort (permutation) ~N constraints     one permutation argument
aggregation        ~10 constraints    per-element accumulation
```

simple queries (single opening): ~3K constraints, ~1 KiB proof.
complex queries (join + sort + filter): ~100K constraints, ~5 KiB proof.
all verify in 10-50 μs regardless of query complexity (zheng-2).

## interaction with gravity commitment

gravity commitment ([[gravity-commitment]]) makes common queries cheaper:

```
"balance of top neuron"      → hot layer opening, ~1 KiB
"top-100 by π"               → hot layer batch opening, ~2 KiB
"obscure particle 3 hops out" → cold layer, multi-hop, ~10 KiB
```

the query compiler is aware of gravity layers and routes openings to the cheapest layer.

## open questions

1. **query expressiveness**: which CozoDB operations are efficiently provable? recursive queries (transitive closure) may require unbounded circuit depth. practical bound: max depth parameter per query
2. **query privacy**: the proof reveals the query was correctly executed, but does it reveal the query itself? for private queries (e.g., "my balance"), the proof should not leak what was queried. ZK-friendly query encoding needed
3. **compilation cost**: translating CozoDB to CCS is a compile-time cost. caching compiled query plans for common patterns (top-k, neighbor lookup) amortizes this
4. **CozoDB integration**: CozoDB already has a query optimizer. extending it to output circuit plans instead of (or alongside) execution plans. the optimizer's cost model must account for proof cost, not just execution cost

see [[algebraic-nmt]] for polynomial queries on public indexes, [[temporal-polynomial]] for time-dimension queries, [[gravity-commitment]] for cost-aware routing