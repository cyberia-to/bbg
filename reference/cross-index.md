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

consistency proofs between the three public NMTs that index [[axons]]: particles.root, axons_out.root, and axons_in.root. [[LogUp]] lookup arguments prove that these three trees contain the same set of axon-particles and that aggregate weight deltas are correct across all three.

## LogUp protocol

LogUp proves "set {f₁, ..., f_m} is contained in table {t₁, ..., t_n}" via the algebraic identity:

```
Σ 1/(β - f_i) = Σ m_j/(β - t_j)
```

evaluated at a random challenge β via [[sumcheck]].

## cross-index consistency

three NMTs index the same set of axon-particles from different perspectives:

| tree | namespace key | stores |
|------|---------------|--------|
| particles.root | CID | all particles including axon-particles |
| axons_out.root | source particle | outgoing axon references |
| axons_in.root | target particle | incoming axon references |

the lookup statement:

```
"every axon in axons_out.root exists in particles.root,
 AND every axon in axons_in.root exists in particles.root,
 AND every axon-particle in particles.root appears in both
     axons_out.root and axons_in.root"
```

LogUp proof structure:

```
given axon-particle a = H(from, to) with weight A_{pq}:

  1. f = {a, a, a}  (same axon hash, looked up in 3 trees)
  2. t₁ = particles.root leaf evaluations (axon-particles subset)
  3. t₂ = axons_out.root leaf evaluations
  4. t₃ = axons_in.root leaf evaluations

  prover constructs the sumcheck polynomial and commits.
  verifier checks the identity at random β.
```

## aggregate weight consistency

when a [[cyberlink]] updates an axon's aggregate weight, LogUp proves the delta is correct across all three trees:

```
transaction: new cyberlink adds weight δ to axon H(p, q)

lookup statement:
  "the weight delta +δ applied to axon H(p,q) in particles.root
   equals the delta applied in axons_out.root[p]
   equals the delta applied in axons_in.root[q]"

the prover commits:
  1. old weight A_{pq} in all three trees (must match)
  2. delta δ (from the cyberlink's public aggregate contribution)
  3. new weight A_{pq} + δ in all three trees (must match)

LogUp verifies all three trees transition from the same old value
to the same new value via the same delta.
```

## batch verification

LogUp batches naturally. a block containing B transactions updating K distinct axons:

```
all K axon updates → one LogUp proof for all 3K lookups

prover: O(K log K) — linear in number of affected axons
verifier: O(log K) — logarithmic in number of affected axons

constraints for cross-index consistency of entire block:
  ~500 × K + O(log K) amortization

for a block updating 10,000 axons:
  ~5,000,000 constraints (vs. 75,000,000 without LogUp)
```

cost is proportional to the number of axons touched, not to the total number of cyberlinks in the block (many cyberlinks may aggregate into the same axon).

## decay batch consistency

at epoch boundaries, batch decay recomputes axon weights across all three trees (see [[temporal]]). LogUp proves the decayed weights are consistent:

```
for each axon with old weight A and new weight A' = A × α^Δt:
  particles.root, axons_out.root, and axons_in.root
  all transition from A to A'

one LogUp proof covers all decayed axons in the epoch.
```

see [[architecture]] for the NMT structure, [[temporal]] for decay protocol, [[storage]] for the storage model