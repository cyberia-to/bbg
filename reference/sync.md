---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0030146186555904744
heat: 0.0020766703217247637
focus: 0.0013733314853650736
gravity: 0
density: 1.13
---
# sync

one mechanism at three scales. a [[signal]] is the universal unit of state change — created on a device, merged between devices, submitted to the network, queried by clients. the signal structure is identical at every scale. only the merge function changes.

## signal lifecycle

```
creation          device creates cyberlinks → signal batch with zheng proof
                  signal = (ν, l⃗, π_Δ, σ, prev, mc, vdf, step)

ordering          hash chain (prev) establishes per-neuron sequence
                  VDF proves minimum wall-clock time since previous signal
                  Merkle clock captures full causal state
                  step counter provides monotonic logical clock

completeness      per-device NMT commits signal set — withholding detectable
                  per-neuron NMT at network scale — same guarantee

availability      DAS + 2D Reed-Solomon erasure coding
                  any k-of-n chunks reconstruct original
                  O(√n) sampling verifies availability

merge             LOCAL:  CRDT (G-Set union) — devices of same neuron
                  GLOBAL: foculus (π convergence) — neurons in network

finalization      network includes signal in block → assigns t (block height)
                  signal enters signals.root (MMR)
                  state transitions applied to BBG sub-roots

query             client requests namespace from BBG_root
                  NMT completeness proof (current) or PCS opening (algebraic NMT)
                  provably complete response
```

## three scales

| | local | global | query |
|---|---|---|---|
| **who** | neuron's devices (1-20) | network's neurons | light client ↔ peer |
| **direction** | bidirectional merge | neuron → network | pull (client reads) |
| **data** | private cyberlinks, files, names | signals (public aggregate) | BBG state (public) |
| **privacy** | private (individual records) | public (aggregate) | public (NMT/PCS proofs) |
| **trust** | same neuron, semi-trusted | different neurons, untrusted | peer untrusted, only BBG_root |
| **merge** | CRDT (G-Set union) | foculus (π convergence) | N/A (read-only) |
| **ordering** | VDF + hash chain + Merkle clock | VDF + hash chain + Merkle clock | block height (t) |
| **statefulness** | ongoing DAG | ongoing accumulator | stateless query-response |
| **signal structure** | identical | identical | N/A (queries BBG_root) |

the signal structure is scale-invariant. device = neuron at different scales. the five verification layers apply identically. only the merge function varies.

## signal structure

the [[signal]] is s = (ν, l⃗, π_Δ, σ, prev, mc, vdf, step, t). the ordering fields are part of the signal — not a separate envelope. the same fields serve local device sync and global [[foculus]] consensus.

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

## five verification layers

every signal passes five layers. all layers apply at both local and global scale.

```
layer           mechanism               guarantee
─────           ─────────               ─────────
1. validity     zheng proof per signal  invalid state transition → rejected
2. ordering     hash chain + VDF        reordering/flooding → structurally prevented
3. completeness NMT per source          withholding → structurally detectable
4. availability DAS + erasure coding    data loss → recoverable from k-of-n
5. merge        CRDT or foculus         convergence → deterministic final state
```

layers 1-4 are identical at all scales. layer 5 is the ONLY scale-dependent component.

### layer 1: validity

```
each signal carries σ = zheng proof

the proof covers:
  - cyberlinks well-formed (7-tuple structure)
  - focus sufficient for conviction weight
  - impulse π_Δ consistent with cyberlinks
  - neuron signature valid

invalid signal → proof verification fails → signal rejected by any peer
cost: 10-50 μs verification (zheng-2)
```

### layer 2: ordering

four mechanisms establish temporal structure without consensus.

**hash chain** — each neuron chains signals via the `prev` field:

```
neuron's chain: s1 ← s3 ← s5 ← s8
  prev(s3) = H(s1)
  prev(s5) = H(s3)

properties:
  immutable: cannot insert, remove, or reorder past signals
  verifiable: any peer can walk the chain and verify continuity
  fork-evident: two signals with same prev = cryptographic equivocation proof
```

at local scale, each device maintains its own chain. at global scale, the per-neuron chain is authoritative.

**VDF** — physical time without clocks:

