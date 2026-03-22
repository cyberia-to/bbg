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

all authenticated state committed under a single root. individual [[cyberlinks]] are private (mutator set). the public state contains only aggregates.

## BBG root

```
BBG_root = H(
  particles.root       ‖    NMT (all particles: content + axons)
  axons_out.root       ‖    NMT by source (outgoing axon index)
  axons_in.root        ‖    NMT by target (incoming axon index)
  neurons.root         ‖    NMT (focus, karma, stake)
  locations.root       ‖    NMT (proof of location)
  coins.root           ‖    NMT (fungible token denominations)
  cards.root           ‖    NMT (names and knowledge assets)
  files.root           ‖    NMT (content availability, DAS)
  cyberlinks.root      ‖    MMR peaks hash (private record commitments)
  spent.root           ‖    MMR root (archived consumption proofs)
  balance.root         ‖    hemera-2 hash (active consumption bitmap)
  time.root            ‖    NMT (temporal index, 7 namespaces)
  signals.root         ‖    MMR (finalized signal batches)
)
```

13 sub-roots. each is 32 bytes ([[hemera]]-2 output). total input to root hash: 416 bytes = 52 F_p elements = ~7 absorption blocks.

## state diagram

```
                              ┌──────────────┐
                              │   BBG_root   │
                              └──────┬───────┘
                                     │
  ┌──────────┬───────────┬───────────┼───────────┬──────────┬──────────┐
  │          │           │           │           │          │          │
┌─┴───────┐┌─┴────────┐┌─┴────────┐┌─┴────────┐┌─┴───────┐┌─┴───────┐┌─┴────────┐
│particles││axons_out  ││axons_in  ││neurons   ││locations││coins    ││cards     │
│ (NMT)   ││ (NMT)     ││ (NMT)    ││ (NMT)    ││ (NMT)   ││ (NMT)   ││ (NMT)    │
└─────────┘└──────────┘└──────────┘└──────────┘└─────────┘└─────────┘└──────────┘

  ┌──────────┬───────────┬───────────┬──────────┬──────────┐
  │          │           │           │          │          │
┌─┴────────┐┌─┴─────────┐┌─┴────────┐┌─┴───────┐┌─┴────────┐
│files     ││cyberlinks  ││spent     ││balance  ││time      │
│ (NMT)    ││ (MMR/AOCL) ││ (MMR)    ││(bitmap) ││ (NMT)    │
└──────────┘└────────────┘└──────────┘└─────────┘└──────────┘
                                                       │
                                                 ┌─────┴──────┐
                                                 │  signals   │
                                                 │  (MMR)     │
                                                 └────────────┘
```

public indexes (NMTs): particles, axons_out, axons_in, neurons, locations, coins, cards, files, time.
private state (mutator set): cyberlinks (AOCL), spent (inactive SWBF archive), balance (active SWBF window).
finalization: signals (batch proofs).

## checkpoint

```
CHECKPOINT = (
  BBG_root,           ← all 13 sub-roots
  folding_acc,        ← zheng-2 accumulator (constant size, ~30 field elements)
  block_height        ← current height
)

checkpoint size: O(1) — a few hundred bytes
contains: proof that ALL history from genesis is valid
updated: O(1) per block via folding
proof size: 1-5 KiB, verification: 10-50 μs
```

## state transitions

```
TRANSITION: W × Transaction → W' | ⊥

TRANSACTION TYPES:

1. CYBERLINK — create private record + update public aggregates
   input:  (neuron, from_particle, to_particle, token, amount, valence, zk_proof)
   private effect:
     - append commitment to cyberlinks.root (AOCL)
   public effect:
     - update axon H(from, to) weight in particles.root
     - update axon in axons_out[from] and axons_in[to]
     - deduct focus from neurons.root[neuron]
     - update energy in particles.root[to]
   cost:   focus proportional to weight
   proof:  ZK proof of well-formed cyberlink + aggregate update consistency

2. PRIVATE TRANSFER — move value between private records
   input:  (removal_records, addition_records, deltas, fee, zk_proof)
   effect: update mutator set (AOCL append + SWBF bit set)
   cost:   fee (publicly committed)
   proof:  ZK proof of spend validity + conservation (see [[privacy]])

3. COMPUTATION — execute nox reduction
   input:  (neuron, subject, formula, budget, signature)
   effect: consumes focus from neurons.root, produces result
   cost:   focus proportional to computation steps
   proof:  reduction trace + focus deduction

4. MINT CARD — create a non-fungible knowledge asset
   input:  (neuron, bound_particle, signature)
   effect: insert into cards.root, creator = owner = neuron
   cost:   focus fee
   proof:  particle exists in particles.root + card_id uniqueness

5. TRANSFER CARD — transfer knowledge asset ownership
   input:  (from_neuron, to_neuron, card_id, signature)
   effect: update owner field in cards.root leaf
   cost:   fixed fee
   proof:  current ownership + signature validity

6. BRIDGE — convert coin to focus
   input:  (neuron, denomination, amount, signature)
   effect: burn coin supply in coins.root, add focus to neurons.root[neuron]
   cost:   fixed fee
   proof:  coin balance sufficiency + conservation

VALIDITY CONDITIONS:
  1. authorization: valid signature OR valid ZK proof
  2. focus sufficiency: focus >= operation cost (for CYBERLINK, COMPUTATION, MINT CARD)
  3. conservation: inputs = outputs + fee (for PRIVATE TRANSFER, BRIDGE)
  4. consistency: LogUp cross-index proof valid (axons_out ↔ axons_in ↔ particles.root)
  5. non-duplication: SWBF check passes (for PRIVATE TRANSFER)
  6. temporal: timestamp within acceptable range
```

see [[architecture]] for sub-root specification, [[privacy]] for mutator set, [[cross-index]] for LogUp