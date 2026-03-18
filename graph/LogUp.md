---
tags: cyber, computer science, cryptography
crystal-type: entity
crystal-domain: computer science
alias: LogUp lookup argument, LogUp lookup arguments, lookup arguments
---
# LogUp

a lookup argument protocol that proves "set {f₁, ..., f_m} is contained in table {t₁, ..., t_n}" via an algebraic identity. introduced by Haböck (2022). deployed in production by Polygon and Scroll since 2023 for cross-table consistency in ZK rollups.

## the protocol

LogUp proves containment via:

```
Σᵢ 1/(β - fᵢ) = Σⱼ mⱼ/(β - tⱼ)

evaluated at a random challenge β via sumcheck
```

prover constructs the sumcheck polynomial and commits. verifier checks the identity at random β.

## cost

```
per lookup:       ~500 stark constraints (sumcheck + challenges)
prover work:      O(k log k) where k = lookups in batch
verifier work:    O(log k)
```

LogUp batches naturally — all lookups in a block combine into one proof. cost is proportional to what is touched, not total table size.

## use in cyber

[[cyber]] applies LogUp to graph index consistency: proving that axon-particles appear consistently across particles.root, axons_out.root, and axons_in.root in the [[BBG]]. ~500 constraints per axon update vs ~7,500 for independent proofs (15× savings).

see [[cross-index]] for the full consistency specification, [[BBG]] for the graph architecture
