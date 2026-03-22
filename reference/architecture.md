---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0017011238266031621
heat: 0.0012045351663314363
focus: 0.0008048560055902217
gravity: 0
density: 2.54
---
# architecture

the authenticated state layer for [[cyber]]. individual [[cyberlinks]] are private — who linked what is never disclosed. the [[cybergraph]] is the public aggregate: [[axons]] (directed weights between [[particles]]), [[neuron]] summaries, [[particle]] energy, [[token]] supplies, π* distribution. all derived from cyberlinks, revealing no individual contribution.

## three laws

**law 1: bounded locality.** no global recompute for local change. every operation's cost is proportional to what it touches, not to the total graph size. at 10^15 nodes, global operations are physically impossible — light-speed delays across Earth exceed any acceptable latency bound. cyberlinks update public aggregate NMTs — O(log n) per affected namespace. private record lifecycle (creation, spending) touches the mutator set — O(log N). bridge operations (coin → focus) cross the private→public boundary explicitly.

**law 2: constant-cost verification.** verification cost is O(1) — bounded by a constant independent of computation size. any computation produces a proof verifiable in 10-50 μs via [[zheng]]-2 folding. the verifier's work is independent of the prover's work.

**law 3: structural security.** security guarantees emerge from data structure invariants, not from protocol correctness. a protocol can have bugs. a tree whose internal nodes carry min/max namespace labels cannot lie about completeness — the structure itself prevents it.

## ontology

five irreducible primitives. everything in the system is composed from these.

| primitive | role | identity |
|-----------|------|----------|
| [[particle]] | content-addressed node, atom of knowledge | hash of content (32 bytes) |
| [[cyberlink]] | private authenticated edge, unit of meaning | hash of (neuron, from, to, token, amount, valence, time) |
| [[neuron]] | agent with stake, identity, and focus | hash of public key |
| [[token]] | protocol-native value: [[coin]], [[card]], [[score]], [[badge]] | denomination hash / content hash |
| [[focus]] | emergent attention distribution, computed by [[tri-kernel]] | [[diffusion]], [[springs]], [[heat kernel]] |

derived: [[axon]] = H(from, to) ∈ P — aggregate of all cyberlinks between two particles. axons are particles (A6). the tri-kernel operates on axons, not individual cyberlinks.

## naming

three layers, three names:

- [[nox]] — the computation model (16 reduction patterns, deterministic costs)
- [[cybergraph]] — the data model (particles, cyberlinks, neurons, tokens, focus)
- [[bbg]] — the authenticated state structure (this spec)

## privacy model

