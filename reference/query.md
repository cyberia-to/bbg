---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# query

verifiable queries over [[BBG]] polynomial state. any query against BBG_poly produces a cryptographic proof that the result is correct and complete.

## the mechanism

BBG_poly(index, key, t) is committed via [[Brakedown]] PCS. a query IS a polynomial evaluation. the proof IS a PCS opening. verification IS O(1) field operations.

```
query:    Q applied to BBG_poly
result:   R = Q(BBG_poly)
proof:    PCS opening proving R = Q(BBG_poly) against BBG_root
verify:   Brakedown.verify(BBG_root, query_point, R, proof) → accept/reject
cost:     10-50 μs, independent of query complexity or graph size
```

## simple queries (single opening)

```
"energy of particle P"
= BBG_poly(particles, P, t_now)
= one PCS opening, ~200 bytes proof

"all outgoing axons from P"
= BBG_poly(axons_out, P, t_now)
= one PCS batch opening, ~200 bytes proof

"neuron N's focus, karma, stake"
= BBG_poly(neurons, N, t_now)
= one PCS opening, ~200 bytes proof

"state at time T"
= BBG_poly(index, key, T)
= one PCS opening at historical time dimension
```

every simple query: ~200 bytes proof, O(√N) field operations to verify.

## complex queries (compiled)

queries beyond single-point evaluation are compiled from relational algebra into CCS constraints, then proved via [[zheng]]:

```
CozoDB query
    ↓
relational algebra (logical plan)
    ↓
circuit plan (arithmetic operations over BBG_poly)
    ↓
CCS instance (zheng constraint system)
    ↓
proof via SuperSpartan + sumcheck
    ↓
verifiable result + proof
```

### relational operations → constraints

| CozoDB operation | circuit encoding | constraints |
|---|---|---|
| select (filter) | range check | ~64 per comparison |
| project | polynomial evaluation at subset | ~100 per field |
| join | LogUp lookup argument | ~500 per lookup |
| sort | permutation argument | ~N per element |
| aggregate (sum, max) | running accumulator | ~10 per element |
| limit (top-k) | comparison chain + truncation | ~64 per comparison |

### examples

**top 100 particles by π:**

```
SELECT particle_id, pi FROM particles ORDER BY pi DESC LIMIT 100

compilation:
  1. batch opening of particles polynomial               (PCS opening)
  2. permutation argument proving sort                    (~N constraints)
  3. truncation proof: output[100].pi ≤ output[99].pi    (1 comparison)
  4. completeness: no particle with π > output[100].pi   (range check)

proof: ~5 KiB, verify: 10-50 μs
```

**multi-hop path (A to B, max 3 hops):**

```
compilation:
  1. open axons_out for A → neighbors N₁                 (PCS opening)
  2. open axons_out for each n ∈ N₁ → N₂                 (batch PCS opening)
  3. open axons_out for each n ∈ N₂ → N₃                 (batch PCS opening)
  4. check B ∈ N₁ ∪ N₂ ∪ N₃                              (membership)

each hop folds into accumulator via HyperNova (~30 field ops).
proof: ~3 KiB, verify: 10-50 μs regardless of path length
```

**temporal range (all cyberlinks from neuron N between t₁ and t₂):**

```
compilation:
  1. open BBG_poly(neurons, N, t₁) and BBG_poly(neurons, N, t₂)    (2 temporal openings)
  2. diff: Δ = state(t₂) - state(t₁)                               (field subtraction)
  3. prove Δ decomposes into individual cyberlinks                   (batch opening)

proof: ~3 KiB, verify: 10-50 μs
```

## cost model

```
query type              constraints    proof size    verification
────────────────────    ───────────    ──────────    ────────────
single point opening    ~100           ~200 bytes    O(√N) field ops
namespace range         ~100 × range   ~1-3 KiB     10-50 μs
top-k                   ~N + 64k       ~5 KiB       10-50 μs
multi-hop (d hops)      ~100 × d       ~3 KiB       10-50 μs
join (2 indexes)        ~500           ~2 KiB       10-50 μs
temporal range          ~200           ~3 KiB       10-50 μs
arbitrary CozoDB        varies         ~5-10 KiB    10-50 μs
```

verification is ALWAYS 10-50 μs (one zheng decider). proof size is always < 10 KiB. query complexity affects PROVER cost, not verifier cost.

## query cost optimization

query cost optimization via polynomial layering is a roadmap item (see roadmap/).

see [[architecture]] for BBG_poly structure, [[state]] for evaluation dimensions, [[sync]] for namespace query protocol, [[data-availability]] for algebraic DAS queries
