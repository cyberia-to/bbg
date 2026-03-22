---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.001618791052513895
heat: 0.0011471352009717693
focus: 0.0007686761802915088
gravity: 0
density: 0.65
---
# signal sync

the ordering and merge mechanisms that make [[sync]] work. this file specifies the internals of signal ordering fields. for the sync protocol itself, see [[sync]].

## signal structure

the [[signal]] is s = (ν, l⃗, π_Δ, σ, prev, mc, vdf, step, t). the ordering fields (prev, mc, vdf, step) are part of the signal — not a separate envelope. the same fields serve local device sync and global [[foculus]] consensus. device = neuron at different scales.

```
signal = {
  // payload — what the signal means
  ν:              neuron_id                   subject (signing neuron)
  l⃗:              [cyberlink]                 links (L⁺), each a 7-tuple (ν, p, q, τ, a, v, t)
  π_Δ:            [(particle, F_p)]*          impulse: sparse focus update
  σ:              zheng_proof                 recursive proof covering impulse + conviction

  // ordering — where the signal sits in causal and physical time
  device:         device_id                   which device within ν (local sync only)
  prev:           H(author's previous signal) per-neuron hash chain
  merkle_clock:   H(causal DAG root)          compact causal state
  vdf_proof:      VDF(prev, T)               physical time proof
  step:           u64                         monotonic logical clock

  // finalization
  t:              u64                         block height (assigned at network inclusion)

  hash:           H(all above)
}

lifecycle:
  created on device:    (ν, l⃗, π_Δ, σ, prev, mc, vdf, step)
  synced between peers: full signal
  submitted to network: full signal (ordering fields preserved)
  included in block:    network assigns t (block height)

signal size: ~1-5 KiB proof + impulse + 160 bytes ordering metadata
```

## VDF — physical time without clocks

a Verifiable Delay Function computes `f(x)` in sequential time T. verification is fast. the output proves real wall-clock time elapsed since x was known.

```
signal.vdf_proof = VDF(prev_signal_hash, T_min)

T_min: minimum sequential computation between signals
proves: "at least T_min wall-clock time elapsed since prev signal"

no NTP, no clock sync, no trusted timestamps
physical time embedded in the proof itself
```

### VDF properties

```
RATE LIMITING:
  each signal requires VDF(prev, T_min) computation
  flood of N signals requires N × T_min wall-clock time
  cannot be parallelized — VDF is inherently sequential

FORK COST:
  equivocation (two signals from same prev) requires
  computing VDF twice from the same starting point
  total VDF time doesn't match elapsed time → detectable

ORDERING EVIDENCE:
  VDF proofs create partial physical time ordering
  between causally independent signals
  supplements causal ordering with real-world time
```

VDF parameters are per-device configurable. a phone with limited compute sets a longer T_min than a workstation.

## Merkle clock

causal history represented as a Merkle DAG. each signal's merkle_clock is the root hash of all signals the device has seen.

```
COMPARISON:
  device A.merkle_clock == device B.merkle_clock?
  O(1) — single hash comparison
  equal → devices are in sync

DIVERGENCE:
  roots differ → walk Merkle DAG
  O(log n) to find first divergence point
  exchange only signals after divergence

MERGE:
  union of both DAGs
  new merkle_clock = H(merged DAG root)
  deterministic — both devices compute same result
```

Merkle clocks replace vector clocks. vector clocks grow O(devices). Merkle clocks are O(1) for comparison, O(log n) for sync.

## hash chain

each neuron chains its signals sequentially via the `prev` field.

```
neuron's chain: s1 ← s3 ← s5 ← s8
  prev(s3) = H(s1)
  prev(s5) = H(s3)

PROPERTIES:
  immutable: cannot insert, remove, or reorder past signals
  verifiable: any peer can walk the chain and verify continuity
  fork-evident: two signals with same prev = equivocation proof
```

at local scale, each device maintains its own chain within the neuron's signal set. at global scale, the per-neuron chain is the authoritative sequence.

## deterministic ordering

given a signal DAG, all participants compute the same total order.

```
1. CAUSAL ORDER preserved:
   if signal A is in signal B's deps → A before B

2. VDF ORDER for physically-timed signals:
   if A.vdf_time < B.vdf_time and not causally related → A before B

3. HASH TIEBREAK for remaining concurrent signals:
   if A and B concurrent and same VDF epoch → order by H(A) < H(B)

RESULT:
  deterministic total order
  every participant computes the SAME sequence
  no negotiation, no leader, no timestamps
```

this ordering is used at both scales:
- local: devices replay signals in this order → identical state
- global: foculus applies signals in this order → deterministic BBG_root

see [[sync]] for the protocol that uses these mechanisms, [[foculus-vs-crdt]] for merge layer comparison, [[architecture]] for BBG root structure
