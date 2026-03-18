---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# temporal

[[axons]] decay over time. without a forgetting mechanism, the [[cybergraph]] grows without bound. temporal decay applies to axon aggregate weights — individual [[cyberlinks]] are private and decay is expressed through their aggregate contribution to the public axon weight.

## exponential weight decay

```
w_eff(axon, t) = A_{pq} × α^(t - t_last)
```

where:
- A_{pq} is the axon's current aggregate weight (sum of all contributing cyberlink weights)
- α ∈ (0, 1) is the global decay constant
- t_last is the timestamp of the most recent cyberlink contributing to this axon

properties:
- each axon decays independently (bounded locality)
- decayed weight returns to the global [[focus]] pool (conservation)
- axon with w_eff < ε is eligible for pruning
- new cyberlinks to the same (from, to) pair refresh the axon weight

provable: α^n approximated via Taylor series in F_p — 4 terms gives ~10⁻⁶ precision, ~20 field operations = ~20 constraints.

conservation invariant: Σ focus_i + Σ active_axon_weights + decay_pool = 1. the decay pool is a single F_p value in the balance NMT, updated each block as axons age.

## pruning protocol

condition: w_eff(axon, current_block) < ε

```
1. prove w_eff < ε (~20 constraints for decay calculation)
2. remove axon-particle from particles.root
3. remove axon entry from axons_out.root
4. remove axon entry from axons_in.root
5. return decayed weight to decay pool
6. LogUp proof of consistent removal from all three NMTs
```

cost: O(log n) NMT updates per tree + LogUp proof. pruners earn a fraction of recycled focus.

when an axon is pruned, the source and target particles lose the axon's energy contribution. if a particle's total energy reaches zero (no remaining axons reference it), the particle is eligible for content reclamation at L3 (see [[storage]]).

## renewal

new cyberlinks to the same (from, to) pair refresh the axon:

```
neuron creates cyberlink c = (ν, p, q, τ, a, v, t_now):
  1. private record appended to cyberlinks.root (AOCL)
  2. axon H(p,q) aggregate weight updated: A_{pq} += a
  3. t_last updated to t_now
  4. w_eff resets: new weight × α^0 = new weight
  5. LogUp proof of consistent update across all three NMTs
```

renewal is implicit — any cyberlink targeting the same (from, to) pair extends the axon's life. no explicit renewal transaction. multiple neurons can independently contribute to the same axon's weight.

## epoch boundaries

decay computation batches at epoch boundaries, aligned with time.root epoch namespaces.

```
epoch length: E blocks

at epoch boundary:
  1. for each axon: recompute w_eff using exact α^Δt
  2. prune axons below threshold ε
  3. batch LogUp proof for all removals and weight updates
  4. recompute NMT roots for particles.root, axons_out.root, axons_in.root
  5. update decay pool in balance NMT
  6. snapshot BBG_root into time.root at the epoch namespace

between epochs:
  w_eff approximated via linear interpolation from last epoch
  exact computation deferred to next epoch boundary
```

epoch boundaries align with time.root namespaces (hours, days, weeks, moons, years). each epoch snapshot captures the post-decay BBG_root, enabling temporal queries: "what was the graph state after decay at epoch E?"

## storage reclamation cascade

pruning an axon triggers effects through the storage tiers:

```
L1 (hot state):
  NMT roots recomputed for all three trees
  immediate — part of the state transition

L2 (particle data):
  axon-particle data removed
  source/target particle energy decremented
  if particle energy reaches zero, metadata eligible for removal

L3 (content store):
  zero-energy particles: content eligible for reclamation
  DAS availability proofs no longer required
  retention is a node policy decision, not a protocol obligation

L4 (archival):
  epoch snapshot in time.root preserves pre-pruning state
  historical queries can reconstruct the axon at any past epoch
```

see [[storage]] for storage tiers, [[cross-index]] for LogUp consistency, [[architecture]] for the layer model
