---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-18
---
# signal-first reconstruction

bbg state is a deterministic function of the signal log. signals are append-only. any node can reconstruct any historical BBG_root from signals alone. the entire L1-L3 state is a materialized view, not primary data.

## the observation

every bbg state transition is triggered by a signal. every signal is self-certifying (carries its own zheng proof). the signal log is append-only and content-addressed. therefore:

```
BBG_state(h) = fold(genesis, signals[0..h])

for any height h:
  replay signals[0..h] → deterministic BBG_root
  compare with claimed BBG_root → fraud detection
```

bbg state (13 sub-roots, NMT trees, mutator set) is derived data. the signal log is the source of truth.

## what this solves

the storage-proofs problem ([[storage-proofs]]) asks: "how do we prove data retention at L2-L4?" signal-first reconstruction answers: prove you have the signals, not the state.

```
current framing:
  prove L1 state is correct             → consensus
  prove L2 particle data is stored      → ??? (open problem)
  prove L3 content is retrievable       → DAS (partial)
  prove L4 archival snapshots exist     → ??? (open problem)

signal-first framing:
  prove signal log is complete           → NMT completeness per neuron
  prove signal log is available          → DAS over signal batches
  derive everything else                 → deterministic replay
```

L2, L3, L4 become caching tiers. if any tier loses data, replay from signals reconstructs it. the only irrecoverable loss is if signals themselves are lost — and signals are protected by DAS + neuron-level NMT completeness.

## reconstruction cost

```
full replay (genesis to height h):
  h signals × ~1-5 KiB each = O(h) data
  verification: one zheng proof per signal = O(h × 10-50 μs)
  at 1000 signals/block, 10^6 blocks: ~10^9 signals
  data: ~1-5 TB
  verify time: ~10-50 seconds (parallelizable)

incremental replay (from checkpoint):
  download checkpoint: (BBG_root, universal_acc, height)
  verify universal_acc: ONE zheng verification (10-50 μs)
  replay signals since checkpoint: O(Δh) data
```

with universal accumulator ([[universal-accumulator]]), a checkpoint is ~200 bytes. verification is O(1). incremental replay from the latest checkpoint is the common case.

## signal log as backup

```
node crash recovery:
  1. fetch latest checkpoint from any peer         (~200 bytes)
  2. verify checkpoint (one zheng verification)     (10-50 μs)
  3. fetch signals since checkpoint (DAS available) (O(Δh) data)
  4. replay signals → reconstruct full BBG state    (deterministic)

no special backup infrastructure. no archival nodes with privileged roles.
every peer with signals IS a backup.
```

## interaction with temporal decay

temporal decay prunes low-energy axons. the pruning is deterministic (same decay parameters → same pruning decisions). replaying signals reproduces the same decay events. no special handling needed — decay is part of the fold function.

```
fold(state, signal) = {
  apply signal effects (new cyberlinks, focus shifts)
  apply decay (deterministic from state.time and decay params)
  update all 13 sub-roots
  return new state
}
```

## what MUST be stored

```
irreducible minimum per node:
  signal log:     append-only, DAS-protected, NMT-complete
  latest checkpoint: ~200 bytes (universal accumulator)

everything else is derivable:
  NMT trees         → rebuild from signal log
  mutator set       → replay UTXO events from signals
  particle data     → extract from signal payloads
  axon aggregates   → recompute from individual cyberlinks
  focus/π values    → recompute tri-kernel from graph state
```

## open questions

1. **signal content binding**: signals carry cyberlinks with particle CIDs (from, to). the CID references content. if the content behind a CID is lost but the signal is preserved, reconstruction restores the graph structure (which particles are linked) but not the content itself. content availability remains a separate concern (DAS via files.root)
2. **replay parallelism**: signals from independent neurons can be replayed in parallel (no shared state until aggregation). what is the optimal parallelization strategy? per-neuron replay → merge?
3. **checkpoint frequency**: more frequent checkpoints = faster recovery but more storage. optimal checkpoint interval depends on signal rate and recovery time target
4. **genesis bootstrap**: first-time nodes must either replay from genesis or trust a checkpoint. trusting a checkpoint = trusting the universal accumulator (cryptographic, not social trust)

see [[storage-proofs]] for the problem this solves, [[universal-accumulator]] for checkpoint verification
