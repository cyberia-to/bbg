---
title: "storage proofs: proving data retention at all tiers"
status: draft → resolved by signal-first
date: 2026-03-17
diffusion: 0.00010722364868599256
springs: 0.00041965751464741007
heat: 0.00032663046559973233
focus: 0.0002448351718571626
gravity: 0
density: 0.87
---
# storage proofs

the biggest gap in bbg: particles.root proves particle existence, but where does the actual content live? how is availability proven across all four storage tiers?

## the problem

bbg authenticates state (13 sub-roots committed in BBG_root) but does not prove that the underlying data is stored and retrievable. particles.root says "particle P exists with energy E" — but if nobody stores the actual content, the knowledge graph is a collection of hashes pointing to nothing.

files.root addresses content availability via DAS, but retention over time and across tiers remains an open problem.

## storage tiers and their gaps

```
L1: Hot state (NMT roots, aggregate data, mutator set state)
    availability: guaranteed by validators running the chain
    gap: none — this IS the consensus state

L2: Particle data (full particle/axon data, indexed by CID)
    availability: validators store it, SSD-local, millisecond access
    gap: after active window, availability depends on archival nodes
    who is incentivized to store old particle/axon data?

L3: Content store (particle content / files, indexed by CID)
    availability: committed via files.root, DAS proves retrievability
    gap: DAS proves availability at commitment time. long-term retention
    requires ongoing proof of storage. if particle content disappears,
    the cybergraph is a graph of dead links.

L4: Archival (historical state snapshots, old proofs)
    availability: DAS during active window, no formal guarantees after
    gap: historical queries require someone to have kept old state
```

## what storage proofs need to cover

1. **L2 retention**: prove that particle/axon data behind particles.root is stored and retrievable. challenge-response: "give me particle at CID C" -> particle data + NMT inclusion proof.

2. **L3 retention**: prove that particle content behind files.root is stored. this may require:
   - proof of storage (like Filecoin's PoRep/PoSt but over [[Goldilocks field]])
   - incentive mechanism: neurons pay focus to keep particles alive
   - relationship to temporal decay: particles with zero inbound energy have no retention incentive

3. **axon-particle -> content binding**: axon-particles in particles.root reference particle CIDs (from, to). particles.root commits to the existence of those particles. files.root proves their content is retrievable. the full chain: BBG_root -> particles.root (existence) -> files.root (retrievability).

4. **cross-tier consistency**: BBG_root authenticates L1. L1 references L2 via NMT commitments. L2 references L3 via particle CIDs. proving the full chain: BBG_root -> particles.root -> CID -> content in files.root.

## design constraints

- must work over [[Goldilocks field]] and be provable in [[zheng]]-2
- must respect bounded locality (law 1): proving storage of MY data costs proportional to MY data
- must compose with [[hemera]]-2 (32-byte hashes, 1 perm/node)
- must integrate with temporal decay: expired content releases storage obligations

## resolution: signal-first + structural sync

[[signal-first]] reframes this problem: prove signal availability, derive everything else. [[structural sync|structural-sync]] provides the formal framework:

```
layer 3 (completeness):  per-neuron NMT proves signal set is complete
layer 4 (availability):  DAS + erasure coding proves signals physically exist

if signals are complete and available → BBG state is reconstructible
→ L2, L3, L4 storage "proofs" reduce to signal availability proofs
→ no separate PoSt or PoRep needed
```

the remaining gap: **content availability**. signals carry cyberlink CIDs but not the content behind them. if content is lost while signals survive, the graph structure is intact but content is gone. files.root + DAS addresses this, but long-term content retention still depends on [[pi-weighted-replication]] incentives.

## original open questions (pre signal-first)

1. is proof of storage (PoSt-style) needed, or is incentivized replication sufficient? → **resolved: signal availability + DAS is sufficient for state; π-weighted replication for content**
2. how does storage pricing interact with focus economics? → **resolved: focus IS the storage payment via π**
3. what is the minimum viable storage proof for pre-genesis? → **resolved: DAS over signal batches**
4. can DAS be extended to cover long-term L3 retention, or is a separate mechanism needed? → **open: DAS proves availability at commitment time, not indefinitely**
5. what happens when particle content is lost? how does the graph degrade? → **open: graph structure survives (signals), content may not**