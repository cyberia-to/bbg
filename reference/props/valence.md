---
title: "valence: ternary epistemic field"
status: implemented
date: 2026-03-17
---
# valence

valence is the ternary epistemic field v in {-1, 0, +1} in the [[cyberlink]] 7-tuple:

```
cyberlink = (neuron, from, to, token, amount, valence, time)
                                              ^^^^^^^^
                                              field 6 of 7
```

valence encodes the neuron's epistemic stance on the link: agree (+1), neutral (0), disagree (-1).

## privacy boundary

individual valence is PRIVATE. it is part of the cyberlink record, which is a private 7-tuple committed to cyberlinks.root (AOCL). who assigned what valence is never disclosed.

aggregate valence (meta-score) is PUBLIC. it appears in axon-particle leaves within particles.root. the meta-score aggregates all individual valence contributions into a single public signal for the axon H(from, to).

## relationship to ICBS

valence seeds the ICBS (Information-Conditional Bayesian Scoring) market mechanism. the axon market state (s_YES, s_NO) in axon-particle leaves reflects the aggregate prediction derived from individual valence signals. neurons express epistemic positions through valence; the market aggregates them into prices.

## location proofs

location proofs are a separate concern. they have their own root (locations.root) — one of the 13 sub-roots in the BBG root. location proofs attest that a [[particle]] or [[neuron]] is associated with a physical or logical location. they require:

- locations.root NMT index
- an oracle or attestation mechanism for location claims
- cross-index consistency with existing indexes via [[LogUp]]

location proofs are orthogonal to valence. valence is epistemic (what do you think about this link). location is spatial (where is this entity).
