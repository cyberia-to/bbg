---
tags: article, cyber, cip
crystal-type: entity
crystal-domain: computer science
status: draft
---
# bbg architecture overview

## one root

BBG commits the entire cybergraph to three polynomial commitments:

```
BBG_root = H(commit(BBG_poly) ‖ commit(A) ‖ commit(N))    32 bytes
```

- **BBG_poly**: public state — 10 evaluation dimensions, all queryable
- **A(x)**: commitment polynomial — every private record ever created
- **N(x)**: nullifier polynomial — every spent record

one hash, three commitments, 32 bytes. a 240-byte checkpoint (root + universal accumulator + height) proves all history from genesis in ~5 microseconds.

## 10 public dimensions + 2 private commitments

BBG_poly(dimension, key, t) encodes public state across three axes: WHAT (dimension), WHERE (key), WHEN (block height).

| # | dimension | what it stores |
|---|-----------|----------------|
| 0 | particles | energy, pi-star, axon fields |
| 1 | axons_out | outgoing edges by source |
| 2 | axons_in | incoming edges by target |
| 3 | neurons | focus, karma, stake |
| 4 | locations | spatial associations |
| 5 | coins | fungible token supplies |
| 6 | cards | non-fungible knowledge assets |
| 7 | files | content availability (DAS) |
| 8 | time | historical state snapshots |
| 9 | signals | finalized signal batches |

cross-index consistency is structural: axons_out and axons_in are dimensions of the SAME polynomial. they cannot disagree. no LogUp needed.

private state:

| commitment | what it stores |
|------------|----------------|
| A(x) | membership — every committed private record (cyberlinks, UTXOs) |
| N(x) | non-membership — every spent record (nullifiers) |

opening A is zero-knowledge. opening N is zero-knowledge. the privacy boundary is enforced by PCS properties — see [[polynomial-privacy]].

## the 7-stage pipeline

```
NEURON                          NETWORK                         CLIENT
──────                          ───────                         ──────
1. create cyberlinks            4. apply to BBG polynomial      6. download checkpoint
   ↓ nox execution                ↓ ~3.2K constraints/link        ↓ ~240 bytes
   ↓ proof-carrying               ↓                               ↓
2. hemera identity              5. fold into epoch accumulator  7. sync + verify
   ↓ folded sponge                ↓ universal accumulator          ↓ PCS openings
   ↓                               ↓ ~200 byte checkpoint          ↓ algebraic DAS
3. build + sync signal
   ↓ structural sync (5 layers)
   ↓ foculus pi convergence
```

**stage 1: nox execution.** every reduce() call computes the result and folds one trace row into a running proof accumulator (~30 field ops per step). at computation end, the accumulator IS the proof. zero separate proving latency.

**stage 2: hemera identity.** hemera wraps PCS commitments with domain tags: particle_id = hemera(PCS.commit(content) ‖ PARTICLE). ~3 hemera calls per execution: one for noun identity (domain separation), one for Fiat-Shamir seed, one internal to Brakedown binding. the heavy work is polynomial commitment; hemera is the thin trust layer. ~164 constraints per hash (folded sponge), down from ~736 (independent permutation).

**stage 3: signal creation + sync.** the signal bundles cyberlinks, focus shifts (pi_delta), and the proof accumulator (sigma). local neurons sync via structural sync: Merkle clock comparison (32 bytes), signal exchange, CRDT merge. then submit to network via foculus pi convergence.

**stage 4: block processing.** the block processor applies each signal to BBG_poly. per cyberlink: ~3,200 constraints (polynomial update) + ~5,000 constraints (polynomial mutator set for private state). hemera calls for state verification: zero. LogUp: zero.

**stage 5: epoch finalization.** 1000 blocks fold into one epoch accumulator via HyperNova folding (~30 field ops per fold + one decider = ~100K constraints total). the universal accumulator covers all five structural sync layers in one ~200 byte object.

**stage 6: light client join.** download checkpoint (~240 bytes). verify accumulator (~5 microseconds). one proof verifies ALL history from genesis. no sync committee, no header chain, no trust assumption beyond mathematics.

**stage 7: query + maintain.** sync namespaces of interest via PCS openings (~200 bytes each). DAS sample for availability (~1.5 KiB batch opening for 20 samples, 99.9999% confidence). fold each new block into local accumulator (~30 field ops/block). total join cost: < 10 KiB.

## signal-first

BBG_poly is derived data. the signal log is the source of truth:

```
BBG_state(h) = fold(genesis, signals[0..h])
```

crash recovery: download checkpoint (240 bytes) + replay signals since checkpoint. no separate storage proofs for state — prove signal availability, derive everything. see [[why-signal-first]].

## pi-weighted everything

pi-star from the tri-kernel is the master distribution:

- **verification cost**: high-pi queries from low-degree polynomial (gravity commitment)
- **storage replication**: replicas proportional to pi
- **DAS parameters**: high-pi data needs fewer samples
- **temporal decay**: low-pi links decay faster
- **query routing**: hot queries from hot-layer polynomial

one distribution governs the entire stack.

## key numbers

```
BBG_root:                32 bytes (one hash of three commitments)
checkpoint:              ~240 bytes (root + accumulator + height)
verification:            ~5 μs (one zheng decider, recursive Brakedown)
per-cyberlink:           ~8,400 constraints total
  public state update:   ~3,200 constraints (polynomial)
  private state update:  ~5,000 constraints (polynomial mutator set)
  identity:              ~164 constraints (folded hemera sponge)
per-block (1000 tx):     ~8.3M constraints
epoch (1000 blocks):     ~100K constraints (HyperNova folding)
inclusion proof:         ~200 bytes (PCS opening)
DAS (20 samples):        ~1.5 KiB bandwidth (batch opening), ~3K constraints
light client join:       < 10 KiB total
hemera calls/execution:  ~3 (domain separation + Fiat-Shamir + PCS binding)
hemera calls/block:      0 for state verification
NMT internal nodes:      0 (polynomial replaces tree structure)
```

see [[architecture]] for the full specification, [[state]] for transaction types, [[sync]] for the five-layer protocol, [[polynomial-privacy]] for the private state model, [[why-polynomial-state]] for why polynomial over hash tree, [[why-signal-first]] for the state derivation model
