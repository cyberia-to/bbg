---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0024406036977162133
heat: 0.0016946955387266466
focus: 0.001124732041403175
gravity: 0
density: 1.33
---
# indexes

the [[cybergraph]] maintains 10 public evaluation dimensions within the unified BBG polynomial committed as [[BBG root|architecture]]. individual [[cyberlinks]] are private — indexes contain only public aggregates: [[particles]], [[axons]], [[neuron]] summaries, [[tokens]], and temporal state.

## polynomial index structure

all public indexes are evaluation dimensions of BBG_poly(index, key, t). the data at each evaluation point is the same as the former NMT leaf data. the commitment mechanism changes — hash trees are replaced by polynomial binding.

```
BBG_poly(index, key, t) = value

query:  PCS.open(BBG_root, (index, key, t)) → (value, proof)
verify: PCS.verify(BBG_root, (index, key, t), value, proof)

verification cost: O(1) field operations, 10-50 μs
proof size: ~200 bytes per opening (vs ~1 KiB NMT path)
```

## completeness proofs

the defining capability: prove "these are ALL items in a given namespace."

```
COMPLETENESS PROOF for namespace N at dimension D:

  polynomial approach:
    PCS opening over the evaluation region where index = D and key ∈ N
    the polynomial's binding property guarantees: the committed polynomial
    evaluates to exactly the claimed values at the queried points.
    omission is algebraically prevented by PCS soundness.

  batch opening for range [a, b]:
    one batch PCS proof covers all entries in the range
    verification: O(1) — one PCS check

ABSENCE PROOF for key K:
  PCS opening at (D, K, t) → 0
  proves: no entry exists at this evaluation point

cost:
  proof size: O(1) per opening ≈ ~200 bytes
  verification: O(1) field operations
  for n = 2^32: same cost (polynomial, not tree depth)
```

## the 10 polynomial evaluation dimensions

```
DIMENSION 1: particles
  evaluation: BBG_poly(particles, CID, t) → particle_record
  key: CID (content hash)
  proves: "particle P exists with energy E and focus π*"
  note: content-particles and axon-particles share the same dimension

DIMENSION 2: axons_out
  evaluation: BBG_poly(axons_out, source_CID, t) → axon_pointers
  key: source particle CID
  proves: "these are ALL outgoing axons from particle P"

DIMENSION 3: axons_in
  evaluation: BBG_poly(axons_in, target_CID, t) → axon_pointers
  key: target particle CID
  proves: "these are ALL incoming axons to particle P"

DIMENSION 4: neurons
  evaluation: BBG_poly(neurons, neuron_id, t) → neuron_record
  key: neuron_id (hash of public key)
  proves: "neuron N has focus F, karma κ, stake S"

DIMENSION 5: locations
  evaluation: BBG_poly(locations, entity_id, t) → location_record
  key: neuron_id or validator_id
  proves: "entity E has proof of location L"

DIMENSION 6: coins
  evaluation: BBG_poly(coins, denom_hash, t) → supply_record
  key: denomination hash
  proves: "denomination D has supply S, and this is complete"

DIMENSION 7: cards
  evaluation: BBG_poly(cards, card_id, t) → card_record
  key: card_id
  proves: "card C exists, bound to axon-particle P, owned by N"

DIMENSION 8: files
  evaluation: BBG_poly(files, CID, t) → availability_record
  key: CID (content hash)
  proves: "content for particle P is retrievable (DAS)"

DIMENSION 9: time
  evaluation: BBG_poly(time, boundary, t) → snapshot
  key: time boundary value
  proves: "BBG_root at time T was R"
  note: replaces 7-namespace NMT. continuous — any t is queryable via PCS opening

DIMENSION 10: signals
  evaluation: BBG_poly(signals, step, t) → signal_batch_hash
  key: step counter / batch index
  proves: "signal batch at step S was accepted"
```

## leaf structures

the data at each evaluation point matches the former NMT leaf structures. the polynomial encodes the same information — only the commitment changes.

```
particle entry (content-particle):
  key: CID                             32 bytes
  energy: F_p                          8 bytes
  π*: F_p                              8 bytes

particle entry (axon-particle, extends content-particle):
  key: CID = H(from, to)              32 bytes
  energy: F_p                          8 bytes
  π*: F_p                              8 bytes
  weight A_{pq}: F_p                   8 bytes
  market state s_YES: F_p              8 bytes
  market state s_NO: F_p               8 bytes
  meta-score: F_p                      8 bytes

axons_out entry:
  key: source_CID                      32 bytes
  axon_particle_CID: F_p⁴             32 bytes

axons_in entry:
  key: target_CID                      32 bytes
  axon_particle_CID: F_p⁴             32 bytes

neuron entry:
  key: neuron_id                       32 bytes
  focus: F_p                           8 bytes
  karma κ: F_p                         8 bytes
  stake: F_p                           8 bytes

coins entry:
  key: denom_hash                      32 bytes
  total_supply: F_p                    8 bytes
  mint_authority: F_p⁴                 32 bytes
  max_supply: F_p (0 = unlimited)      8 bytes
  transfer_rules: u8                   1 byte

cards entry:
  key: card_id                         32 bytes
  bound_axon_particle: F_p⁴           32 bytes
  current_owner: F_p⁴                 32 bytes
  creator: F_p⁴                       32 bytes
  creation_time: u64                   8 bytes

files entry:
  key: CID                             32 bytes
  chunk_count: u32                     4 bytes
  erasure_commitment: F_p⁴            32 bytes
  availability_window: u64             8 bytes

locations entry:
  key: entity_id                       32 bytes
  location_claim: F_p⁴                32 bytes
  attestation_proof: F_p⁴             32 bytes
  timestamp: u64                       8 bytes

time entry:
  key: boundary_value                  8 bytes
  BBG_root_snapshot: F_p⁴             32 bytes

signals entry:
  key: batch_index                     8 bytes
  signal_batch_hash: F_p⁴             32 bytes
  recursive_proof_hash: F_p⁴          32 bytes
```

## cross-index consistency

cross-index consistency between particles, axons_out, and axons_in is structural. all three are evaluation dimensions of BBG_poly — the same committed polynomial. they cannot disagree because they are the same mathematical object evaluated at different points.

```
INVARIANT (enforced by construction)

for every axon-particle a = H(from, to):

  1. BBG_poly(particles, a, t) contains the axon data
  2. BBG_poly(axons_out, from, t) references a
  3. BBG_poly(axons_in, to, t) references a

consistency: structural — same polynomial, same commitment.
cost: 0 additional constraints (was ~1,500 per cyberlink via LogUp).
```

see [[architecture]] for the layer model and BBG root, [[cross-index]] for why LogUp is eliminated, [[privacy]] for the polynomial mutator set and private record lifecycle