```
signal.vdf_proof = VDF(prev_signal_hash, T_min)

T_min: minimum sequential computation between signals
proves: "at least T_min wall-clock time elapsed since prev signal"
no NTP, no clock sync, no trusted timestamps

rate limiting:  flood of N signals costs N × T_min sequential time
                cannot be parallelized — VDF is inherently sequential
fork cost:      equivocation requires computing VDF twice from same prev
                total VDF time doesn't match elapsed time → detectable
ordering:       VDF proofs create partial physical time ordering
                between causally independent signals
```

VDF parameters are per-device configurable. a phone sets longer T_min than a workstation.

**Merkle clock** — causal history as Merkle DAG:

```
each signal's merkle_clock = H(root of all signals the device has seen)

comparison:   O(1) — single hash comparison (equal = in sync)
divergence:   O(log n) — walk DAG to find first difference
merge:        union of both DAGs → H(merged root) — deterministic
```

replaces vector clocks (O(devices)) with O(1) comparison.

**step** — monotonic logical clock:

```
gap-free counter per source
gap in step sequence = missing signal = detectable
```

**deterministic total order** — given a signal DAG, all participants compute the same sequence:

```
1. causal order:   A in B's deps → A before B
2. VDF order:      A.vdf_time < B.vdf_time (if not causally related) → A before B
3. hash tiebreak:  concurrent signals same VDF epoch → H(A) < H(B) → A before B

no negotiation, no leader, no timestamps
```

### layer 3: completeness

```
each source (device at local scale, neuron at global scale) commits its
signal chain to an NMT namespaced by step.

  NMT[ step → signal_hash ]

proves: "these are ALL signals from source S in step range [a, b]"
NMT completeness: source cannot hide a signal in the requested range
the tree's sorting invariant structurally prevents omission

updated: on every new signal (append to NMT, update root)
cost: O(log n) per signal
```

### layer 4: availability

```
2D Reed-Solomon erasure coding over Goldilocks field.

  original data: √n × √n grid
  extended: 2×(√n × √n) with parity rows and columns
  any √n × √n submatrix sufficient for reconstruction

sampling: O(√n) random cells with NMT inclusion proofs
  99.9% confidence at ~20 samples
  verifies availability without downloading full data

fraud proofs: bad encoding detectable with k+1 cells
  decoded polynomial degree mismatch → fraud proof generated
```

### layer 5: merge

the only scale-dependent layer.

**local (CRDT):**
```
mechanism: G-Set union (grow-only set of signals)
  commutative: A ∪ B = B ∪ A
  associative: (A ∪ B) ∪ C = A ∪ (B ∪ C)
  idempotent: A ∪ A = A

works because:
  - single neuron — no adversarial stake to manipulate
  - content-addressed signals — no semantic conflicts
  - deterministic ordering from hash chain + VDF + hash tiebreak

conflict resolution for mutable state (names):
  concurrent signals ordered by:
    1. causal order (A in B's deps → A before B)
    2. VDF time (lower VDF → earlier)
    3. hash tiebreak (H(A) < H(B) → A before B)
  → deterministic total order → replay produces identical state
```

**global (foculus):**
```
mechanism: π convergence (stake-weighted attention)
  each neuron's signals carry conviction weighted by focus
  π* = stationary distribution of tri-kernel (diffusion + springs + heat)
  convergence: π stabilizes to fixed point in 1-3 seconds

works because:
  - π manipulation costs real tokens (focus)
  - exclusive support: staking on A removes support from B
  - convergence is mathematical (eigenvalue), not political (voting)

the same signal at global scale means:
  all signals from all neurons are valid (layer 1)
  all signals are ordered (layer 2)
  no neuron can withhold (layer 3)
  data is available (layer 4)
  global state = π-weighted convergence of all signals (layer 5)
```

## steps

a step is a logical clock tick. steps advance on cyberlink production and heartbeat intervals.

```
cyberlink step:
  device creates cyberlinks → step increments
  signal contains the cyberlink batch + zheng proof

heartbeat step:
  no cyberlinks within heartbeat interval
  device emits empty signal (liveness attestation)
  contains: device_id, capacity, VDF proof, merkle_clock, step

snapshot step:
  every K steps (configurable, e.g. K=100)
  signal contains BBG_root snapshot at this step
  enables fast sync (skip-to-snapshot, avoid full replay)

step properties:
  per-source monotonic, gap-free
  does not tick without either cyberlinks or heartbeat
  heartbeat interval: device-configurable (e.g. 60s)
```

## local sync protocol

two devices of the same neuron reconnect.

