---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# signal sync

private synchronization of [[cyberlinks]] and file blobs across a neuron's devices. signals are the unit of sync — self-certifying batches of operations ordered by a Merkle clock DAG. no leader, no quorum, no timestamps. Byzantine faults eliminated structurally: [[zheng]] proofs prevent forging, hash chains prevent reordering, equivocation is O(1) detectable.

## the problem

a [[neuron]] operates from multiple devices (1-20) with intermittent connectivity. each device creates [[cyberlinks]] and stores file blobs. all devices must converge to identical state. the sync protocol must work with any subset of devices online, including a single device operating alone.

## signal structure

the [[signal]] is s = (ν, l⃗, π_Δ, σ, prev, mc, vdf, step, t). the ordering fields (prev, mc, vdf, step) are part of the signal — not a separate envelope. the same fields serve local device sync and global [[foculus]] consensus. device = neuron at different scales.

```
signal = {
  // payload — what the signal means
  ν:              neuron_id                   subject (signing neuron)
  l⃗:              [cyberlink]                 links (L⁺), each a 7-tuple (ν, p, q, τ, a, v, t)
  π_Δ:            [(particle, F_p)]*          impulse: sparse focus update, how the batch shifts π*
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

## steps

a step is a logical clock tick. steps advance when cyberlinks are produced and on heartbeat intervals.

```
STEP ADVANCEMENT:

  cyberlink step:
    device creates cyberlinks → step increments
    signal contains the cyberlink batch
    step holds: batch, proof, causal state

  heartbeat step:
    no cyberlinks produced within heartbeat interval
    device emits empty signal (no batch, proof of liveness)
    step holds: device capacity, liveness attestation

  snapshot step:
    every K steps (configurable, e.g. K=100)
    signal contains: BBG_root snapshot at this step
    enables skip-to-snapshot sync (avoid full replay)

STEP PROPERTIES:
  - steps do not tick without either cyberlinks or heartbeat
  - heartbeat interval: device-configurable (e.g. 60s)
  - heartbeat signals carry: device_id, capacity metrics, VDF proof
  - snapshot signals carry: full state hash, enabling fast bootstrap
  - step counter is per-device, monotonic, gap-free
```

## VDF — physical time without clocks

a Verifiable Delay Function computes `f(x)` in sequential time T. verification is fast. the output proves real wall-clock time elapsed since x was known.

```
VDF INTEGRATION:

  signal.vdf_proof = VDF(prev_signal_hash, T_min)

  T_min: minimum sequential computation between signals
  proves: "at least T_min wall-clock time elapsed since prev signal"

  no NTP, no clock sync, no trusted timestamps
  physical time embedded in the proof itself
```

### why VDF for personal sync

agents are Byzantine even on your own device. an LLM agent, a compromised process, or a rogue script can create signals at machine speed — flooding the DAG, exhausting focus, or racing legitimate user operations. VDF provides:

```
RATE LIMITING:
  each signal requires VDF(prev, T_min) computation
  T_min = minimum wall-clock delay between signals
  a flood of 1000 signals requires 1000 × T_min wall-clock time
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

VDF parameters are per-device configurable. a phone with limited compute sets a longer T_min than a workstation. the heartbeat interval accounts for VDF computation time.

## Merkle clock

causal history represented as a Merkle DAG. each signal's merkle_clock is the root hash of all signals the device has seen.

```
MERKLE CLOCK OPERATIONS:

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

## hash chain — device-local history

each device chains its own signals sequentially via the `prev` field.

```
device A:  s1 ← s3 ← s5 ← s8
           prev(s3) = H(s1)
           prev(s5) = H(s3)

PROPERTIES:
  - immutable: cannot insert, remove, or reorder past signals
  - verifiable: any peer can walk the chain and verify continuity
  - fork-evident: two signals with same prev = equivocation proof
```

## Byzantine fault elimination

```
FAULT CLASS          MECHANISM               STRUCTURAL GUARANTEE
═════════════        ═════════               ════════════════════

