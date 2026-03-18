---
tags: cyber, computer science, cryptography
crystal-type: entity
crystal-domain: computer science
alias: LogUp lookup argument, LogUp lookup arguments, lookup arguments
---
# LogUp

a lookup argument protocol that proves "set {f₁, ..., f_m} is contained in table {t₁, ..., t_n}" via an algebraic identity. used in [[cyber]] for cross-index consistency in the [[BBG]] — proving that axon-particles appear consistently across particles.root, axons_out.root, and axons_in.root.

introduced by Haböck (2022). used in Polygon, Scroll, and [[cyber]].

## the problem

each [[axon]] update touches 3 NMT indexes: particles.root, axons_out.root, and axons_in.root. the [[stark]] must prove that the axon-particle in particles.root is the SAME axon referenced in axons_out and axons_in, and that weight deltas are consistent across all three trees. without LogUp, each cross-index check requires independent [[WHIR]] proofs against each NMT — ~7,500 constraints per axon update.

## the protocol

LogUp proves containment via the algebraic identity:

```
Σᵢ 1/(β - fᵢ) = Σⱼ mⱼ/(β - tⱼ)

evaluated at a random challenge β via sumcheck
```

for [[cyber]]:

```
CROSS-INDEX CONSISTENCY via LogUp
─────────────────────────────────

given axon-particle a = H(from, to) with weight A_{pq}

lookup statement:
  "a exists in particles.root as an axon-particle leaf
   AND a appears in axons_out.root[from]
   AND a appears in axons_in.root[to]"

LogUp proof:
  1. f = {a, a, a}  (same axon hash, looked up in 3 trees)
  2. t₁ = particles.root leaf evaluations (axon-particles subset)
  3. t₂ = axons_out.root leaf evaluations
  4. t₃ = axons_in.root leaf evaluations

  prover constructs the sumcheck polynomial and commits
  verifier checks the identity at random β
```

## cost advantage

```
per axon update:
  LogUp:              ~500 stark constraints (sumcheck + challenges)
  independent WHIR:    3 × 2,500 = 7,500 stark constraints
  savings:            15×

block updating 10,000 axons:
  LogUp:              ~5,000,000 constraints
  independent WHIR:    ~75,000,000 constraints

prover work:  O(k log k) where k = axon updates in block
verifier work: O(log k) — dominated by sumcheck verification
```

the key property: prover cost is proportional to what it touches, not to the total graph size. this is bounded locality — the first design law of [[BBG]]. cost is proportional to the number of axons touched, not to the total number of [[cyberlinks]] in the block (many cyberlinks may aggregate into the same axon).

## batch verification

LogUp batches naturally. a block containing B transactions updating K distinct axons:

```
all K axon updates → one LogUp proof for all 3K lookups

prover:  O(K log K) — linear in number of affected axons
verifier: O(log K) — logarithmic in number of affected axons
```

this is where LogUp becomes essential: at planetary scale (10¹⁵ nodes), verifying cross-index consistency axon-by-axon is physically impossible. LogUp makes it proportional to the block, not the graph.

## production heritage

LogUp lookup arguments have been deployed in production by Polygon and Scroll since 2023 for cross-table consistency in ZK rollups. [[cyber]] applies the same technique to graph index consistency rather than rollup state tables.

see [[BBG]] for the full graph architecture, [[cross-index]] for the canonical consistency specification, [[WHIR]] for the underlying proof mechanism, [[NMT]] for the index structure