```
1. COMPARE merkle_clock roots                    O(1)
   equal → done (already in sync)
   different → continue

2. EXCHANGE signal NMT roots                     O(1)
   each device sends its current signal NMT root

3. REQUEST missing step ranges                   O(log n)
   with NMT completeness proofs
   → provably ALL signals in range received
   → no withholding possible

4. DAS SAMPLE content chunks                     O(√n)
   verify content availability
   request missing chunks by CID

5. VERIFY each received signal:
   a) zheng proof valid?                         layer 1: validity
   b) hash chain intact? (prev links)            layer 2: ordering
   c) no equivocation? (no duplicate prev)       layer 2: ordering
   d) VDF proof valid?                           layer 2: ordering
   e) step counter monotonic?                    layer 2: ordering
   f) NMT inclusion proof valid?                 layer 3: completeness

6. MERGE signal DAGs                             O(signals)
   compute deterministic total order (CRDT)

7. REPLAY ordered signals                        O(signals)
   apply state transitions → identical state

FAST SYNC (snapshot available):
   find most recent snapshot step in common
   replay only signals after snapshot
   → O(signals since snapshot) instead of O(all signals)
```

## global sync: signal submission

signals flow from local device sync to the network.

```
1. neuron creates signals on devices
2. devices sync locally (protocol above)
3. neuron submits finalized signals to network
4. network verifies (layers 1-4)
5. foculus merges (layer 5): π-weighted convergence
6. block producer includes signals → assigns t (block height)
7. state transitions applied to BBG sub-roots
8. BBG_root updated → enters time.root for historical queries
```

## query sync protocol

light clients and full nodes query BBG state. read-only — no signals produced.

### namespace queries

five query types, each backed by NMT completeness proof.

**outgoing axons from particle P:**
```
client → peer: (axons_out, namespace=P, state_root=BBG_root)
peer → client: NMT completeness proof + all axon entries in namespace P
client verifies: proof valid, all entries in namespace, nothing withheld
guarantee: "ALL outgoing axons from P. nothing hidden."
```

**incoming axons to particle Q:**
```
client → peer: (axons_in, namespace=Q, state_root=BBG_root)
peer → client: NMT completeness proof + all axon entries in namespace Q
guarantee: "ALL incoming axons to Q. nothing hidden."
```

**neuron public state:**
```
client → peer: (neurons, namespace=N, state_root=BBG_root)
peer → client: NMT proof + neuron data (focus, karma, stake)
guarantee: "neuron N's complete public state."
```

**particle data:**
```
client → peer: (particles, namespace=P, state_root=BBG_root)
peer → client: NMT proof + particle data (energy, π*, axon fields)
guarantee: "particle P's complete public data."
```

**state at time T:**
```
client → peer: (time, namespace=<unit>, key=T, state_root=BBG_root)
peer → client: NMT proof + BBG_root snapshot at time T
guarantee: "authenticated state root at time T."
client can then query any namespace against the historical root.
```

### incremental sync

```
client has state at height h₁, wants updates through h₂:

1. REQUEST time range [h₁, h₂] with BBG_root at h₂
2. RESPONSE: time.root entries between h₁ and h₂
   + per monitored namespace: diff of added/removed/updated entries
   + updated NMT proofs at height h₂
3. VERIFY: time.root NMT completeness + namespace diffs against updated roots

data transferred: O(|changes since h₁|) — NOT O(|total state|)
```

## light client protocol

```
new client joins with no history:

1. OBTAIN checkpoint = (BBG_root, folding_acc, height) from any peer

2. VERIFY folding_acc:
   final_proof = decide(folding_acc)        zheng-2 decider
   verify(final_proof, BBG_root)
   → ONE verification proves ALL history from genesis valid
   → cost: 10-50 μs

3. SYNC namespaces of interest (any query type above)

4. MAINTAIN:
   - fold each new block into local folding_acc (~30 field ops)
   - update mutator set proofs for owned private records
   - update NMT proofs for monitored namespaces

join cost:     ONE zheng verification + namespace sync
ongoing cost:  O(log N) per block
storage:       O(|monitored_namespaces| + |owned_records| × log N)
```

trust: only BBG_root (from consensus). peer is completely untrusted.

## content sync

file blobs are content-addressed [[particles]]. three layers compose.

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