FORGING              zheng proof per signal  invalid state transition →
(invalid operation)                          proof verification fails →
                                             signal rejected by any peer
                                             cost: 0 (cannot produce valid proof)

EQUIVOCATION         hash chain + VDF        two signals with same prev →
(double-signaling)                           cryptographic proof of fault
                                             VDF: total time doesn't add up
                                             cost: O(1) detection

REORDERING           hash chain              reordering breaks chain →
(changing history)                           prev hashes don't match →
                                             detectable by any peer
                                             cost: O(1) detection

WITHHOLDING          NMT + DAS               NMT completeness proof per device:
(refusing to share)                          device commits signal set to NMT
                                             peer requests namespace proof →
                                             withheld signals detectable
                                             DAS: content availability verifiable
                                             cost: O(√n) sampling to detect

FLOODING             VDF rate limiting       each signal costs T_min wall time
(signal spam)                                cannot be parallelized
                                             physical bound on throughput
                                             cost: T_min × signal_count
```

no BFT protocol, no leader election, no quorum, no voting rounds. structural guarantees replace protocol guarantees. the third law of [[bbg]] applied to sync.

## deterministic ordering

given a signal DAG, all devices compute the same total order.

```
ORDERING RULES:

  1. CAUSAL ORDER preserved:
     if signal A is in signal B's deps → A before B

  2. VDF ORDER for physically-timed signals:
     if A.vdf_time < B.vdf_time and not causally related → A before B

  3. HASH TIEBREAK for remaining concurrent signals:
     if A and B concurrent and same VDF epoch → order by H(A) < H(B)

  RESULT:
    deterministic total order
    every device computes the SAME sequence
    no negotiation, no leader, no timestamps
```

## signal NMT — per-device signal commitment

each device commits its signal chain to a per-device NMT, namespaced by step.

```
SIGNAL NMT:

  structure: NMT[ step → signal_hash ]
  namespace: step counter (u64)
  root: signed by device key

  proves: "these are ALL signals from device D in step range [a, b]"
  NMT completeness: device cannot hide a signal in the requested range

  updated: on every new signal (append to NMT, recompute root)
  cost: O(log n) per signal (NMT path update)
```

this transforms withholding from self-punishing to detectable. a peer requests a namespace proof for the step range it's missing. the NMT either provides all signals in that range or the proof fails. structural guarantee — the tree cannot lie.

## sync protocol

```
SYNC (two devices reconnect):

  1. COMPARE merkle_clock roots               O(1)
     equal → done (already in sync)
     different → continue

  2. EXCHANGE signal NMT roots                 O(1)
     each device sends its current signal NMT root

  3. REQUEST missing step ranges               O(log n)
     with NMT completeness proofs
     → provably ALL signals in range received
     → no withholding possible

  4. DAS SAMPLE content chunks                 O(√n)
     verify content availability
     request missing chunks by CID

  5. VERIFY each received signal:
     a) zheng proof valid?                     forging check
     b) hash chain intact? (prev links)        reordering check
     c) no equivocation? (no duplicate prev)   double-signal check
     d) VDF proof valid?                       rate/time check
     e) step counter monotonic?                gap check
     f) NMT inclusion proof valid?             completeness check

  6. MERGE signal DAGs                         O(signals)
     compute deterministic total order

  7. REPLAY ordered signals                    O(signals)
     apply state transitions
     → identical state on all devices

  FAST SYNC (snapshot available):
     find most recent snapshot step in common
     replay only signals after snapshot
     → O(signals since snapshot) instead of O(all signals)
```

## content sync

file blobs are content-addressed [[particles]]. content is chunked, erasure-coded, and committed to an NMT. three layers compose: CRDT for merge, DAS for completeness, NMT for provability.

```
CONTENT LAYER:

  file → content-defined chunks → each chunk is a particle
  file_CID = H(chunk_CID_1 ‖ chunk_CID_2 ‖ ... ‖ chunk_CID_n)
  chunk size: configurable (default 256 KiB, content-defined boundaries)
