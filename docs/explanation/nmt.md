---
tags: cyber, computer science, cryptography
crystal-type: entity
crystal-domain: computer science
alias: namespaced Merkle tree, namespaced Merkle trees, NMTs
diffusion: 0.00010722364868599256
springs: 0.0021780347003353987
heat: 0.0015123942096783773
focus: 0.0010095010763792782
gravity: 0
density: 1.96
---
# NMT

namespaced Merkle tree. a binary Merkle tree over sorted leaves where each internal node carries the minimum and maximum namespace of its subtree. the defining property: structural completeness proofs — the tree physically cannot represent a valid root over misordered leaves.

invented for [[Celestia]] (2023). used in [[cyber]] as the primary index structure for the [[cybergraph]] ([[BBG]]).

## structure

```
internal node:
  NMT_node = (min_ns, max_ns, H_merkle(left_child ‖ right_child))

leaf node:
  NMT_leaf = (namespace, H_merkle(payload))

invariant:
  for every internal node N with children L, R:
    N.min_ns = L.min_ns
    N.max_ns = R.max_ns
    L.max_ns ≤ R.min_ns        ← sorting invariant
```

the sorting invariant is structural — enforced by construction. any valid NMT path that violates sorting produces an invalid Merkle root, detectable by any verifier with only the root hash.

## completeness proofs

the defining capability: prove "these are ALL items in namespace N."

```
COMPLETENESS PROOF for namespace N:

  1. path to leftmost leaf with namespace N
  2. path to rightmost leaf with namespace N
  3. left boundary: left neighbor has namespace < N (or is tree boundary)
  4. right boundary: right neighbor has namespace > N (or is tree boundary)

ABSENCE PROOF for namespace N:
  show two adjacent leaves with namespaces that bracket N:
    leaf_i.namespace < N < leaf_{i+1}.namespace

COST:
  proof size: O(log n) hash digests
  verification: O(log n) hash computations
  for n = 2³²: ~32 × 736 = ~23,500 stark constraints
```

## why NMTs

sorted [[polynomial commitments]] can approximate completeness but lack structural guarantees. polynomial completeness requires a protocol: prove boundary entries, prove contiguity, prove sorting was maintained — each step requires a separate argument. NMT completeness is a structural invariant: the tree physically cannot represent a valid root over misordered leaves. DAS requires NMTs for namespace-aware sampling. sync requires NMTs for provable namespace queries. see [[why-nmt]] for full analysis.

## use in cyber

the [[BBG]] maintains nine NMT indexes: particles, axons_out, axons_in, neurons, locations, coins, cards, files, time. individual [[cyberlinks]] are private — NMT indexes contain only public aggregates.

see [[indexes]] for leaf structures, [[cross-index]] for LogUp consistency across NMTs, [[data-availability]] for DAS