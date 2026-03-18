---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# indexes

the [[cybergraph]] maintains nine NMT indexes committed in the [[BBG root|architecture]]. individual [[cyberlinks]] are private — indexes contain only public aggregates: [[particles]], [[axons]], [[neuron]] summaries, [[tokens]], and temporal snapshots.

## NMT structure

a Namespaced Merkle Tree is a binary Merkle tree over sorted leaves where each internal node carries the minimum and maximum namespace of its subtree:

```
internal node:
  NMT_node = (min_ns, max_ns, H(left_child ‖ right_child))

leaf node:
  NMT_leaf = (namespace, H(payload))

invariant:
  for every internal node N with children L, R:
    N.min_ns = L.min_ns
    N.max_ns = R.max_ns
    L.max_ns <= R.min_ns        ← sorting invariant
```

the sorting invariant is structural — enforced by construction, not by protocol. any valid NMT path that violates sorting produces an invalid root, detectable by any verifier with just the root hash.

with [[hemera]]-2 (32-byte output): each node is 64 bytes (two 32-byte children), hashed in a single permutation call. tree hashing throughput: ~53 MB/s.

## completeness proofs

the NMT's defining capability: prove "these are ALL items in namespace N."

```
COMPLETENESS PROOF for namespace N:
  1. path to leftmost leaf with namespace N
  2. path to rightmost leaf with namespace N
  3. left boundary: left neighbor has namespace < N (or is tree boundary)
  4. right boundary: right neighbor has namespace > N (or is tree boundary)

VERIFICATION (by any client with only the root):
  a) both Merkle paths verify against root           — O(log n) hashes
  b) leftmost leaf has namespace = N                  — 1 comparison
  c) rightmost leaf has namespace = N                 — 1 comparison
  d) left neighbor namespace < N                      — 1 comparison
  e) right neighbor namespace > N                     — 1 comparison

ABSENCE PROOF for namespace N:
  show two adjacent leaves with namespaces that bracket N:
    leaf_i.namespace < N < leaf_{i+1}.namespace

cost:
  proof size: O(log n) × 32 bytes
  verification: O(log n) hemera calls = O(log n × 736) constraints
  for n = 2^32: ~32 × 736 = ~23,500 constraints
```

## the nine NMT indexes

```
INDEX 1: particles.root
  structure: NMT[ CID → particle_record ]
  namespace: CID (content hash)
  proves: "particle P exists with energy E and focus π*"
  note: content-particles and axon-particles share the same tree

INDEX 2: axons_out.root
  structure: NMT[ source_CID → axon_pointer ]
  namespace: source particle CID
  proves: "these are ALL outgoing axons from particle P"

INDEX 3: axons_in.root
  structure: NMT[ target_CID → axon_pointer ]
  namespace: target particle CID
  proves: "these are ALL incoming axons to particle P"

INDEX 4: neurons.root
  structure: NMT[ neuron_id → neuron_record ]
  namespace: neuron_id (hash of public key)
  proves: "neuron N has focus F, karma κ, stake S"

INDEX 5: locations.root
  structure: NMT[ neuron_id → location_record ]
  namespace: neuron_id
  proves: "neuron N has proof of location L"

INDEX 6: coins.root
  structure: NMT[ denom_hash → supply_record ]
  namespace: denomination hash
  proves: "denomination D has supply S, and this is complete"

INDEX 7: cards.root
  structure: NMT[ card_id → card_record ]
  namespace: card_id
  proves: "card C exists, bound to axon-particle P, owned by N"

INDEX 8: files.root
  structure: NMT[ CID → availability_record ]
  namespace: CID (content hash)
  proves: "content for particle P is retrievable (DAS)"

INDEX 9: time.root
  structure: NMT[ time_namespace → BBG_root_snapshot ]
  namespace: one of 7 time units (steps, seconds, hours, days, weeks, moons, years)
  proves: "BBG_root at boundary T was R"
```

## NMT leaf structures

```
particle leaf (content-particle):
  namespace: CID                          32 bytes
  energy: F_p                             8 bytes
  π*: F_p                                 8 bytes

particle leaf (axon-particle, extends content-particle):
  namespace: CID = H(from, to)            32 bytes
  energy: F_p                             8 bytes
  π*: F_p                                 8 bytes
  weight A_{pq}: F_p                      8 bytes
  market state s_YES: F_p                 8 bytes
  market state s_NO: F_p                  8 bytes
  meta-score: F_p                         8 bytes

axons_out leaf:
  namespace: source_CID                   32 bytes
  axon_particle_CID: F_p⁴                32 bytes

axons_in leaf:
  namespace: target_CID                   32 bytes
  axon_particle_CID: F_p⁴                32 bytes

neuron leaf:
  namespace: neuron_id                    32 bytes
  focus: F_p                              8 bytes
  karma κ: F_p                            8 bytes
  stake: F_p                              8 bytes

coins leaf:
  namespace: denom_hash                   32 bytes
  total_supply: F_p                       8 bytes
  mint_authority: F_p⁴                    32 bytes
  max_supply: F_p (0 = unlimited)         8 bytes
  transfer_rules: u8                      1 byte

cards leaf:
  namespace: card_id                      32 bytes
  bound_axon_particle: F_p⁴              32 bytes
  current_owner: F_p⁴                    32 bytes
  creator: F_p⁴                          32 bytes
  creation_time: u64                      8 bytes

files leaf:
  namespace: CID                          32 bytes
  chunk_count: u32                        4 bytes
  erasure_commitment: F_p⁴               32 bytes
  availability_window: u64                8 bytes

locations leaf:
  namespace: entity_id                    32 bytes
  location_claim: F_p⁴                   32 bytes
  attestation_proof: F_p⁴                32 bytes
  timestamp: u64                          8 bytes

time leaf:
  namespace: time_unit                    32 bytes
  boundary_value: u64                     8 bytes
  BBG_root_snapshot: F_p⁴                32 bytes
```

## cross-index consistency

LogUp lookup arguments prove consistency across the three particle-related indexes:

```
INVARIANT (enforced by zheng on every state transition)

for every axon-particle a = H(from, to) in particles.root:

  1. a appears in axons_out[from]
  2. a appears in axons_in[to]

for every entry in axons_out or axons_in:

  3. the referenced axon_particle_CID exists in particles.root
     as an axon-particle leaf

multiplicity:
  every axon-particle appears exactly once in particles.root,
  exactly once in axons_out (under source namespace),
  exactly once in axons_in (under target namespace).

cross-index consistency enforced via [[LogUp]] lookup arguments (see [[cross-index]]).
```

see [[architecture]] for the layer model and BBG root, [[cross-index]] for LogUp consistency proofs, [[privacy]] for the mutator set and private record lifecycle
