---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0016211605879495649
heat: 0.0011493503835617823
focus: 0.0007698300774402123
gravity: 0
density: 0.89
---
# cross-index

consistency between the three public evaluation dimensions that index [[axons]]: particles, axons_out, and axons_in. with the unified polynomial architecture, cross-index consistency is structural — a property of the commitment, not a separate proof obligation.

## why LogUp is eliminated

LogUp was required when particles.root, axons_out.root, and axons_in.root were three independent NMT trees. three separate data structures could disagree. LogUp lookup arguments proved they contained the same set of axon-particles and that aggregate weight deltas matched across all three.

with BBG_poly, all three indexes are evaluation dimensions of the SAME polynomial:

```
BBG_poly(particles, H(from, to), t)    axon data
BBG_poly(axons_out, from, t)           outgoing index
BBG_poly(axons_in, to, t)              incoming index
```

these are evaluations of one committed object. consistency is definitional — they cannot disagree because they are the same polynomial evaluated at different points. the Lens commitment binds all dimensions simultaneously.

```
previous architecture:
  axons_out.root ←LogUp→ axons_in.root ←LogUp→ particles.root
  cost: ~500 constraints per lookup × 3 lookups per cyberlink = ~1,500
  per block (K=4000 axons): ~6M constraints

current architecture:
  BBG_poly(axons_out, P, t) and BBG_poly(axons_in, P, t) and BBG_poly(particles, P, t)
  are evaluations of the same committed polynomial
  consistency: free (same commitment)
  cost: 0 additional constraints
```

## aggregate weight consistency

when a [[cyberlink]] updates an axon's aggregate weight, the update modifies BBG_poly at multiple evaluation points in one operation:

```
transaction: new cyberlink adds weight δ to axon H(p, q)

polynomial update:
  BBG_poly(particles, H(p,q), t):  weight += δ
  BBG_poly(axons_out, p, t):       pointer updated
  BBG_poly(axons_in, q, t):        pointer updated

all three are updates to the same polynomial.
the Lens recommitment covers all changes in one operation.
no separate consistency proof needed.
```

## batch updates

a block containing B transactions updating K distinct axons:

```
all K axon updates → polynomial update at affected evaluation points

prover: O(K) field operations for polynomial updates + O(1) Lens recommit
verifier: O(1) — one Lens verification per batch opening

constraints for cross-index consistency of entire block: 0
(was ~500 × K + O(log K) with LogUp)

for a block updating 10,000 axons:
  cross-index cost: 0 (was ~5,000,000 constraints)
```

## decay batch consistency

at epoch boundaries, batch decay recomputes axon weights. all evaluation dimensions update simultaneously as part of the same polynomial modification:

```
for each axon with old weight A and new weight A' = A × α^Δt:
  BBG_poly(particles, axon, t_new) = updated weight
  BBG_poly(axons_out, source, t_new) = updated pointer
  BBG_poly(axons_in, target, t_new) = updated pointer

one polynomial recommitment covers all decayed axons in the epoch.
consistency is structural — no separate proof.
```

## architectural improvement

the elimination of LogUp is a qualitative change, not just an optimization:

| property | separate NMTs + LogUp | unified polynomial |
|----------|----------------------|-------------------|
| consistency model | proved per transaction | structural (free) |
| cost per cyberlink | ~1,500 constraints | 0 |
| cost per block (K=4000) | ~6M constraints | 0 |
| failure mode | LogUp proof error → inconsistency | impossible (same object) |
| complexity | three trees + sumcheck protocol | one polynomial |

the guarantee is stronger: with LogUp, consistency depends on the prover correctly constructing the sumcheck polynomial. with unified polynomial, consistency depends only on Lens binding — the same assumption that authenticates ALL state. no additional mechanism, no additional trust.

see [[architecture]] for evaluation dimensions, [[temporal]] for decay protocol, [[storage]] for the storage model
