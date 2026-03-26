---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0023635433586825325
heat: 0.0016452369732423736
focus: 0.0010917222265962167
gravity: 0
density: 1.23
---
# temporal

[[axons]] decay over time. without a forgetting mechanism, the [[cybergraph]] grows without bound. temporal decay applies to axon aggregate weights — individual [[cyberlinks]] are private and decay is expressed through their aggregate contribution to the public axon weight.

time is a native dimension of BBG_poly. historical queries are polynomial evaluations at (index, key, t_past) — one Lens opening, O(1) verification.

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

conservation invariant: Σ focus_i + Σ active_axon_weights + decay_pool = 1. the decay pool is a single F_p value in the balance evaluation, updated each block as axons age.

## time as polynomial dimension

the former time.root NMT with 7 namespaces (steps, seconds, hours, days, weeks, moons, years) is replaced by the time dimension of BBG_poly:

```
BBG_poly(index, key, t) — t is a native variable

historical query:
  "what was π of particle P at block 1000?"
  → Lens.open(BBG_root, (particles, P, 1000))
  → one evaluation proof, ~5 μs verification

range query:
  "all focus changes for neuron N between t₁ and t₂"
  → evaluate BBG_poly(neurons, N, t₁) and BBG_poly(neurons, N, t₂)
  → two Lens openings, prove the difference

any time, any index, any key — one polynomial opening.
```

the 7 temporal granularities (steps, seconds, hours, days, weeks, moons, years) are encoded as evaluation regions within the time dimension. epoch snapshots at each granularity are evaluation points of BBG_poly at the corresponding time boundaries.

```
cost comparison:

  NMT time.root (previous):
    "state at time t" = walk NMT for namespace + open BBG_root snapshot
    ~96 hemera permutations = ~70K constraints per historical query

  polynomial time dimension:
    "state at time t" = Lens.open(BBG_root, (index, key, t))
    O(1) field operations per historical query
```

## pruning protocol

condition: w_eff(axon, current_block) < ε

```
1. prove w_eff < ε (~20 constraints for decay calculation)
2. update BBG_poly(particles, axon, t): remove axon-particle
3. update BBG_poly(axons_out, source, t): remove entry
4. update BBG_poly(axons_in, target, t): remove entry
5. return decayed weight to decay pool
6. cross-index consistency: structural (same polynomial, no separate proof)
```

cost: O(1) polynomial updates per dimension + batch Lens recommit. pruners earn a fraction of recycled focus.

when an axon is pruned, the source and target particles lose the axon's energy contribution. if a particle's total energy reaches zero (no remaining axons reference it), the particle is eligible for content reclamation at L3 (see [[storage]]). historical state is preserved in the polynomial — past evaluations at (index, key, t_before_pruning) still return the axon data.

## renewal

new cyberlinks to the same (from, to) pair refresh the axon:

```
neuron creates cyberlink c = (ν, p, q, τ, a, v, t_now):
  1. private record: extend commitment polynomial A(x) (O(1))
  2. BBG_poly(particles, H(p,q), t_now): weight updated, A_{pq} += a
  3. t_last updated to t_now
  4. w_eff resets: new weight × α^0 = new weight
  5. cross-index: structural (same polynomial update covers all dimensions)
```

renewal is implicit — any cyberlink targeting the same (from, to) pair extends the axon's life. no explicit renewal transaction. multiple neurons can independently contribute to the same axon's weight.

## epoch boundaries

decay computation batches at epoch boundaries.

```
epoch length: E blocks

at epoch boundary:
  1. for each axon: recompute w_eff using exact α^Δt
  2. prune axons below threshold ε
  3. batch polynomial update for all removals and weight changes
  4. recommit BBG_poly with updated evaluations (one Lens recommit)
  5. update decay pool
  6. BBG_poly(time, epoch_boundary, t): snapshot evaluation point

between epochs:
  w_eff approximated via linear interpolation from last epoch
  exact computation deferred to next epoch boundary
```

epoch boundaries correspond to temporal granularity levels. each epoch snapshot is an evaluation point in the time dimension, enabling temporal queries: "what was the graph state after decay at epoch E?" — one Lens opening.

## storage reclamation cascade

pruning an axon triggers effects through the storage tiers:

```
L1 (hot state):
  polynomial evaluation tables updated for affected dimensions
  BBG_poly recommitted — one Lens recommit per block
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
  historical state preserved in polynomial time dimension
  past evaluations at (index, key, t_past) still return pre-pruning data
  no separate snapshot storage needed — the polynomial encodes all history
```

see [[storage]] for storage tiers and π-weighted replication, [[cross-index]] for why cross-index consistency is structural, [[architecture]] for the layer model
