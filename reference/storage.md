---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# storage

tiered storage model for the [[cybergraph]]. individual [[cyberlinks]] are private records in the [[mutator set]] — they never appear as public data. public storage holds [[particles]], [[axons]], [[neuron]] summaries, and aggregate state.

## storage tiers

```
L1: Hot state
    NMT roots, aggregate data, mutator set state
    contents: 13 sub-roots (32 bytes each), active SWBF window (128 KB),
              MMR peaks for cyberlinks.root and spent.root
    size: O(roots + SWBF_window) — kilobytes to megabytes
    latency: sub-millisecond (in-memory)

L2: Particle data
    full particle and axon data, indexed by CID
    contents: particle energy, π* values, axon weights, market state,
              neuron focus/karma/stake, coin/card metadata
    size: O(particles + axons + neurons) — gigabytes to terabytes
    latency: milliseconds (SSD)
    content-addressed, append-mostly (axon weights update on new cyberlinks)

L3: Content store
    particle content (files), indexed by CID
    contents: raw content bytes for each particle
    size: unbounded — petabytes across network
    latency: seconds (network retrieval)
    DAS availability proofs via files.root
    self-authenticating: H(content) = CID

L4: Archival
    historical state via time.root snapshots
    contents: BBG_root snapshots at epoch boundaries
    size: unbounded
    latency: minutes to hours
    DAS ensures availability during active window
```

## private record lifecycle

individual cyberlinks exist only as private records in the mutator set. the public layer never sees the 7-tuple.

```
creation:
  1. neuron constructs cyberlink c = (ν, p, q, τ, a, v, t)
  2. addition record ar = H_commit(c ‖ ρ) appended to cyberlinks.root (AOCL)
  3. public aggregates updated:
     - axon H(p,q) weight incremented by a in particles.root
     - axon entry updated in axons_out.root and axons_in.root
     - neuron ν focus decremented in neurons.root
     - particle p and q energy updated in particles.root
  4. LogUp proof: aggregate deltas consistent across all three NMTs

active:
  private record exists in mutator set (provable via AOCL membership)
  public aggregates reflect the sum of all active private records

spending:
  1. neuron proves ownership of the private record (AOCL membership + secret)
  2. nullifier bits set in SWBF (balance.root)
  3. public aggregates decremented accordingly
  4. double-spend = all SWBF bits already set = structural rejection
```

## public aggregate storage

the public layer stores only aggregates derived from private cyberlinks.

```
particles.root (NMT):
  key: CID (32 bytes)
  value: energy (8 bytes), π* (8 bytes)
  axon-particles additionally: weight (8 bytes), market state (16 bytes), meta-score (8 bytes)

axons_out.root (NMT):
  key: source particle namespace
  value: pointer to axon-particle in particles.root

axons_in.root (NMT):
  key: target particle namespace
  value: pointer to axon-particle in particles.root

neurons.root (NMT):
  key: neuron_id (hash of public key)
  value: focus (8 bytes), karma (8 bytes), stake (8 bytes)
```

no individual cyberlink data is stored publicly. the mapping from private records to public aggregates is one-way: many cyberlinks contribute to one axon weight, but the axon weight reveals nothing about individual contributions.

## content availability (L3)

particles are content-addressed: CID = H(content). L2 stores the particle metadata (energy, π*). L3 stores the actual content bytes. files.root provides DAS availability proofs — evidence that content is retrievable, not just that CIDs exist.

retrieval: any node with the CID can request content from any peer. verification: H(retrieved_content) = CID. no proof infrastructure required for content authentication. the gap is availability, addressed by files.root DAS.

## storage reclamation

when an axon's aggregate weight decays below threshold ε (see [[temporal]]):

```
1. axon-particle removed from particles.root
2. axon entry removed from axons_out.root and axons_in.root
3. LogUp proof of consistent removal across all three NMTs
4. if particle has zero remaining energy (no other axons reference it),
   particle eligible for L3 content reclamation
5. L4 archival snapshots remain valid at their height
```

see [[architecture]] for the layer model, [[cross-index]] for LogUp consistency, [[temporal]] for axon decay