```
file → content-defined chunks → each chunk is a particle
file_CID = H(chunk_CIDs)
chunk size: configurable (default 256 KiB)

erasure coding:
  k data chunks + (n-k) parity chunks
  any k-of-n reconstructs original
  distributed across N devices

sampling:
  device commits content to per-device NMT (namespace = CID)
  verifier samples O(√n) random chunk positions
  each sample: chunk + NMT inclusion proof
  99.9% confidence at O(√n) samples
```

### NMT completeness

```
device commits content set to NMT:
  content_nmt.root = NMT(all CIDs held)

peer requests namespace proof:
  "give me ALL CIDs in range [X, Y] with proof"

NMT completeness: device cannot hide a CID in range
structural guarantee — tree cannot lie
```

### three layers composed

```
layer           mechanism       guarantees
─────           ─────────       ──────────
merge           CRDT (G-Set)    convergence, commutative, idempotent
completeness    NMT proof       provable completeness, withholding impossible
availability    DAS + erasure   data survives device failure, O(√n) verification

CRDT alone:  converges on possibly incomplete data
DAS alone:   proves availability, no merge semantics
NMT alone:   proves completeness, no availability
together:    provably complete, provably available, correctly merged
```

## name resolution

name bindings (cards) are mutable state. concurrent updates resolved deterministically.

```
device A offline: ~/paper.md → CID_v2  (signal sA)
device B offline: ~/paper.md → CID_v3  (signal sB)

sA and sB are concurrent (neither in other's deps)

resolution:
  H(sA) < H(sB) → sA ordered first, sB second
  replay: paper.md = CID_v2, then paper.md = CID_v3
  result: paper.md → CID_v3

both devices agree. both versions exist as particles. full history in AOCL.
```

## fault handling

```
fault class          mechanism               guarantee
═════════════        ═════════               ═════════

FORGING              zheng proof per signal  proof fails → rejected
(invalid operation)                          cost: 0 (cannot forge valid proof)

EQUIVOCATION         hash chain + VDF        two signals same prev →
(double-signaling)                           cryptographic misbehavior proof
                                             VDF: total time doesn't add up
                                             cost: O(1) detection

REORDERING           hash chain              prev hashes break →
(changing history)                           detectable by any peer
                                             cost: O(1) detection

WITHHOLDING          NMT + DAS               NMT completeness proof →
(hiding signals)                             withheld signals detectable
                                             DAS: availability verifiable
                                             cost: O(√n) sampling

FLOODING             VDF rate limiting       each signal costs T_min wall time
(signal spam)                                inherently sequential
                                             cost: T_min × signal_count

COMPROMISED DEVICE   key revocation signal  neuron revokes device key →
                                             future signals rejected →
                                             past signals remain (AOCL immutable)

STALE DEVICE         snapshot + fast sync    reconnect → find common snapshot →
(long offline)                               replay from snapshot
                                             VDF on received signals verifies time
```

## algebraic NMT impact

when algebraic NMT ([[algebraic-nmt]]) replaces NMT trees with polynomial commitments, the query sync protocol changes:

```
current (NMT):
  query: (namespace, key, BBG_root)
  response: NMT completeness proof + entries
  proof size: O(log n) × 32 bytes ≈ 1 KiB per namespace
  verify: walk NMT path, check sorting invariant

algebraic NMT:
  query: (namespace, key, BBG_commitment)
  response: PCS opening proof + entries
  proof size: O(1) ≈ 200 bytes per namespace
  verify: one PCS verification, 10-50 μs

the five verification layers remain identical.
layer 3 (completeness) shifts from structural (NMT sorting) to algebraic (PCS binding).
layers 1, 2, 4, 5 are unaffected.
```

incremental sync also changes:

```
current: exchange NMT diffs, recompute paths
algebraic: exchange polynomial update deltas, verify batch opening
cost: O(|changes|) field ops instead of O(|changes| × log n) hemera hashes
```

the light client protocol benefits most:

```
current: ONE zheng verify + NMT namespace sync (~1 KiB per namespace)
algebraic: ONE zheng verify + PCS namespace sync (~200 bytes per namespace)

with unified polynomial state ([[unified-polynomial-state]]):
  BBG_root = single PCS commitment (32 bytes)
  every query = one PCS opening
  cross-index consistency = structural (same polynomial)
```

see [[architecture]] for BBG root structure, [[signal-sync explained|signal-sync]] for ordering design rationale, [[data-availability explained|data-availability]] for DAS details, [[foculus-vs-crdt]] for merge layer comparison, [[algebraic-nmt]] for polynomial transition
