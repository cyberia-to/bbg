---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-18
---
# temporal polynomial

replace discrete time.root NMT (7 namespaces: steps, seconds, hours, days, weeks, moons, years) with a continuous polynomial BBG_poly(x, t). state at any time t is one polynomial opening. historical queries become evaluation proofs.

## current cost

time.root is an NMT with 7 temporal namespaces. querying "state at time t" requires:

```
1. determine which namespace contains t          O(1)
2. walk NMT for that namespace                   O(log n) hemera hashes
3. open the BBG_root snapshot at that position    O(log n) hemera hashes
4. query specific data within that snapshot       O(log n) hemera hashes

total: ~3 × O(log n) hemera permutations per historical query
at n = 2³²: ~96 hemera permutations = ~70K constraints
```

cross-namespace queries (e.g., "all state changes between hour H and day D") require walking multiple NMTs and merging results.

## the construction

model BBG state as a bivariate polynomial:

```
BBG_poly(x, t) where:
  x encodes the state key (particle, neuron, axon, etc.)
  t encodes the time dimension

BBG_poly(x_particle_P, t_42) = state of particle P at time 42
```

commit via zheng PCS (WHIR or Brakedown):

```
commitment: C = PCS.commit(BBG_poly)    one commitment covers all time
opening:    PCS.open(C, (x, t)) → v     one evaluation proof
verify:     PCS.verify(C, (x, t), v)    10-50 μs (zheng-2)
```

## historical queries

```
current (NMT):
  "what was π of particle P at block 1000?"
  → walk time.root NMT → find snapshot → walk particles.root NMT → open leaf
  → ~96 hemera permutations

temporal polynomial:
  "what was π of particle P at block 1000?"
  → PCS.open(C, (x_P, t_1000))
  → one evaluation proof, 10-50 μs verification
```

## range queries

```
"all focus changes for neuron N between t₁ and t₂"

current: walk time NMT for each namespace in range, collect snapshots, diff
temporal poly: evaluate BBG_poly(x_N, t) at t₁ and t₂, prove the difference
  or: open a univariate slice BBG_poly(x_N, ·) and prove it on the range
```

## incremental updates

at each block, the polynomial extends by one time step:

```
BBG_poly(x, t_new) = BBG_poly(x, t_old) + Δ(x)

where Δ(x) encodes all state changes in the new block
```

with updateable PCS (Brakedown or structured WHIR):
- update cost: O(|changes|) field ops per block
- no full recomputation needed

without updateable PCS:
- maintain running polynomial, recompute commitment periodically
- amortize over epoch boundaries

## interaction with gravity commitment

gravity commitment ([[gravity-commitment]]) assigns layers by access frequency. temporal polynomial composes naturally:

```
hot layer:   recent time + high-π keys    → lowest-degree terms, cheapest proof
warm layer:  recent time + low-π keys     → medium-degree terms
cold layer:  old time + any keys          → highest-degree terms, most expensive

proof cost for "current balance of top neuron": ~1 KiB, ~10 μs
proof cost for "π of obscure particle 3 years ago": ~8 KiB, ~200 μs
```

the temporal dimension and the importance dimension both compress into polynomial degree. recent + important = cheap. old + obscure = expensive. natural.

## time.root elimination

```
current 13 sub-roots:
  particles.root, axons_out.root, ..., time.root, ..., signals.root

with temporal polynomial:
  BBG_poly commitment replaces time.root entirely
  BBG_poly also subsumes the temporal dimension of ALL other public indexes
  any historical query to any index = one polynomial opening

BBG_root = H(BBG_poly_commitment ‖ cyberlinks.root ‖ spent.root ‖ balance.root ‖ signals.root)
```

time.root was 1 of 13 sub-roots. temporal polynomial absorbs its function and adds continuous-time queries to all public indexes.

## open questions

1. **polynomial degree growth**: each block adds one time step. after 10^6 blocks, the polynomial has degree 10^6 in t. PCS commitment and opening cost grow with degree. periodic re-basing (new polynomial starting from latest checkpoint) may be needed
2. **sparse representation**: most state keys don't change every block. the polynomial is extremely sparse in the (x, t) plane. sparse polynomial commitment schemes may reduce cost dramatically
3. **interaction with algebraic NMT**: if algebraic NMT ([[algebraic-nmt]]) replaces spatial NMTs with polynomials, temporal polynomial unifies the spatial and temporal dimensions into one object. BBG_poly(x, index, t) = one polynomial for everything
4. **pruning and forgetting**: temporal decay removes old state. the polynomial must support efficient "forgetting" of old evaluations. re-basing at epoch boundaries may serve this purpose

see [[algebraic-nmt]] for spatial polynomial, [[gravity-commitment]] for access-weighted encoding