```
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

see [[privacy]] for the full boundary specification and mutator set architecture.

## BBG root

```
BBG_root = H(
  particles.root       ‖    NMT (all particles: content + axons)
  axons_out.root       ‖    NMT by source (outgoing axon index)
  axons_in.root        ‖    NMT by target (incoming axon index)
  neurons.root         ‖    NMT (focus, karma, stake)
  locations.root       ‖    NMT (proof of location)
  coins.root           ‖    NMT (fungible token denominations)
  cards.root           ‖    NMT (names and knowledge assets)
  files.root           ‖    NMT (content availability, DAS)
  cyberlinks.root      ‖    MMR peaks hash (private record commitments)
  spent.root           ‖    MMR root (archived consumption proofs)
  balance.root         ‖    hemera-2 hash (active consumption bitmap)
  time.root            ‖    NMT (temporal index, 7 namespaces)
  signals.root         ‖    MMR (finalized signal batches)
)
```

13 sub-roots. each is 32 bytes ([[hemera]]-2 output). total input to root hash: 416 bytes = 52 F_p elements = ~7 absorption blocks.

## sub-root specification

### particles.root — NMT

all particles in one tree. content-particles and axon-particles share the same namespace. each leaf stores:

- CID (32 bytes) — content hash, namespace key
- energy (8 bytes) — aggregate Σ weight from all incoming axons
- π* (8 bytes) — focus ranking from tri-kernel

axon-particles carry additional fields:

- weight A_{pq} (8 bytes) — aggregate conviction
- market state: s_YES, s_NO (16 bytes) — ICBS reserve amounts
- meta-score (8 bytes) — aggregate valence prediction

direct lookup of any particle or axon by CID: O(log n).

### axons_out.root — NMT (directional index)

axon-particles indexed by source particle namespace. querying "all outgoing from p" is a single NMT namespace proof. leaf data: pointer to axon-particle in particles.root.

### axons_in.root — NMT (directional index)

axon-particles indexed by target particle namespace. querying "all incoming to q" is a single NMT namespace proof. leaf data: pointer to axon-particle in particles.root.

LogUp proves consistency: every axon in axons_out and axons_in exists in particles.root, and vice versa.

### neurons.root — NMT

one leaf per neuron. namespace key: neuron_id (hash of public key). each leaf stores:

- focus (8 bytes) — available attention budget
- karma κ (8 bytes) — accumulated BTS score
- stake (8 bytes) — total committed conviction

neuron linking history is private — only aggregates are committed here.

### locations.root — NMT

proof of location for neurons and validators. enables spatial queries, geo-sharding, and latency guarantees.

### coins.root — NMT

fungible token denominations. one leaf per denomination τ. stores total supply and denomination parameters.

### cards.root — NMT

non-fungible knowledge assets. every cyberlink is a card. names are cards bound to axon-particles (A6: axons are particles, names are human-readable identifiers for those particles).

### files.root — NMT

content availability commitments. DAS (Data Availability Sampling) over stored content. proves that particle content is retrievable, not just that CIDs exist. without this root, the knowledge graph is a collection of hashes pointing to nothing.

### cyberlinks.root — MMR peaks hash

the AOCL (Append-Only Commitment List). an MMR storing every private record ever committed. when a neuron creates a cyberlink, an addition record is appended:

```
ar = H_commit(record ‖ ρ)    where ρ is hiding randomness
```

never modified — append-only. peaks = O(log N) digests, each 32 bytes. the root is H(peak₀ ‖ ... ‖ peak_k). cyberlinks arrive in [[signals]] (batches with recursive proofs), but the AOCL commits individual records. see signals.root for the finalization layer.

### spent.root — MMR root

the SWBF inactive archive. an MMR storing compacted chunks of the Sliding-Window Bloom Filter. when a private record is spent, pseudorandom bit positions are derived from H_nullifier(record ‖ aocl_index ‖ ρ) and set in the SWBF. old window chunks compact into this MMR. double-spend = all bits already set = structural rejection.

### balance.root — hemera-2 hash

the SWBF active window. a bitmap of 2^20 bits (128 KB) tracking recent consumption. committed as hemera-2(window_bits) — a single 32-byte hash of the full bitmap. slides forward periodically: oldest chunk compacts into spent.root, fresh chunk enters active window.

### time.root — NMT (7 namespaces)

temporal index over the full chain history. 7 namespaces for 7 time units:

| namespace | unit | boundary |
|-----------|------|----------|
| steps | unix timestamp | every block |
| seconds | 1s | every second boundary |
| hours | 3600s | every hour boundary |
| days | 86400s | every day boundary |
| weeks | 604800s | every week boundary |
| moons | ~2551443s | every lunar cycle (~29.53 days) |
| years | ~31557600s | every year boundary |

each namespace contains an MMR of BBG_root snapshots at that granularity. no full state duplication — one 32-byte hash per boundary. queries: "state at block T" → steps namespace O(log T). "state at hour H" → hours namespace O(log H). NMT completeness proofs give "all boundaries in range."

### signals.root — MMR

finalized [[signal]] batches. a signal bundles cyberlinks with an [[impulse]] (π_Δ — the proven focus shift) and a recursive [[zheng]]-2 proof covering the entire batch. the cyberlink is the object of learning; the signal is the object of finalization. signals.root commits the finalization history — which batches were accepted and in what order.

## checkpoint

```
CHECKPOINT = (
  BBG_root,           ← all 13 sub-roots
  folding_acc,        ← zheng-2 accumulator (constant size, ~30 field elements)
  block_height        ← current height
)

checkpoint size: O(1) — a few hundred bytes
contains: proof that ALL history from genesis is valid
updated: O(1) per block via folding
```

## storage layers

```
L1: Hot state      NMT roots, aggregate data, mutator set state
                   in-memory, sub-millisecond, 32-byte roots

L2: Particle data  full particle/axon data, indexed by CID
                   SSD, milliseconds, content-addressed

L3: Content store  particle content (files), indexed by CID
                   network retrieval, seconds, DAS availability proofs

L4: Archival       historical state snapshots, old proofs
                   DAS ensures availability during active window
```

## unified primitives

| primitive | role | heritage |
|-----------|------|----------|
| NMT | graph completeness proofs, DAS | Celestia (2023—) |
| MMR | append-only record history | Grin, Neptune (2019—) |
| SWBF | private double-spend prevention | Neptune (2024—) |
| WHIR polynomial commitments | batch proofs, evaluation | WHIR (2025) |
| LogUp lookup arguments | cross-index consistency | Polygon, Scroll (2023—) |

unified by [[hemera]]-2 (32-byte output, 24 rounds, ~736 constraints/perm), [[Goldilocks field]], and [[zheng]]-2 (1-5 KiB proofs, 10-50 μs verification, folding-first).

see [[state]] for transaction types and state transitions, [[privacy]] for the mutator set and privacy boundary, [[cross-index]] for LogUp consistency proofs, [[sync]] for namespace synchronization, [[data-availability]] for DAS, [[temporal]] for edge decay