---
tags: cyber
crystal-type: entity
crystal-domain: cyber
---
# why signal-first

## state is a derived function

BBG state at any height h is a deterministic function of the signal log:

```
BBG_state(h) = fold(genesis, signals[0..h])
```

every state transition is triggered by a signal. every signal is self-certifying (carries a zheng proof). the signal log is append-only and content-addressed. given the same signal sequence, any node computes the same BBG_root.

the polynomial commitments (BBG_poly, A, N), the NMT indexes, the axon aggregates, the neuron focus values — all of it is derived data. the signal log is the source of truth. BBG_root is a materialized view, not primary storage.

## the fold function

```
fold(state, signal) = {
  apply signal effects:
    new cyberlinks → update BBG_poly dimensions (particles, axons_out, axons_in, neurons)
    focus shifts → update BBG_poly(neurons, ...) and BBG_poly(particles, ...)
    economic transfers → extend A(x) and N(x)
  apply temporal decay:
    deterministic from state.time and decay parameters
    low-energy axons pruned, reclaimed focus returned
  update all commitments:
    BBG_root = H(commit(BBG_poly) ‖ commit(A) ‖ commit(N))
  return new state
}
```

the fold is deterministic. same input state + same signal = same output state. temporal decay is part of the fold — same decay parameters and same time means same pruning decisions. no randomness, no external input, no non-determinism.

## crash recovery

a node that crashes needs two things to resume:

```
1. latest checkpoint     ~240 bytes (BBG_root + universal accumulator + height)
2. signals since checkpoint    fetched from any peer via DAS
```

recovery procedure:

```
1. fetch checkpoint from any peer                         ~240 bytes
2. verify checkpoint (one zheng accumulator verification)  ~5 μs
3. fetch signals since checkpoint (DAS guarantees availability)
4. replay: fold(checkpoint_state, signals[checkpoint_h..now])
5. result: identical BBG_root as every other correct node
```

no special backup infrastructure. no archival nodes with privileged roles. every peer with signals IS a backup. the checkpoint is self-proving — verification is cryptographic (accumulator), not social (trust the peer).

## storage proofs reduce to signal availability

the question "is the data retained?" has a clean answer: prove the signal log is available.

```
signal availability guarantees:
  per-neuron completeness:   PCS proof that no signals were withheld
  physical availability:     DAS over signal batches (erasure-coded)
  validity:                  each signal carries its own zheng proof
```

if the signals are available, any state can be reconstructed. L2 particle data, L3 content, L4 archival snapshots — all derivable from signals. the only irrecoverable loss is if signals themselves are lost. signals are protected by erasure coding (any k-of-n chunks reconstruct) and DAS (O(sqrt(n)) samples verify availability with 99.9999% confidence).

storage proofs for derived state become unnecessary. prove signal availability — everything else follows from deterministic replay.

## the irreducible minimum

what a node must store:

```
irreducible:
  signal log        append-only, DAS-protected, PCS-complete
  latest checkpoint ~240 bytes (universal accumulator)

derivable (can be reconstructed from signals):
  NMT indexes       → rebuild by replaying cyberlink signals
  polynomial state  → rebuild by replaying all signals
  mutator set       → replay UTXO events from signals
  particle data     → extract from signal payloads
  axon aggregates   → recompute from individual cyberlinks
  focus/π values    → recompute tri-kernel from graph state
```

a node that stores only the signal log and a checkpoint can reconstruct the entire BBG state. caching derived state (indexes, aggregates, polynomial commitments) is an optimization for query speed — not a correctness requirement.

## reconstruction cost

full replay from genesis:

```
h signals × ~2 KiB each = O(h) data
verification: one zheng proof per signal = O(h × ~5 μs)
at 10^9 signals: ~2 TB data, ~5 seconds verification (parallelizable)
```

incremental replay from checkpoint:

```
download checkpoint: 240 bytes
verify: ~5 μs
replay signals since checkpoint: O(delta_h) data
```

the common case is incremental. a node that was offline for one hour replays ~100 signals (~300 KiB). a node joining for the first time verifies one checkpoint and replays from there — or trusts a recent checkpoint (cryptographic trust via accumulator, not social trust) and starts syncing immediately.

## structural sync alignment

signal-first is the architectural expression of the five structural sync layers:

```
layer 1 (validity):       each signal carries zheng proof — verified on receipt
layer 2 (ordering):       hash chain + VDF embedded in signal — self-ordering
layer 3 (completeness):   per-neuron PCS over signals — nothing withheld
layer 4 (availability):   DAS + erasure coding over signal log — data survives
layer 5 (merge):          CRDT locally, foculus globally — convergence guaranteed
```

the signal log is the canonical state. BBG_root is a derived commitment. signals flow from neuron to light client through five layers, each independently verifiable. a node does not trust that its state is correct — it verifies, locally, from the signal log.

see [[architecture-overview]] for the full pipeline, [[architecture]] for the specification, [[data-availability explained|data-availability]] for DAS mechanics
