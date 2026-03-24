---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-23
depends: [algebraic-nmt.md, signal-first.md, unified-polynomial-state.md, mutator-set-polynomial.md, temporal-polynomial.md, pi-weighted-replication.md, verifiable-query.md]
---
# full pipeline — from cyberlink to light client in one continuous fold

the endgame architecture. every proposal in bbg, zheng, hemera, and nox composes into one pipeline where computation, hashing, proving, and syncing are a single continuous fold. no separate proving phase. no separate sync protocol. no separate verification step. signals flow from neuron to light client through five structural sync layers, each optimized by its corresponding proposals.

## the pipeline

```
NEURON                          NETWORK                         CLIENT
──────                          ───────                         ──────
create cyberlinks               include in block                download checkpoint
  ↓                               ↓                               ↓
nox execution                   apply to BBG polynomial         verify accumulator
  ↓ proof-carrying                ↓ algebraic NMT                 ↓ one decider
hemera identity                 fold into block accumulator     sync namespaces
  ↓ folded sponge                 ↓ folding-first                 ↓ PCS openings
build signal                    fold into epoch accumulator     DAS sample
  ↓ (ν, l⃗, π_Δ, σ, prev...)     ↓ universal accumulator          ↓ algebraic DAS
local sync                      publish checkpoint              full VEC state
  ↓ CRDT + NMT + DAS              ↓ ~232 bytes                    ↓ < 10 KiB total
submit to network
  ↓ foculus π convergence
```

## stage 1: signal creation (neuron)

a neuron creates cyberlinks. computation, hashing, and proving happen in one pass.

### nox execution with proof-carrying

```
every reduce() call:
  1. compute result                                    (nox pattern dispatch)
  2. generate one trace row                            (trace buffer)
  3. fold trace row into running accumulator           (HyperNova, ~30 field ops)

at computation end:
  accumulator IS the proof (after one decider call)
  zero additional proving latency
```

source: [[proof-carrying]]

for binary workloads (quantized AI, tri-kernel SpMV):
```
nox<F₂> execution → Binius PCS → fold into F_p accumulator
boundary cost: ~766 F_p constraints per algebra crossing
binary jets: popcount, packed_inner_product, binary_matvec → 1,400× over F_p
```

source: [[binius-pcs]], [[binary-jets]]

### hemera identity with folded sponge

```
content addressing: H(cyberlink) = particle identity

current:  each hemera absorption block = independent permutation
          K blocks × 736 constraints = ~54K constraints for 4 KiB particle

folded:   fold each absorption into proof-carrying accumulator
          K blocks × 30 field ops + 1 decider = ~2,956 constraints

savings:  18× per hash operation
```

source: [[folded-sponge]]

### signal assembly

```
signal = {
  ν:    neuron_id
  l⃗:    [cyberlink]                    the batch
  π_Δ:  [(particle, F_p)]              impulse (focus shift)
  σ:    proof_carrying_accumulator     the proof (from step 1)
  prev: H(previous signal)            ordering (hash chain)
  mc:   H(causal DAG root)            ordering (Merkle clock)
  vdf:  VDF(prev, T_min)              ordering (physical time)
  step: u64                           ordering (logical clock)
}

signal size: ~1-5 KiB (proof + impulse + 160 bytes metadata)
creation cost: computation time + VDF time (T_min)
proof cost: ZERO additional (proof-carrying)
```

## stage 2: local sync (neuron ↔ neuron)

local neurons (same identity) sync without coordination. structural sync layers 1-5.

```
1. COMPARE  merkle_clock roots                       O(1), 32 bytes
   equal → done

2. EXCHANGE signal NMT roots                         O(1), 32 bytes
   (algebraic: polynomial commitment, 32 bytes)

3. REQUEST  missing step ranges with proofs           O(log n)
   current:   NMT completeness proof                 ~1 KiB per namespace
   algebraic: PCS opening                            ~200 bytes per namespace

4. DAS SAMPLE content chunks                          O(√n)
   current:   NMT inclusion proof per sample          ~1 KiB × 20 = 20 KiB
   algebraic: PCS opening per sample                  ~200B × 20 = 4 KiB

5. VERIFY each signal                                 10-50 μs
   σ contains proof-carrying accumulator
   one decider call per signal (or batch-verify block of signals)

6. CRDT MERGE                                         O(signals)
   deterministic total order from signal metadata
   replay in order → identical state

total bandwidth (catch up 1 hour, ~100 signals):
  current:    ~100 × 3 KiB + 20 KiB DAS = ~320 KiB
  algebraic:  ~100 × 2 KiB + 4 KiB DAS = ~204 KiB
```

## stage 3: network submission and foculus

signal flows from local sync to global consensus.

```
1. neuron submits signal to network
2. peers verify: σ is valid (10-50 μs)
3. foculus: π convergence over all submitted signals
   - each neuron's signals carry conviction weighted by focus
   - π* = stationary distribution of tri-kernel
   - convergence: 1-3 seconds to fixed point
4. block producer includes signals, assigns t (block height)
```

no ordering coordination. no leader for signal ordering. deterministic ordering from signal metadata (causal > VDF > hash tiebreak). foculus determines WEIGHTS, not ORDER.

