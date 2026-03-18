---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
alias: BBG, Big Badass Graph, privacy model, nox privacy, ZK privacy, cyber/privacy
stake: 43936669831471920
---
# BBG: Big Badass Graph

individual [[cyberlinks]] are private — who linked what is never disclosed. the [[cybergraph]] is the public aggregate: [[axons]] (directed weights between [[particles]]), [[neuron]] summaries, [[particle]] energy, [[token]] supplies, π* distribution. all derived from cyberlinks, revealing no individual contribution.

bbg stores this split through NMT completeness proofs on the public side and a [[mutator set]] (AOCL + SWBF) on the private side. NMTs provide structural completeness ("these are ALL items in namespace N") — the tree structure itself prevents omission. "sync only my namespace" is a mathematical property, not a feature. a light client tracking one particle downloads only data touching that particle, with proof that the response is complete. no trust in the data provider required.

unified by [[hemera]]-2 (32-byte output, 24 rounds, ~736 constraints/perm), [[Goldilocks field]], and [[zheng]]-2 (1-5 KiB proofs, 10-50 μs verification, folding-first composition).

## Structure

```
╔═══════════════════════════════════════════════════════════════════════════╗
║                    BBG: BIG BADASS GRAPH STRUCTURE                       ║
╠═══════════════════════════════════════════════════════════════════════════╣
║                                                                         ║
║  PUBLIC NMTs (9 roots)                                                  ║
║  ─────────────────────                                                  ║
║    particles.root     NMT  all particles: content + axons, energy, π*   ║
║    axons_out.root     NMT  by source (outgoing axon index)              ║
║    axons_in.root      NMT  by target (incoming axon index)              ║
║    neurons.root       NMT  focus, karma, stake per neuron               ║
║    locations.root     NMT  proof of location                            ║
║    coins.root         NMT  fungible token denominations                 ║
║    cards.root         NMT  names and knowledge assets                   ║
║    files.root         NMT  content availability (DAS)                   ║
║    time.root          NMT  temporal index (7 namespaces)                ║
║                                                                         ║
║  PRIVATE STATE (3 roots)                                                ║
║  ────────────────────────                                               ║
║    cyberlinks.root    MMR  peaks hash (append-only commitment list)     ║
║    spent.root         MMR  root (archived consumption proofs)           ║
║    balance.root       hash active consumption bitmap (SWBF 128 KB)     ║
║                                                                         ║
║  FINALIZATION (1 root)                                                  ║
║  ─────────────────────                                                  ║
║    signals.root       MMR  finalized signal batches                     ║
║                                                                         ║
║  GRAPH ROOT                                                             ║
║  ──────────                                                             ║
║    BBG_root = H(                                                        ║
║      particles.root  ‖ axons_out.root  ‖ axons_in.root  ‖              ║
║      neurons.root    ‖ locations.root  ‖ coins.root     ‖              ║
║      cards.root      ‖ files.root      ‖ time.root      ‖              ║
║      cyberlinks.root ‖ spent.root      ‖ balance.root   ‖              ║
║      signals.root                                                       ║
║    )   13 sub-roots × 32 bytes = 416 bytes                             ║
║                                                                         ║
╚═══════════════════════════════════════════════════════════════════════════╝
```

## Index Consistency Invariant

```
INVARIANT (enforced by zheng on every state transition)
───────────────────────────────────────────────────────

axon-based LogUp consistency across three NMTs:

  particles.root ↔ axons_out.root ↔ axons_in.root

for every axon H(p, q) committed in the system:

  1. H(p, q) exists as a leaf in particles.root
  2. H(p, q) exists in axons_out[p] namespace
  3. H(p, q) exists in axons_in[q] namespace

LogUp lookup arguments prove that the multisets match:
  every axon in axons_out and axons_in exists in particles.root,
  and vice versa. no axon can appear in a directional index
  without being committed as a particle.

proof size: 1-5 KiB. verification: 10-50 μs.
```

## Namespace Sync Protocol

```
NAMESPACE SYNC (NMT Completeness Proof)
───────────────────────────────────────

sync operates on public NMTs. four query classes:

  AXON NAMESPACES
    axons_out[p] — all outgoing axons from particle p
    axons_in[q]  — all incoming axons to particle q

  PARTICLE DATA
    particles[CID] — energy, π*, axon weight, market state

  NEURON STATE
    neurons[ν] — focus, karma, stake for neuron ν

  TEMPORAL QUERIES
    time[unit, range] — state snapshots at any granularity

to sync namespace ns from any public NMT:

  1. REQUEST
     client → peer: (nmt_index, namespace=ns, state_root=BBG_root)

  2. RESPONSE
     peer → client:
       - NMT completeness proof for namespace ns
       - leaf data for all items in namespace ns

  3. VERIFY
     client checks:
       a) NMT proof valid against the relevant sub-root (from BBG_root)
       b) all leaves present and correctly ordered
       c) namespace bounds match — no items omitted

  4. GUARANTEE
     "I have ALL items in namespace ns. Nothing hidden."
     proof is mathematical, not trust-based.

cost: O(|my_items|) data + O(log |tree|) × 32 bytes proof overhead
```

## Privacy Layer

[[nox]] implements private ownership with public aggregates. individual [[cyberlink]] ownership remains hidden — who linked what, who owns what, who sent to whom — while aggregate properties remain publicly verifiable: total energy per [[particle]], conservation laws, [[focus]] distribution. the network knows that energy is conserved without knowing who holds it. enough transparency for [[consensus]], enough privacy for participation.