```

### CRDT merge (grow-only set)

```
  sync: exchange missing CIDs
  "do you have CID X?" → yes (skip) / no (transfer)

  no ordering needed for content
  no conflicts possible (content-addressed)
  deduplication automatic
  verification: H(received) == CID
```

### DAS — data availability sampling

content chunks are erasure-coded across devices using 2D Reed-Solomon over [[Goldilocks field]].

```
ERASURE CODING:

  file chunks → k data chunks + (n-k) parity chunks
  any k-of-n chunks reconstruct the original
  distributed across N devices in the sync group

  device goes offline permanently?
    any k surviving devices → full recovery
    no single device is a point of failure

DAS VERIFICATION:

  each device commits its content to a per-device NMT
  (namespace = CID, sorted by content hash)

  verifier samples O(√n) random chunk positions
  each sample: request chunk + NMT inclusion proof
  if all samples verify → data is available with high probability
  99.9% confidence at O(√n) samples

  cost: O(√n) samples vs O(n) full download
  a phone verifies availability without downloading everything
```

### NMT completeness — provable sync

```
COMPLETENESS PROOF:

  device A commits content set to NMT:
    content_nmt.root = NMT(all CIDs held by device A)

  device B requests namespace proof:
    "give me ALL CIDs in range [X, Y] with proof"

  NMT completeness guarantee:
    device A cannot hide a CID in the requested range
    the tree structure prevents omission
    device B KNOWS it has everything

  CRDT alone:  "I merged what I received" (was everything sent?)
  NMT + DAS:   "I have everything, provably" (structural guarantee)
```

### three layers composed

```
LAYER           MECHANISM       GUARANTEES
─────           ─────────       ──────────
merge           CRDT (G-Set)    convergence on received data
                                commutative, associative, idempotent

completeness    NMT proof       provable completeness per namespace
                                withholding structurally impossible
                                O(log n) proof size

availability    DAS + erasure   data survives device failure
                                O(√n) verification cost
                                no single point of failure

CRDT alone: converges, but on possibly incomplete data
DAS alone: proves availability, but no merge semantics
NMT alone: proves completeness, but no availability
together: provably complete, provably available, correctly merged
```

## name resolution sync

name bindings (cards) are mutable state modified by signals.

```
CONCURRENT NAME UPDATE:

  device A offline: ~/paper.md → CID_v2  (signal sA)
  device B offline: ~/paper.md → CID_v3  (signal sB)

  sA and sB are concurrent (neither in other's deps)

  deterministic resolution:
    H(sA) < H(sB) → sA ordered first, sB second
    replay: paper.md = CID_v2, then paper.md = CID_v3
    result: paper.md → CID_v3

    both devices agree
    both versions exist as particles (content never lost)
    full history in AOCL
```

## heartbeat and liveness

```
HEARTBEAT SIGNAL:

  emitted when no cyberlinks produced within heartbeat interval
  contains:
    device_id:       identity
    capacity:        available compute, storage, bandwidth
    vdf_proof:       proves device is alive and computing
    merkle_clock:    current causal state
    step:            current step counter

  uses:
    - device liveness detection (peer sees heartbeats stop → offline)
    - capacity discovery (which device has most resources)
    - VDF chain continuity (no gaps in physical time proof)
    - sync state advertisement (merkle_clock enables O(1) sync check)
```

## fault handling

```
EQUIVOCATION DETECTED:
  two signals from same (author, prev)
  → cryptographic proof of misbehavior
  → device key revoked by neuron
  → future signals from that device rejected
  → past signals remain in history (AOCL immutable)

COMPROMISED DEVICE:
  signals are valid (proofs check out) but unwanted
  neuron revokes device key
  remaining devices sync without compromised device
  revocation is a signal (signed by neuron master key)

STALE DEVICE (long offline):
  reconnects with old merkle_clock
  fast sync: find common snapshot → replay from there
  if no common snapshot: full replay from genesis
  VDF proofs on received signals verify time continuity
```

see [[sync]] for public namespace sync, [[privacy]] for mutator set, [[storage]] for fjall layout, [[state]] for transaction types