## stage 4: block processing

the block processor updates BBG state for each included signal. every operation is a polynomial update, not a tree rehash.

### per-cyberlink state update

```
current (NMT):
  4.5 tree path updates × 32 depth × 736 constraints = ~106K constraints
  + LogUp cross-index: ~1.5K constraints
  total: ~107.5K constraints per cyberlink

algebraic NMT (Verkle phase):
  4.5 polynomial path updates × 32 depth × ~100 field ops = ~3.2K constraints
  + LogUp: 0 (structural consistency — same polynomial)
  total: ~3.2K constraints per cyberlink

improvement: 33×
```

source: [[algebraic-nmt]]

### per-cyberlink private update

```
current (SWBF + MMR):
  AOCL membership: O(log N) hemera hashes
  SWBF non-membership: 128 KB witness + O(log N) MMR
  total: ~40K constraints

polynomial mutator set:
  commitment polynomial: O(1) PCS opening
  nullifier polynomial: O(1) PCS opening
  total: ~5K constraints

improvement: 8×
```

source: [[mutator-set-polynomial]]

### per-block accumulation

```
block with 1000 cyberlinks:

  state updates:       1000 × 3.2K = ~3.2M constraints (was ~108M)
  private updates:     1000 × 5K = ~5M constraints (was ~40M)
  hemera identity:     batched-proving → 736 + O(1000) ≈ ~2K (was 1000 × 736 = 736K)
  fold per signal:     1000 × 30 field ops = 30K field ops (negligible)
  block decider:       1 × 70K constraints

  total: ~8.3M constraints (was ~148M)
  improvement: 18×

  hemera calls for state verification: 0 (was 144,000)
  hemera calls for identity only: batched → 736 + O(N)
```

source: [[algebraic-nmt]], [[mutator-set-polynomial]], [[batched-proving]], [[folding-first]]

### BBG commitment update

```
current:
  BBG_root = H(13 × 32-byte sub-roots) = H(416 bytes)
  13 independent structures, each updated separately

phase 1 (algebraic NMT):
  BBG_root = H(BBG_poly_commitment ‖ private_roots ‖ signals.root)
  9 NMTs → 1 polynomial commitment (32 bytes)

phase 3 (unified polynomial):
  BBG_root = PCS.commit(BBG_poly)
  one 32-byte commitment for ALL state
```

source: [[unified-polynomial-state]]

## stage 5: epoch finalization

```
epoch = 1000 blocks

current:
  1000 blocks × 70K recursive verify = 70M constraints

folding-first:
  1000 folds × 30 field ops = 30K field ops
  1 decider = 70K constraints
  total: ~100K constraints

improvement: 700×

universal accumulator:
  fold ALL proof obligations into one running accumulator
  not just signal validity — also state integrity, cross-index,
  availability commitments, VDF chain, merge correctness

  all five structural sync layers → one ~200 byte object
```

source: [[folding-first]], [[universal-accumulator]]

## stage 6: light client

```
join the network with no history:

1. DOWNLOAD checkpoint                               ~232 bytes
   BBG_poly_commitment:  32 bytes
   universal_acc:        ~200 bytes
   height:               8 bytes (u64)

2. VERIFY accumulator                                 10-50 μs
   one zheng decider call
   proves: ALL history from genesis is valid
   all five structural sync layers verified in one shot

3. SYNC namespaces of interest                        ~200 bytes each
   PCS opening per namespace (algebraic NMT)
   "all outgoing axons from particle P" = one polynomial opening

4. DAS SAMPLE                                         ~4 KiB
   20 PCS opening samples (algebraic DAS)
   99.9999% confidence data exists

5. MAINTAIN                                           ~30 field ops / block
   fold each new block into local accumulator

total join cost:
  bandwidth:     < 10 KiB
  computation:   10-50 μs verification + 20 field verifications
  storage:       ~232 bytes (checkpoint) + monitored namespaces
  trust:         ZERO (mathematics only)
```

## stage 7: query

```
arbitrary query with cryptographic proof:

verifiable query compiler:
  CozoDB query → relational algebra → CCS constraints → zheng proof

"top 100 particles by π":
  batch PCS opening + permutation argument + range check
  proof: ~5 KiB, verify: 10-50 μs

"all cyberlinks from neuron N after time t":
  temporal polynomial opening + batch decomposition
  proof: ~3 KiB, verify: 10-50 μs

gravity commitment:
  high-π queries (hot layer): ~1 KiB proof, ~10 μs
  low-π queries (cold layer): ~8 KiB proof, ~200 μs
  average (power-law): ~2 KiB proof, ~30 μs
```

source: [[verifiable-query]], [[gravity-commitment]], [[temporal-polynomial]]

## end-to-end numbers

