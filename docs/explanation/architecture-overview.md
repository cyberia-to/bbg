---
tags: article, cyber, cip
crystal-type: entity
crystal-domain: computer science
status: draft
---
# bbg architecture overview

## one polynomial, one root, one verification

[[BBG]] commits the entire [[cybergraph]] to a single polynomial:

$$\text{BBG\_root} = \text{Brakedown.commit}(\text{BBG\_poly}) \quad \text{(32 bytes)}$$

BBG_poly(index, key, t) has three dimensions: WHAT (12 evaluation dimensions), WHERE (namespace key), WHEN (block height). every query is a polynomial opening. every verification is O(1) field operations. a 240-byte checkpoint proves all history from genesis in 10–50 μs.

## the three laws

**law 1: bounded locality.** operation cost $\propto$ what it touches. a single [[cyberlinks|cyberlink]] at $10^{15}$ [[particles]] costs the same as at $10^3$. no global recomputation. ever.

**law 2: constant-cost verification.** one PCS opening (~200 bytes, 10–50 μs) verifies any claim. independent of graph size, history length, or computation complexity. a phone verifies what a datacenter computes.

**law 3: structural security.** guarantees from mathematical structure, not protocol correctness. polynomial binding prevents lying. [[Brakedown]] PCS is post-quantum. privacy from polynomial mutator set (see [[mutator-set|polynomial mutator set]]).

## the 12 dimensions

BBG_poly encodes 12 evaluation dimensions — 10 public, 2 private:

| # | dimension | what it stores |
|---|---|---|
| 0 | particles | energy, π*, axon fields |
| 1 | axons_out | outgoing edges by source |
| 2 | axons_in | incoming edges by target |
| 3 | neurons | focus, karma, stake |
| 4 | locations | spatial associations |
| 5 | coins | fungible token supplies |
| 6 | cards | non-fungible knowledge assets |
| 7 | files | content availability (DAS) |
| 8 | time | historical state snapshots |
| 9 | signals | finalized signal batches |
| 10 | commitments (private) | commitment polynomial A(x) |
| 11 | nullifiers (private) | nullifier polynomial N(x) |

cross-index consistency is structural: axons_out and axons_in are dimensions of the SAME polynomial. they cannot disagree. no LogUp needed.

## the pipeline

```
DEVICE                     NETWORK                    CLIENT
nox execution              polynomial state update    checkpoint (240 bytes)
  ↓ proof-carrying           ↓ ~3.2K constraints        ↓ one zheng decider
hemera identity            fold into accumulator      PCS namespace sync
  ↓ folded sponge             ↓ ~30 field ops            ↓ ~200 bytes each
signal                     epoch accumulator          algebraic DAS
  ↓ structural sync           ↓ universal accumulator    ↓ ~4 KiB
```

cost of one cyberlink in the permanent knowledge graph:

```
proof:           ~30 field ops per nox step (proof-carrying)
identity:        ~164 constraints (folded hemera sponge)
public state:    ~3,200 constraints (polynomial update)
private state:   ~5,000 constraints (polynomial mutator set)
total:           ~8,400 constraints
```

## signal-first

BBG_poly is derived data. the signal log is the source of truth:

$$\text{BBG\_poly}(t) = \text{fold}(\text{genesis}, \text{signals}[0..t])$$

crash recovery: download checkpoint (240 bytes) + replay signals since checkpoint. no separate storage proofs for STATE — prove signal availability, derive everything.

## π-weighted everything

$\pi^*$ from the [[tri-kernel]] is the master distribution:
- verification cost: high-π cheaper ([[gravity commitment]])
- storage replication: replicas $\propto \pi$
- DAS parameters: high-π needs fewer samples
- temporal decay: low-π links decay faster
- query routing: hot queries from low-degree polynomial

one distribution governs the entire stack. see [[universal law]] for why exponential allocation is optimal.

## key numbers

```
BBG_root:               32 bytes (one Brakedown commitment)
checkpoint:             ~240 bytes (root + accumulator + height)
verification:           10–50 μs (one zheng decider)
per-cyberlink:          ~8,400 constraints total
per-block (1000 tx):    ~8.3M constraints
epoch (1000 blocks):    ~100K constraints (HyperNova folding)
inclusion proof:        ~200 bytes (PCS opening)
DAS (20 samples):       ~4 KiB bandwidth, ~3K constraints
light client join:      < 10 KiB
hemera calls/block:     0 for state verification
```

see [[architecture]] for the full specification, [[state]] for transaction types, [[sync]] for the five-layer protocol, [[privacy]] for the polynomial mutator set, [[storage]] for ShardStore backends, [[query]] for verifiable queries, [[structural-sync]] for the theoretical foundation
