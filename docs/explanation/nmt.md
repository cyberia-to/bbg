---
tags: cyber, computer science, cryptography
crystal-type: entity
crystal-domain: computer science
alias: namespaced Merkle tree, namespaced Merkle trees, NMTs
---
# NMT: from authentication to storage optimization

## what changed

[[NMT]] (Namespace Merkle Tree) was originally bbg's authentication mechanism — the sorted invariant at internal nodes provided completeness proofs. the polynomial state architecture replaced NMTs for authentication: Lens binding now provides completeness via algebraic guarantees.

NMT survives in a different role: **cold storage disk layout optimization**.

## the original role (authentication)

an NMT is a binary [[hemera]] hash tree where each internal node carries min/max namespace labels. the sorting invariant guaranteed: if a leaf exists in namespace N, it MUST appear in any completeness proof for N. omission was structurally impossible.

this was replaced by polynomial evaluation:

```
NMT completeness:         walk tree, check sorting invariant    O(log n) hemera hashes
polynomial completeness:  Lens opening, check binding            O(1) field operations
```

the polynomial approach is 33× cheaper per [[cyberlinks|cyberlink]] and eliminates cross-index consistency proofs entirely. see [[indexes]] for the polynomial evaluation dimensions.

## the surviving role (cold storage)

NMT's sorted namespace property is optimal for cold storage on slow disks:

```
cold storage:  historical state on HDD/network
access pattern: "give me all entries in namespace N"

NMT layout:    entries sorted by namespace → sequential disk region
HDD sequential: 200 MB/s
HDD random:     0.1 MB/s (8 ms seek)
ratio:          2000×
```

| role | polynomial state | NMT |
|---|---|---|
| authentication (trust) | primary: Lens opening, O(1) | unnecessary |
| hot storage (RAM) | flat array | unnecessary |
| warm storage (SSD) | B+ tree | unnecessary |
| cold storage (HDD) | not designed for disk | **useful**: sorted namespace = sequential scan |
| [[DAS]] chunk layout | Lens-based algebraic DAS | **useful**: namespace-sorted chunks |

the separation: polynomial provides AUTHENTICATION (cryptographic). NMT provides LAYOUT (physical). they serve different layers of the storage stack.

see [[storage]] for the storage tiers and ShardStore trait, [[data-availability]] for algebraic DAS, [[indexes]] for polynomial evaluation dimensions, [[data structures for polynomial state]] for the full theory
