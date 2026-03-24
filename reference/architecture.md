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

**law 1: bounded locality.** no global recompute for local change. every operation's cost is proportional to what it touches, not to the total graph size. at 10^15 nodes, global operations are physically impossible — light-speed delays across Earth exceed any acceptable latency bound. cyberlinks update the BBG polynomial at affected evaluation dimensions — O(1) field operations per dimension touched. private record lifecycle (creation, spending) updates the commitment and nullifier polynomials — O(1) PCS operations. bridge operations (coin → focus) cross the private→public boundary explicitly.

**law 2: constant-cost verification.** verification cost is O(1) — bounded by a constant independent of computation size. any computation produces a proof verifiable in 10-50 μs via [[zheng]]-2 folding. any state query produces a PCS opening verifiable in O(1) field operations. the verifier's work is independent of the prover's work.

**law 3: structural security.** security guarantees emerge from the commitment scheme's binding property, not from protocol correctness. a protocol can have bugs. a polynomial commitment scheme whose binding is rooted in hemera collision resistance cannot produce two different openings for the same evaluation point — the algebra itself prevents it. completeness shifts from data structure invariants (NMT sorting) to algebraic binding (PCS soundness), both rooted in the same trust foundation: hemera.

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
BBG_root = PCS.commit(BBG_poly)    one Brakedown commitment, 32 bytes
```

BBG_poly is a multivariate polynomial over the Goldilocks field:

```
BBG_poly(index, key, t)

dimensions:
  index ∈ {particles, axons_out, axons_in, neurons, locations,
           coins, cards, files, time, signals,
           commitments, nullifiers}
  key   = namespace key within that index (particle CID, neuron ID, etc.)
  t     = time (block height)
```

every query is a polynomial evaluation:

```
"energy of particle P at time t"     = BBG_poly(particles, P, t)
"outgoing axons of particle P"       = BBG_poly(axons_out, P, t)
"focus of neuron N"                  = BBG_poly(neurons, N, t)
"nullifier status of n"              = BBG_poly(nullifiers, n, t)
"state at historical time t_past"    = BBG_poly(index, key, t_past)
```

one PCS opening per query. one verification per proof. 10-50 μs.

## evaluation dimensions

BBG_poly has 12 evaluation dimensions. each former sub-root is now an evaluation dimension of the same polynomial. the data is unchanged — the commitment mechanism changes from hash trees to polynomial binding.

### particles — evaluation dimension

all particles in one index. content-particles and axon-particles share the same namespace. each entry stores:

- CID (32 bytes) — content hash, namespace key
- energy (8 bytes) — aggregate Σ weight from all incoming axons
- π* (8 bytes) — focus ranking from tri-kernel

axon-particles carry additional fields:

- weight A_{pq} (8 bytes) — aggregate conviction
- market state: s_YES, s_NO (16 bytes) — ICBS reserve amounts
- meta-score (8 bytes) — aggregate valence prediction

direct lookup of any particle or axon by CID: one PCS opening, O(1).

### axons_out — evaluation dimension (directional index)

axon-particles indexed by source particle namespace. querying "all outgoing from p" is a single PCS batch opening. entry data: pointer to axon-particle in particles dimension.

### axons_in — evaluation dimension (directional index)

axon-particles indexed by target particle namespace. querying "all incoming to q" is a single PCS batch opening. entry data: pointer to axon-particle in particles dimension.

cross-index consistency is structural: axons_out, axons_in, and particles are evaluation dimensions of the SAME polynomial. they cannot disagree. LogUp is unnecessary.

### neurons — evaluation dimension

one entry per neuron. namespace key: neuron_id (hash of public key). each entry stores:

- focus (8 bytes) — available attention budget
- karma κ (8 bytes) — accumulated BTS score
- stake (8 bytes) — total committed conviction

neuron linking history is private — only aggregates are committed here.

### locations — evaluation dimension

proof of location for neurons and validators. enables spatial queries, geo-sharding, and latency guarantees.

### coins — evaluation dimension

fungible token denominations. one entry per denomination τ. stores total supply and denomination parameters.

### cards — evaluation dimension

non-fungible knowledge assets. every cyberlink is a card. names are cards bound to axon-particles (A6: axons are particles, names are human-readable identifiers for those particles).

### files — evaluation dimension

content availability commitments. DAS (Data Availability Sampling) over stored content. proves that particle content is retrievable, not just that CIDs exist. without this dimension, the knowledge graph is a collection of hashes pointing to nothing.

### time — evaluation dimension

time is a native dimension of BBG_poly. historical queries are polynomial evaluations at (index, key, t_past). the 7 temporal granularities (steps, seconds, hours, days, weeks, moons, years) are encoded as evaluation regions within the time dimension. any query at any past time is one PCS opening.

### signals — evaluation dimension

finalized [[signal]] batches. a signal bundles cyberlinks with an [[impulse]] (π_Δ — the proven focus shift) and a recursive [[zheng]]-2 proof covering the entire batch. the cyberlink is the object of learning; the signal is the object of finalization. the signals dimension commits the finalization history — which batches were accepted and in what order.

### commitments — evaluation dimension (private)

the commitment polynomial A(x). every private record (cyberlink, UTXO) has a commitment A(c_i) = v_i. membership proof: one PCS opening, O(1). append-only — new records extend the polynomial by one degree.

### nullifiers — evaluation dimension (private)

the nullifier polynomial N(x) = ∏(x - n_i). when a record is spent, its nullifier is absorbed into the polynomial. non-membership proof: prove N(c) ≠ 0 via one PCS opening, O(1). double-spend = N(c) = 0 = structural rejection.

## checkpoint

```
CHECKPOINT = (
  BBG_root,           ← PCS.commit(BBG_poly), 32 bytes
  folding_acc,        ← zheng-2 accumulator (constant size, ~30 field elements)
  block_height        ← current height
)

