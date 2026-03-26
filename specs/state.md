---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0013038142900827774
heat: 0.0009314701217393358
focus: 0.0006310501357156885
gravity: 0
density: 0.6
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
TRANSITION: W × Transaction → W' | ⊥

BBG_poly updated at affected evaluation dimensions.
cross-index consistency is free — same polynomial, no LogUp needed.

TRANSACTION TYPES:

1. CYBERLINK — create private record + update public aggregates
   input:  (neuron, from_particle, to_particle, token, amount, valence, zk_proof)
   private effect:
     - extend independent commitment polynomial A(x) at new point (O(1) Lens update)
   public effect:
     - update BBG_poly(particles, H(from,to), t): axon weight
     - update BBG_poly(axons_out, from, t): outgoing index
     - update BBG_poly(axons_in, to, t): incoming index
     - update BBG_poly(neurons, neuron, t): focus deduction
     - update BBG_poly(particles, to, t): energy
   cost:   focus proportional to weight
   proof:  ZK proof of well-formed cyberlink + polynomial update consistency
   constraints: ~3,200 per cyberlink (polynomial updates, no hemera path rehash)

2. PRIVATE TRANSFER — move value between private records
   input:  (removal_records, addition_records, deltas, fee, zk_proof)
   effect:
     - extend A(x) for new commitments (O(1) per addition)
     - extend N(x) for spent nullifiers: N'(x) = N(x) × (x - n_new) (O(1) per spend)
   cost:   fee (publicly committed)
   proof:  ZK proof of spend validity + conservation (see [[privacy]])
   constraints: ~5,000 per transfer (polynomial openings, no SWBF witness)

3. COMPUTATION — execute nox reduction
   input:  (neuron, subject, formula, budget, signature)
   effect: update BBG_poly(neurons, neuron, t): focus consumed
   cost:   focus proportional to computation steps
   proof:  reduction trace + focus deduction

4. MINT CARD — create a non-fungible knowledge asset
   input:  (neuron, bound_particle, signature)
   effect: insert into BBG_poly(cards, card_id, t)
   cost:   focus fee
   proof:  particle exists in BBG_poly(particles, CID, t) + card_id uniqueness

5. TRANSFER CARD — transfer knowledge asset ownership
   input:  (from_neuron, to_neuron, card_id, signature)
   effect: update BBG_poly(cards, card_id, t): owner field
   cost:   fixed fee
   proof:  current ownership + signature validity

6. BRIDGE — convert coin to focus
   input:  (neuron, denomination, amount, signature)
   effect:
     - update BBG_poly(coins, denom, t): burn supply
     - update BBG_poly(neurons, neuron, t): add focus
   cost:   fixed fee
   proof:  coin balance sufficiency + conservation

VALIDITY CONDITIONS:
  1. authorization: valid signature OR valid ZK proof
  2. focus sufficiency: focus >= operation cost (for CYBERLINK, COMPUTATION, MINT CARD)
  3. conservation: inputs = outputs + fee (for PRIVATE TRANSFER, BRIDGE)
  4. consistency: structural (same polynomial — no separate cross-index proof needed)
  5. non-duplication: N(nullifier) ≠ 0 (for PRIVATE TRANSFER — polynomial non-membership)
  6. temporal: timestamp within acceptable range
```

see [[architecture]] for evaluation dimension specification, [[privacy]] for the polynomial mutator set, [[cross-index]] for why LogUp is eliminated
