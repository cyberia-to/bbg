---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# query

verifiable queries over [[BBG]] polynomial state. any query against BBG_poly produces a cryptographic proof that the result is correct and complete.

## the mechanism

BBG_poly(index, key, t) is committed via [[Brakedown]] Lens. a query IS a polynomial evaluation. the proof IS a Lens opening. verification IS O(1) field operations.

```
query:    Q applied to BBG_poly
result:   R = Q(BBG_poly)
proof:    Lens opening proving R = Q(BBG_poly) against BBG_root
verify:   Brakedown.verify(BBG_root, query_point, R, proof) → accept/reject
cost:     independent of query complexity or graph size
```

## simple queries (single opening)

```
"energy of particle P"
= BBG_poly(particles, P, t_now)
= one Lens opening

"all outgoing axons from P"
= BBG_poly(axons_out, P, t_now)
= one Lens batch opening

"neuron N's focus, karma, stake"
= BBG_poly(neurons, N, t_now)
= one Lens opening

"state at time T"
= BBG_poly(index, key, T)
= one Lens opening at historical time dimension
```

every simple query: one opening. proof sizes and verify times of the retired zheng design do not hold; the measured numbers live in [[soft3/proposals/proof-system-repair|the proof-system repair]].

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

**top 100 particles by φ*:**

```
SELECT particle_id, pi FROM particles ORDER BY pi DESC LIMIT 100

compilation:
  1. batch opening of particles polynomial               (Lens opening)
  2. permutation argument proving sort                    (~N constraints)
  3. truncation proof: output[100].pi ≤ output[99].pi    (1 comparison)
  4. completeness: no particle with φ* > output[100].pi   (range check)

proof and verify: one zheng proof
```

**multi-hop path (A to B, max 3 hops):**

```
compilation:
  1. open axons_out for A → neighbors N₁                 (Lens opening)
  2. open axons_out for each n ∈ N₁ → N₂                 (batch Lens opening)
  3. open axons_out for each n ∈ N₂ → N₃                 (batch Lens opening)
  4. check B ∈ N₁ ∪ N₂ ∪ N₃                              (membership)

each hop folds into the accumulator.
proof and verify: one zheng proof regardless of path length
```

**temporal range (all cyberlinks from neuron N between t₁ and t₂):**

```
compilation:
  1. open BBG_poly(neurons, N, t₁) and BBG_poly(neurons, N, t₂)    (2 temporal openings)
  2. diff: Δ = state(t₂) - state(t₁)                               (field subtraction)
  3. prove Δ decomposes into individual cyberlinks                   (batch opening)

proof and verify: one zheng proof
```

## cost model

```
query type              constraints    proof size    verification
────────────────────    ───────────    ──────────    ────────────
single point opening    ~100           —             —
namespace range         ~100 × range   —             —
top-k                   ~N + 64k       —             —
multi-hop (d hops)      ~100 × d       —             —
join (2 indexes)        ~500           —             —
temporal range          ~200           —             —
arbitrary CozoDB        varies         —             —
```

verification is always one zheng decider. query complexity affects prover cost, and leaves verifier cost unchanged. proof size and verify time are measured in [[soft3/proposals/proof-system-repair|the proof-system repair]].

## query cost optimization

query cost optimization via polynomial layering is a roadmap item (see roadmap/).

see [[architecture]] for BBG_poly structure, [[state]] for evaluation dimensions, [[foculus]] for namespace query protocol, [[data-availability]] for algebraic DAS queries
