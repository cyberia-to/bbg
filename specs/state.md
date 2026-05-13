---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# state

all authenticated state committed under a single polynomial commitment. individual [[cyberlinks]] are private (polynomial mutator set). the public state contains only aggregates.

## BBG root

```
BBG_root = H(Lens.commit(BBG_poly) ‖ Lens.commit(A) ‖ Lens.commit(N))

three 32-byte Brakedown commitments hashed together → one 32-byte root

BBG_poly(index, key, t) — multivariate polynomial over Goldilocks field
10 public evaluation dimensions:
  particles     all particles: content + axons
  axons_out     directional index by source
  axons_in      directional index by target
  neurons       focus, karma, stake per neuron
  locations     proof of location
  coins         fungible token denominations
  cards         names and knowledge assets
  files         content availability (DAS)
  time          temporal snapshots (continuous, replaces 7-namespace NMT)
  signals       finalized signal batches

2 independent private polynomial commitments (NOT dimensions of BBG_poly):
  A(x)          commitment polynomial — private record commitments
  N(x)          nullifier polynomial — spent record tracking
```

## state diagram

```
                          ┌────────────────────────────────────────────────────┐
                          │                    BBG_root                        │
                          │ H(Lens.commit(BBG_poly) ‖ Lens.commit(A) ‖ Lens.commit(N)) │
                          │                   32 bytes                         │
                          └─────────┬──────────────────┬──────────────┬───────┘
                                    │                  │              │
                          ┌─────────┴────────┐  ┌──────┴─────┐ ┌─────┴──────┐
                          │   BBG_poly       │  │   A(x)     │ │   N(x)     │
                          │ 10 public dims   │  │ commitment │ │ nullifier  │
                          │ Lens.commit: 32B │  │ Lens: 32 B │ │ Lens: 32 B │
                          └─────────┬────────┘  └────────────┘ └────────────┘
                                    │
                  ┌───┬───┬───┬───┬─┴─┬───┬───┬───┬───┐
                  p  a_o a_i  n  loc coin card file time sig
```

BBG_poly: 10 public evaluation dimensions (particles, axons_out, axons_in, neurons, locations, coins, cards, files, time, signals).
A(x), N(x): independent private polynomial commitments, each with its own Brakedown Lens commitment.
cross-index consistency: structural — same polynomial, different evaluation dimensions. LogUp eliminated.

## checkpoint

```
CHECKPOINT = (
  BBG_root,           ← H(Lens.commit(BBG_poly) ‖ Lens.commit(A) ‖ Lens.commit(N)), 32 bytes
  folding_acc,        ← zheng-2 accumulator (constant size, ~30 field elements)
  block_height        ← current height
)

checkpoint size: O(1) — ~232 bytes
contains: proof that ALL history from genesis is valid
updated: O(1) per block via folding
proof size: ~2 KiB, verification: ~5 μs
```

## state transitions

```
TRANSITION: W × Signal → W' | ⊥

BBG has one operation: insert(signal).
BBG_poly updated at all affected evaluation dimensions.
cross-index consistency is free — same polynomial, no LogUp needed.

INPUT: Signal s = (ν, ℓ⃗, Δφ*, σ, t)
  ν     — neuron (signing agent)
  ℓ⃗    — one or more cyberlinks, each ℓ = (p, q, τ, a, v)
  Δφ*  — proven focus shift (sparse update)
  σ     — single zheng proof covering the entire signal
  t     — block height

EFFECT per cyberlink ℓ = (p, q, τ, a, v):

  public (BBG_poly):
    particles[H(p,q)]:  weight += a          ← axon-particle conviction
    particles[q]:       energy += a          ← target particle energy
    axons_out[p]:       insert H(p,q)        ← directional index
    axons_in[q]:        insert H(p,q)        ← directional index
    neurons[ν]:         focus -= cost(ℓ)     ← focus consumed

  private (independent polynomials):
    A(x): extend at new commitment point      ← O(1) Lens update
    N(x): absorb nullifiers for spent UTXOs   ← N'(x) = N(x) × (x - n_new)

  at block finalization:
    time[height]:       BBG_root snapshot

STRUCTURAL INVARIANT (BBG enforces):
  non-duplication: N(nullifier) = 0 → reject (double-spend)
  all other validity is pre-checked by cybergraph before insert

constraints: ~3,200 per cyberlink (polynomial updates, no hemera path rehash)
```

see [[cybergraph/validation]] for the full validation protocol, [[architecture]] for evaluation dimensions, [[privacy]] for the polynomial mutator set, [[cross-index]] for why LogUp is eliminated
