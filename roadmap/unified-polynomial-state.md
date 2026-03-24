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
# unified polynomial state

replace all 13 BBG sub-roots with one multivariate polynomial commitment. BBG_root = single PCS commitment. every query = one polynomial opening. cross-index consistency is automatic. LogUp becomes unnecessary.

## current architecture

```
BBG_root = H(
  particles.root ‖ axons_out.root ‖ axons_in.root ‖
  neurons.root ‖ locations.root ‖ coins.root ‖
  cards.root ‖ files.root ‖ time.root ‖
  cyberlinks.root ‖ spent.root ‖ balance.root ‖
  signals.root
)

13 sub-roots × 32 bytes = 416 bytes input → 1 hemera hash → 32-byte BBG_root
9 independent NMT trees (public)
3 independent hash commitments (private)
1 MMR (signals)
```

each sub-root is maintained independently. cross-index consistency requires LogUp arguments (~500 constraints per lookup). a single cyberlink update touches 4-5 independent trees.

## the endgame

```
BBG_poly(index, key, t) = multivariate polynomial

dimensions:
  index ∈ {particles, axons_out, axons_in, neurons, locations,
           coins, cards, files, commitments, nullifiers}
  key   = namespace key within that index (particle CID, neuron ID, etc.)
  t     = time (block height)

BBG_root = PCS.commit(BBG_poly)    one 32-byte commitment
```

every query is a polynomial evaluation:

```
"energy of particle P at time t"     = BBG_poly(particles, P, t)
"outgoing axons of particle P"       = BBG_poly(axons_out, P, t)
"focus of neuron N"                  = BBG_poly(neurons, N, t)
"nullifier status of n"              = BBG_poly(nullifiers, n, t)
```

one PCS opening per query. one verification per proof. 10-50 μs.

## why LogUp disappears

currently LogUp proves: "the axon in axons_out.root is the same axon in axons_in.root is the same axon-particle in particles.root." three independent data structures must agree.

with unified polynomial: axons_out, axons_in, and particles are different evaluation dimensions of the SAME polynomial. consistency is structural — they cannot disagree because they are the same object evaluated at different points.

```
current:
  axons_out.root ←LogUp→ axons_in.root ←LogUp→ particles.root
  cost: ~500 constraints per lookup × 3 lookups per cyberlink = ~1500

unified:
  BBG_poly(axons_out, P, t) and BBG_poly(axons_in, P, t) and BBG_poly(particles, P, t)
  are evaluations of the same committed polynomial
  consistency: free (same commitment)
  cost: 0 additional constraints
```

## per-cyberlink savings

```
current (13 sub-roots + NMTs + LogUp):
  4-5 NMT path updates:  ~94K constraints (hemera)
  LogUp consistency:      ~1.5K constraints
  total:                  ~95.5K constraints

algebraic NMT (intermediate):
  1 polynomial update:    ~3K constraints
  LogUp consistency:      ~1.5K constraints
  total:                  ~4.5K constraints

unified polynomial:
  1 polynomial update:    ~3K constraints
  LogUp:                  0 (structural consistency)
  total:                  ~3K constraints

improvement over current: ~32×
improvement over algebraic NMT: ~1.5× (LogUp elimination)
```

the big win is algebraic NMT (30×). unified polynomial adds LogUp elimination and architectural simplification.

## privacy boundary

private state (commitments, nullifiers, balances) is a separate evaluation dimension of the same polynomial. privacy is maintained by the PCS:

```
public query:   BBG_poly(particles, P, t) → energy    (anyone can open)
private query:  BBG_poly(nullifiers, n, t) → status   (ZK opening, reveals nothing else)
```

the polynomial structure does not leak cross-dimensional information. opening BBG_poly at (particles, P, t) reveals nothing about (nullifiers, n, t) — standard PCS zero-knowledge property.

## signals.root

signals are append-only (MMR). the MMR structure serves a different purpose than the NMT indexes — it provides append-only ordering, not namespace completeness. signals.root may remain as a separate MMR commitment or be absorbed into the polynomial's time dimension.

```
option A: signals.root stays as MMR
  BBG_root = H(BBG_poly_commitment ‖ signals.root)
  2 sub-roots instead of 13

option B: signals absorbed into BBG_poly
  signals as evaluation dimension: BBG_poly(signals, step, t)
  BBG_root = BBG_poly_commitment
  1 commitment. period.
```

## migration path

```
phase 1: algebraic NMT — 9 public NMTs → 1 polynomial    (current proposal)
phase 2: mutator set polynomial — AOCL + SWBF → polynomials
phase 3: unified polynomial — merge public + private + temporal into one
phase 4: signals absorption — merge signals MMR into polynomial (optional)

each phase is independently valuable. each reduces sub-root count:
  phase 1: 13 → 5 (9 NMTs → 1 poly + 3 private + 1 signals)
  phase 2: 5 → 3 (private → 1 poly + public poly + signals)
  phase 3: 3 → 2 (merge public + private → 1 poly + signals)
  phase 4: 2 → 1 (merge signals → 1 poly)
```

## the ultimate BBG_root

```
BBG_root = PCS.commit(BBG_poly)

32 bytes. one polynomial. all state. all time. all indexes.
every query: one opening. every verification: 10-50 μs.
cross-index consistency: structural. LogUp: eliminated.
privacy: ZK dimension isolation. completeness: algebraic.
```

## open questions

1. **polynomial dimension count**: BBG_poly has 3+ dimensions (index, key, time). multivariate PCS over 3+ dimensions — what is the concrete cost? multilinear extensions handle this naturally but commitment size grows with dimension count
2. **dimension-isolated ZK**: opening one dimension must not leak information about another. standard multilinear PCS provides this, but the security proof for dimension isolation in the unified setting needs verification
3. **update cost**: a cyberlink updates multiple dimensions simultaneously (axons_out, axons_in, particles, neurons). in the unified polynomial, this is one update to BBG_poly at multiple evaluation points. batch update cost?
4. **commitment size**: a single polynomial over all state may have very high degree. at 10^9 particles × 10^6 blocks × 10 indexes, the polynomial has ~10^16 evaluation points. sparse representation essential

see [[algebraic-nmt]] for phase 1, [[mutator-set-polynomial]] for phase 2, [[temporal-polynomial]] for time dimension