```
                        current stack            full pipeline          improvement
──────────────────────  ─────────────            ─────────────          ───────────
SIGNAL CREATION
  proving latency:      separate phase           zero (proof-carrying)  ∞
  hemera per hash:      ~1,152 constraints       ~164 (folded sponge)   7×
  binary workloads:     32-64× F_p overhead      native (Binius)        32-64×

LOCAL SYNC
  completeness proof:   ~1 KiB (NMT)            ~200 bytes (PCS)       5×
  DAS verification:     ~20 KiB                  ~4 KiB                 5×
  signal verify:        ~1 ms                    10-50 μs               20-100×

BLOCK PROCESSING
  per cyberlink:        ~107.5K constraints      ~3.2K                  33×
  per block (1000):     ~148M constraints        ~8.3M                  18×
  hemera calls/block:   ~144,000                 0 (state) + batched    ∞ / 400×
  LogUp cost:           ~6M constraints          0 (structural)         ∞

EPOCH (1000 blocks)
  composition:          ~70M constraints         ~100K                  700×

LIGHT CLIENT
  join bandwidth:       full chain headers       < 10 KiB               1000×+
  join verification:    O(chain) replay          10-50 μs               ∞
  checkpoint size:      varies                   ~232 bytes             —
  DAS bandwidth:        ~20 KiB                  ~4 KiB                5×

QUERY
  namespace proof:      ~1 KiB (NMT)            ~200 bytes (PCS)       5×
  arbitrary query:      custom proof             compiler-generated      general
  hot query:            ~1 ms                    ~10 μs                 100×

STORAGE
  NMT internal nodes:   ~5 TB (9 indexes)       0                      ∞
  BBG_root:             416 bytes (13 roots)     32 bytes (1 poly)      13×
  accumulator:          N/A                      ~200 bytes             —
```

## the continuous fold

the deepest architectural property: the entire pipeline is one continuous fold operation.

```
fold #1:   nox reduce() step         → fold trace row into accumulator
fold #2:   hemera absorption block   → fold permutation into accumulator
fold #3:   signal creation           → accumulator IS the proof (σ)
fold #4:   signal into block         → fold σ into block accumulator
fold #5:   block into epoch          → fold block acc into epoch accumulator
fold #6:   epoch into chain          → fold epoch acc into universal accumulator

every operation is a fold. the accumulator grows by ~30 field ops per step.
the decider runs ONCE — at whatever level you want to verify.

run decider after fold #3: verify one signal
run decider after fold #4: verify one block
run decider after fold #6: verify all history
```

there is no "proving" as a separate activity. there is no "syncing" as a separate protocol. there is computation that carries its own proof, and verification that checks one accumulator.

the cost of adding one cyberlink to the permanent, verified, globally-available knowledge graph:

```
computation:  nox execution (application-dependent)
proof:        ~30 field ops per reduce() step (amortized into computation)
identity:     ~164 constraints (folded hemera)
state:        ~3.2K constraints (algebraic NMT polynomial update)
privacy:      ~5K constraints (polynomial mutator set)
ordering:     1 VDF computation (T_min wall-clock)
availability: erasure encoding of signal (O(n log² n) field ops)

total proving overhead beyond raw computation: ~8.5K constraints
(vs ~148K constraints in current architecture — 17× reduction)
```

## proposal dependency map

```
                    hemera                  nox                   zheng
                    ──────                  ───                   ─────
stage 1 (create):   folded-sponge          proof-carrying         folding-first
                    compact-output          binary-jets            binius-pcs
                    inversion-sbox          algebra-polymorphism   ring-aware-fhe

stage 2 (sync):                                                   (algebraic DAS)

stage 3 (submit):                                                 (foculus = layer 5)

stage 4 (block):    batched-proving                               algebraic-extraction
                                                                  brakedown-pcs

stage 5 (epoch):                                                  universal-accumulator

stage 6 (client):                                                 gravity-commitment

stage 7 (query):                                                  tensor-compression
                                                                  gpu-prover

                    bbg proposals
                    ─────────────
stage 1:            signal-first

stage 2:            (structural sync = CRDT + NMT + DAS)

stage 4:            algebraic-nmt
                    mutator-set-polynomial
                    unified-polynomial-state

stage 5:            (universal accumulator folds all 5 layers)

stage 6:            pi-weighted-replication (DAS parameters)

stage 7:            verifiable-query
                    temporal-polynomial
```

## what makes this a quantum jump

the current architecture has distinct systems: a VM (nox), a prover (zheng), a hash (hemera), an authenticated state layer (bbg), and sync protocols. they connect through interfaces.

the full pipeline dissolves the boundaries. there is one continuous fold that starts inside the VM and ends at the light client. the fold accumulator passes through every layer:

```
VM → hash → proof → signal → sync → block → epoch → checkpoint → client
     ↑                                                              ↓
     └──────────── one accumulator, ~30 ops per step ──────────────┘
```

the accumulator is the universal witness. it proves computation happened correctly (layer 1), in the right order (layer 2), completely (layer 3), on available data (layer 4), with deterministic merge (layer 5). five independent properties, one object, one verification.

this is not optimization of the current design. it is a different kind of system: a **self-proving knowledge graph** where every edge carries its own proof, every query is verified, every sync is complete, and the entire history compresses to 200 bytes.

see [[structural-sync]] for the theoretical foundation, [[sync]] for the protocol, [[zheng-2]] for the proof architecture, [[signal-first]] for the state model
