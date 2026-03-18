---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
alias: BBG, Big Badass Graph, privacy model, nox privacy, ZK privacy, cyber/privacy
stake: 43936669831471920
---
# BBG: Big Badass Graph

individual [[cyberlinks]] are private — who linked what is never disclosed. the [[cybergraph]] is the public aggregate: [[axons]] (directed weights between [[particles]]), [[neuron]] summaries, [[particle]] energy, [[token]] supplies, π* distribution. all derived from cyberlinks, revealing no individual contribution.

bbg stores this split through [[NMT]] completeness proofs on the public side and a [[mutator set]] (AOCL + SWBF) on the private side. NMTs provide structural completeness ("these are ALL items in namespace N") — the tree structure itself prevents omission.

unified by [[hemera]]-2 (32-byte output, 24 rounds, ~736 constraints/perm), [[Goldilocks field]], and [[zheng]]-2 (1-5 KiB proofs, 10-50 μs verification, folding-first composition).

## structure

13 sub-roots under BBG_root. 9 public [[NMT]] indexes, 3 private state roots, 1 finalization root. total: 416 bytes.

public NMTs: [[particles]].root, axons_out.root, axons_in.root, [[neurons]].root, locations.root, [[coins]].root, [[cards]].root, files.root, time.root.

private state: cyberlinks.root (AOCL MMR), spent.root (archived SWBF), balance.root (active SWBF window).

finalization: signals.root (finalized signal batches).

BBG_root = H(all 13 sub-roots).

## key properties

- **bounded locality**: every operation's cost is proportional to what it touches. no global recompute for local change.
- **constant-cost verification**: any computation produces a proof verifiable in 10-50 μs. verifier work is independent of prover work.
- **structural security**: guarantees from data structure invariants, not protocol correctness. a tree whose internal nodes carry min/max namespace labels cannot lie about completeness.

## cross-index consistency

[[LogUp]] lookup arguments prove that axon-particles appear consistently across particles.root, axons_out.root, and axons_in.root. ~500 constraints per axon update (15× savings over independent proofs). see [[cross-index]].

## names are cyberlinks

a [[name]] is a [[card]] bound to an axon-particle. since axons are particles (A6), names are human-readable identifiers for those particles. every name lives in cards.root as an NMT leaf. names resolve to the current target particle — the latest cyberlink determines the binding.

```
~mastercyb/blog → QmXyz

the name "blog" under neuron mastercyb creates an axon-particle:
  axon = H(H("blog"), QmXyz)

this axon exists in:
  particles.root   — as a particle with energy, weight
  axons_out.root   — indexed by source H("blog")
  axons_in.root    — indexed by target QmXyz
  cards.root       — as a named card for O(1) resolution
```

see [[indexes]] for NMT leaf structures, [[privacy]] for mutator set, [[storage]] for fjall layout, [[architecture]] for the layer model, [[architecture-overview]] for overview