checkpoint size: O(1) — ~232 bytes
contains: proof that ALL history from genesis is valid
updated: O(1) per block via folding
proof size: 1-5 KiB, verification: 10-50 μs
```

## storage layers

```
L1: Hot state      polynomial commitments, aggregate data, private polynomial state
                   in-memory, sub-millisecond, 32-byte commitments

L2: Particle data  full particle/axon data, indexed by CID
                   SSD, milliseconds, content-addressed

L3: Content store  particle content (files), indexed by CID
                   network retrieval, seconds, DAS availability proofs

L4: Archival       historical state via polynomial time dimension
                   DAS ensures availability during active window
```

## unified primitives

| primitive | role | heritage |
|-----------|------|----------|
| PCS (Brakedown) | state commitment, completeness proofs, DAS | Brakedown (2023—) |
| BBG_poly | unified state polynomial | algebraic NMT evolution |
| commitment polynomial A(x) | private record commitments | Neptune mutator set heritage |
| nullifier polynomial N(x) | private double-spend prevention | Neptune SWBF heritage |
| WHIR polynomial commitments | batch proofs, evaluation | WHIR (2025) |

unified by [[hemera]] (32-byte output, 24 rounds, ~736 constraints/perm), [[Goldilocks field]], and [[zheng]] (1-5 KiB proofs, 10-50 μs verification, folding-first).

## pipeline

the end-to-end flow from cyberlink creation to light client verification:

```
DEVICE                          NETWORK                         CLIENT
──────                          ───────                         ──────
nox execution                   include signals in block        download checkpoint
  ↓ proof-carrying                ↓ polynomial state update       ↓ one zheng decider
hemera identity                 fold into block accumulator     sync namespaces
  ↓ folded sponge                 ↓ HyperNova folding             ↓ PCS openings
build signal                    fold into epoch accumulator     DAS sample
  ↓ (ν, l⃗, π_Δ, σ, prev...)     ↓ universal accumulator          ↓ algebraic DAS
local sync                      publish checkpoint              query
  ↓ CRDT + PCS + DAS              ↓ ~240 bytes                    ↓ verifiable query
submit to network
  ↓ foculus π convergence
```

| stage | where specified | cost |
|-------|----------------|------|
| signal creation | [[nox]] reduction.md (proof-carrying) | ~30 field ops per step |
| local sync | [[sync]] (structural sync layers 1-5) | ~200 bytes per namespace |
| block processing | [[state]] (polynomial updates) | ~3,200 constraints per cyberlink |
| epoch composition | [[sync]] + zheng recursion.md | ~100K constraints per epoch |
| light client | [[sync]] (checkpoint + PCS) | < 10 KiB, 10-50 μs |
| query | [[query]] (verifiable query compiler) | ~200 bytes to ~5 KiB proof |

see [[state]] for transaction types, [[privacy]] for the polynomial mutator set, [[cross-index]] for why LogUp is eliminated, [[sync]] for namespace synchronization, [[data-availability]] for algebraic DAS, [[temporal]] for the time dimension, [[query]] for verifiable queries
