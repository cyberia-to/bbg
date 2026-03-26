---
tags: cyber
crystal-type: entity
crystal-domain: cyber
---
# why polynomial state

## the argument

a knowledge graph with N particles, M axons, and K neurons needs multiple views of the same data: by content (particles), by source (axons_out), by target (axons_in), by agent (neurons). the question is how to commit to these views so that any query produces a short, verifiable proof.

hash trees (NMTs, Merkle trees) commit each view as an independent tree. each tree hashes leaves up to a root. querying any view produces a logarithmic authentication path. this works, but the views are independent objects. proving that axons_out and axons_in agree about the same axon requires a separate consistency argument (LogUp). each tree stores O(n) internal nodes. updating one record touches O(log n) hashes in every affected tree.

a polynomial commits all views as evaluation dimensions of one object:

```
BBG_poly(dimension, key, t)

dimension ∈ {particles, axons_out, axons_in, neurons, ...}
key       = namespace identifier (CID, neuron_id)
t         = block height
```

querying any view is a polynomial opening: evaluate BBG_poly at (dimension, key, t), produce a Lens proof. verification: O(1) field operations, ~200 bytes.

## cross-index consistency is structural

with separate trees, axons_out.root and axons_in.root are independent commitments. an axon appears in both. proving it is the same axon requires LogUp — a lookup argument that costs ~500 constraints per axon, ~6M constraints per block.

with one polynomial, axons_out and axons_in are evaluation dimensions of the same committed object. they cannot disagree. the polynomial is one function — evaluating it at (axons_out, P, t) and (axons_in, P, t) returns values bound by the same commitment. consistency is definitional, not proved. LogUp cost: zero.

this is stronger than a consistency proof. a LogUp argument proves agreement under computational assumptions. a single polynomial commitment makes disagreement algebraically impossible within the Lens binding guarantee.

## 33x fewer constraints per cyberlink

a cyberlink (neuron links particle P to particle Q) touches 4-5 indexes: particles (energy update for P and Q), axons_out (insert under P), axons_in (insert under Q), neurons (focus deduction).

with hash trees at 4 billion entries per index:

```
tree depth:                32
hemera hash per level:     736 constraints
per path:                  32 × 736 = 23,552 constraints
per cyberlink:             4.5 paths × 23,552 = ~106,000 constraints
LogUp cross-index:         ~1,500 constraints
total:                     ~107,500 constraints
```

with polynomial state:

```
polynomial update:         ~100 field ops per level
per path (Verkle):         32 × 100 = ~3,200 constraints
LogUp:                     0 (structural consistency)
total:                     ~3,200 constraints
```

ratio: 107,500 / 3,200 = 33x.

at 1000 cyberlinks per block: ~108M constraints become ~7.5M. the difference is the cost of maintaining independent hash trees versus updating evaluation points in one polynomial.

## 5 TB of internal nodes vanish

each NMT index with n = 2^32 entries has 2n - 1 internal nodes, each 64 bytes (two 32-byte hashes). nine indexes:

```
internal nodes per index:  (2 × 2^32 - 1) × 64 bytes ≈ 550 GB
nine indexes:              ~5 TB
```

the polynomial has no internal nodes. leaf data is the same in both approaches — the polynomial replaces the tree structure, not the data. commitment state: 32 bytes per Lens commitment.

## the evaluation point model

BBG_poly(dimension, key, t) encodes the entire public state as a function over three axes:

- **dimension** (WHAT): which view — particles, axons_out, axons_in, neurons, locations, coins, cards, files, time, signals
- **key** (WHERE): namespace identifier — particle CID, neuron ID, denomination hash, card ID
- **t** (WHEN): block height — temporal axis for historical queries

every query is an evaluation:

```
"energy of particle P at height h"       = BBG_poly(particles, P, h)
"outgoing axons from particle P"         = BBG_poly(axons_out, P, h)
"focus of neuron N"                      = BBG_poly(neurons, N, h)
"supply of denomination D"               = BBG_poly(coins, D, h)
```

one Lens opening per query. one verification. 10-50 microseconds. ~200 bytes proof.

the temporal dimension means historical state is queryable without separate snapshots. BBG_poly at height h-1000 returns the state 1000 blocks ago — same mechanism, same proof size, same verification cost.

## the completeness guarantee

NMTs provide structural completeness: the sorting invariant at internal nodes prevents omission. if a leaf with namespace N exists, the tree walk must encounter it.

polynomial completeness relies on Lens binding: the commitment uniquely determines all evaluation points. a Lens opening for "all evaluation points in dimension D with key prefix P" cannot omit a point that the polynomial encodes. polynomial binding makes omission algebraically impossible.

the trust root is the same in both cases: hemera. NMTs hash with hemera at tree nodes. polynomial commitments (WHIR, Brakedown) are hash-based Lens schemes built on hemera's collision resistance. both approaches ultimately trust hemera. the polynomial path expresses the completeness guarantee with fewer hash calls.

Lens binding is computational (breaks if the Lens breaks). NMT completeness is structural (breaks only if the tree is corrupted). the polynomial approach trades unconditional structural guarantee for computational algebraic guarantee — with the same underlying hash function, post-quantum security (hash-based Lens), and Schwartz-Zippel error probability of 2^-96 over the extension field F_{p^2}.

## BBG_root

```
BBG_root = Lens.commit(BBG_poly)    32 bytes

one polynomial. all public dimensions. all time.
every query: one opening.
every verification: ~5 μs.
cross-index consistency: structural.
completeness: algebraic (Lens binding).
```

private state (commitment polynomial A(x), nullifier polynomial N(x)) remains separate — see [[polynomial-privacy]]. the full authenticated root:

```
BBG_root = H(commit(BBG_poly) ‖ commit(A) ‖ commit(N))    32 bytes
```

## nouns ARE polynomials

the polynomial model extends beyond aggregate state (BBG_poly) to individual particles. a particle's content is a polynomial over the Goldilocks field. its identity is hemera(Lens.commit(content) ‖ PARTICLE). the tree/polynomial isomorphism means the SAME Lens serves both levels:

- **state queries**: BBG_poly(particles, CID, t) → energy, pi-star (aggregate)
- **content queries**: Lens.open(particle_poly, position) → bytes at offset (data)

same Lens. same proof format. same verification cost (~5 microseconds, ~200 bytes).

this unification has three consequences:

1. **no separate content addressing layer.** particle identity is a Lens commitment wrapped by hemera. there is no hash tree for content and a separate polynomial for state. one algebraic framework covers both.

2. **native DAS.** a polynomial noun is automatically erasure-coded — extending the polynomial beyond its evaluation domain produces parity. no encoding pipeline. the particle IS the erasure code.

3. **algebraic composability end-to-end.** particle identity is a field element derived from polynomial commitment. state operations (energy, focus, axon weights) operate on field elements. there is no hash-to-field boundary anywhere in the pipeline. content, identity, and state are all polynomial objects in the same field.

the evaluation point model from BBG_poly (dimension, key, t) and the content model from particle polynomials (CID, byte_offset) are two applications of the same mathematical structure: polynomial commitment with Lens openings.

see [[architecture]] for the full specification, [[architecture-overview]] for the pipeline, [[polynomial-privacy]] for private state
