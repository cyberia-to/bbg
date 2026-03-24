---
tags: article, cyber, cip
crystal-type: entity
crystal-domain: computer science
status: draft
stake: 10577346440909906
diffusion: 0.00010722364868599256
springs: 0.0014565253095514628
heat: 0.0010394716887343798
focus: 0.0006984637549553021
gravity: 0
density: 2.35
---

## Authenticated State Architecture for [[nox]]

Version 2.0 — March 2026

*"The network doesn't simulate thinking. The network IS thinking."*

---

## Abstract

The complete authenticated state architecture for [[nox]] — a planetary-scale collective intelligence system targeting 10^15 nodes with cryptographic proofs, privacy by construction, and bounded-locality updates.

Five ontological primitives ([[particle]], [[cyberlink]], [[neuron]], [[token]], [[focus]]) authenticated by five cryptographic data structures:

| Primitive | Role | Heritage |
|-----------|------|----------|
| Namespaced Merkle Tree (NMT) | Graph completeness proofs | Celestia (2023—) |
| Merkle Mountain Range (MMR) | Append-only UTXO history | Grin, Neptune (2019—) |
| Sliding-Window Bloom Filter (SWBF) | Private double-spend prevention | Neptune (2024—) |
| WHIR Polynomial Commitments | Edge membership & batch proofs | WHIR (2025) |
| LogUp Lookup Arguments | Cross-index consistency | Polygon, Scroll (2023—) |

Unified by [[hemera]]-2 (32-byte output, 24 rounds, ~736 constraints/perm), [[Goldilocks field]], and [[zheng]]-2 (1-5 KiB proofs, 10-50 μs verification, folding-first composition).

---

## Three Laws

1. **Bounded Locality.** No global recompute for local change. Every operation's cost proportional to what it touches.
2. **Constant-Cost Verification.** Verification cost is O(1) — bounded by a constant independent of computation size. any computation produces a proof verifiable in 10-50 μs via [[zheng]]-2 folding. the verifier's work is independent of the prover's work.
3. **Structural Security.** Guarantees from data structure invariants, not protocol correctness.

See [[design-principles]] for the full argument.

---

## The Stack

```
STORAGE TIERS                    AUTHENTICATED STATE
─────────────                    ───────────────────

L1: Hot state (in-memory)        NMT roots, aggregate data, mutator set state
                                 32-byte roots, sub-millisecond

L2: Particle data (SSD)          full particle/axon data, indexed by CID
                                 content-addressed, milliseconds

L3: Content store (network)      particle content (files), indexed by CID
                                 DAS availability proofs, seconds

L4: Archival                     historical state snapshots, old proofs
                                 DAS ensures availability during active window
```

```
PRIVACY MODEL
─────────────
PRIVATE (individual)                   PUBLIC (aggregate)
──────────────────────────────────     ──────────────────────────────────
cyberlink 7-tuple (ν, p, q, τ, a, v, t)
who linked what                        axon H(p,q): aggregate weight A_{pq}
individual conviction, valence         axon market state (s_YES, s_NO)
neuron linking history                 axon meta-score
market positions (TRUE/FALSE tokens)   neuron: focus, karma, stake
UTXO values, owners                    particle: energy, π*
                                       token: denominations, total supply
                                       content: availability proofs
```

```
BBG ROOT — 13 SUB-ROOTS
────────────────────────
particles.root       NMT (all particles: content + axons)
axons_out.root       NMT by source (outgoing axon index)
axons_in.root        NMT by target (incoming axon index)
neurons.root         NMT (focus, karma, stake)
locations.root       NMT (proof of location)
coins.root           NMT (fungible token denominations)
cards.root           NMT (names and knowledge assets)
files.root           NMT (content availability, DAS)
cyberlinks.root      MMR peaks hash (private record commitments)
spent.root           MMR root (archived consumption proofs)
balance.root         hemera hash (active consumption bitmap)
time.root            NMT (temporal index, 7 namespaces)
signals.root         MMR (finalized signal batches)
```

---

## Specification

The full specification is decomposed into focused reference documents:

| document | content |
|----------|---------|
| [[architecture]] | three laws, ontology, 13 sub-roots, privacy model, unified primitives |
| [[state]] | BBG root, state diagram, checkpoint, state transitions |
| [[privacy]] | mutator set (AOCL + SWBF), privacy boundary, record model, transfer circuit |
| [[cross-index]] | LogUp cross-index consistency, batch verification |
| [[sync]] | full/incremental namespace sync, light client protocol |
| [[data-availability]] | 2D Reed-Solomon, NMT commitment, fraud proofs, DAS |
| [[temporal]] | edge decay, pruning protocol, storage reclamation, renewal |

## Explanations

| document | question |
|----------|----------|
| [[why-nmt]] | why NMTs cannot be replaced by sorted polynomial commitments |
| [[why-mutator-set]] | why mutator set over polynomial + nullifier |
| [[design-principles]] | the three laws explained in depth |

## Open Design

| proposal | status | topic |
|----------|--------|-------|
| [[valence]] | implemented | ternary epistemic field in cyberlink 7-tuple |
| [[storage-proofs]] | draft | proving data retention at all storage tiers |

## Companion Systems

bbg is the authenticated state layer. it depends on and is used by:

| system | repo | role |
|--------|------|------|
| [[nebu]] | ~/git/nebu/ | Goldilocks field arithmetic |
| [[hemera]] | ~/git/hemera/ | hash function (32-byte output, 24 rounds, x^{-1} S-box) |
| [[nox]] | ~/git/nox/ | VM (16 reduction patterns, CCS constraints) |
| [[zheng]] | ~/git/zheng/ | proof system (folding-first, algebraic opening, 1-5 KiB proofs) |
| [[mudra]] | ~/git/mudra/ | crypto primitives (signatures, key derivation) |

## Key Numbers (hemera + zheng-2)

```
hash output:          32 bytes (4 Goldilocks elements)
tree node:            64 bytes (2 × 32B children) → 1 permutation call
proof size:           1-5 KiB
verification:         10-50 μs
fold per block:       ~30 field ops
private transfer:     ~40,000 constraints, sub-second proving
cross-index (LogUp):  ~500 constraints per edge (15× savings)
light client join:    ONE zheng verification + namespace sync
```

---

*purpose. link. energy.*