the implementation uses a [[mutator set]] (AOCL + SWBF) for private record lifecycle with [[hemera]] commitments and ~40,000-constraint ZK circuits proving conservation. addition and removal records share zero structural similarity — unlinkability is architectural.

### Privacy Boundary

```
                   PUBLIC (aggregate)              PRIVATE (individual)
─────────────────  ──────────────────────────────  ─────────────────────────────
CYBERLINK                                          7-tuple (ν, p, q, τ, a, v, t)
                                                   who linked what
                                                   individual conviction amount
                                                   individual valence
NEURON             total focus                     linking history
                   karma κ                         individual cyberlinks
                   total stake
PARTICLE           CID exists
                   total energy (Σ weight)
                   π* ranking
AXON               H(from, to) exists              which neurons contributed
                   aggregate weight A_{pq}          individual weights
                   market state (s_YES, s_NO)
TOKEN              denominations                   individual UTXO values
                   total supply per τ               owner identity
RECORD                                             value, owner, nonce, randomness
TRANSACTION        SWBF bit indices                which records spent
                   addition records                who spent them
                   Δ per particle                  new owners
                   proof validity                  link between add & remove
FOCUS              π distribution
                   rankings
```

invariant: the ZK circuit MUST enforce this boundary. any violation breaks the economic integrity of collective attention.

### Mutator Set

the mutator set replaces both UTXO commitment polynomials and nullifier sets with two linked structures:

AOCL (Append-Only Commitment List) — an MMR storing addition records. appended when a UTXO is created, never modified. accumulator = O(log N) peak hashes. membership proof = Merkle path from leaf to peak.

SWBF (Sliding-Window Bloom Filter) — tracks which UTXOs have been spent by setting pseudorandom bit positions derived from the record. double-spend = all bits already set = verifier rejects. active window (128 KB) handles recent removals directly; older chunks compact into an MMR.

```
◄──── Inactive (compacted in MMR) ────►◄── Active Window ──►

┌──────┬──────┬──────┬──────┐  ┌──────────────────────────┐
│chunk₀│chunk₁│chunk₂│chunk₃│  │   2²⁰ bits (128 KB)     │
│(MMR) │(MMR) │(MMR) │(MMR) │  │   directly accessible    │
└──────┴──────┴──────┴──────┘  └──────────────────────────┘

window slides forward periodically.
oldest active chunk → compacted into MMR.
growth: O(log N) peaks regardless of chain age.
```

unlinkability: addition record = `H_commit(record ‖ ρ)`, removal record = SWBF bit positions derived from `H_nullifier(record ‖ aocl_index ‖ ρ)`. these share zero structural similarity. the ZK proof establishes validity without revealing which AOCL entry is being spent.

### Record Structure

```
Record:
  particle: F_p⁴    32 bytes   content identifier
  value:    u64       8 bytes   energy amount
  owner:    F_p⁴    32 bytes   owner public key hash
  nonce:    F_p       8 bytes   random for uniqueness

commitment(r, ρ) = H_commit(r.particle ‖ r.value ‖ r.owner ‖ r.nonce ‖ ρ)
```

### Transaction Circuit

```
PRIVATE TRANSFER CIRCUIT
════════════════════════

PUBLIC INPUTS:
  aocl_peaks:    [F_p⁴; log(N)]     AOCL MMR peak hashes
  swbf_root:     F_p⁴               SWBF inactive chunks MMR root
  swbf_window:   F_p⁴               hash of active SWBF window
  removal_data:  [BitIndices; 4]     SWBF bit positions per input
  additions:     [F_p⁴; 4]          new addition records
  deltas:        [(F_p⁴, i64); 8]   per-particle value changes
  fee:           u64                 transaction fee

PRIVATE WITNESS:
  input_records, input_secrets, input_randomness
  aocl_indices, aocl_paths, swbf_paths
  output_records, output_randomness
  input_enabled, output_enabled

CONSTRAINTS (hemera-2, 32-byte hashes, 1 perm/node):
  input validation (4 inputs):              ~36,000
    commitment correctness: ~736 per input
    AOCL membership (MMR path): ~4,000 per input
    SWBF index derivation: ~500 per input
    SWBF bit verification: ~3,000 per input
    ownership proof: ~736 per input
  output validation (4 outputs):             ~3,500
  conservation:                                ~100
  delta consistency:                           ~300
  uniqueness:                                   ~50

TOTAL:                                       ~40,000 constraints
proof generation (zheng-2):                  sub-second
proof size:                                  1-5 KiB
verification:                                10-50 μs
```

### Transaction Types

```
TRANSFER   — move energy between particles (1-4 in, 1-4 out)
MINT       — create new energy (0 in, 1 out, special proof)
BURN       — remove energy from circulation (1-4 in, 0-3 out)
SPLIT      — divide one record into multiple (1 in, 2-4 out, same particle)
MERGE      — combine multiple records (2-4 in, 1 out, same particle)
```

## Names Are Cyberlinks

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

names and cards are not separate concepts — one is the protocol-level [[name]] (a [[cyberlink]] creating a binding), the other is the storage-level commitment that makes resolution fast and provable. the cyberlink is the log. the card is the snapshot.

see [[privacy]] for full mutator set architecture, [[architecture]] for the layer model, [[architecture-overview]] for overview
