---
tags: cyber
crystal-type: entity
crystal-domain: cyber
---
# why mutator set over polynomial + nullifier

the standard approach (Zcash model) uses polynomial commitments for the UTXO set and a sorted nullifier set for double-spend prevention. this has a fatal flaw at planetary scale.

## the nullifier problem

the nullifier set grows monotonically with every spend, forever.

```
year 1:   10⁸ transactions  → 10⁸ nullifiers  → 800 MB
year 10:  10¹¹ cumulative   → 10¹¹ nullifiers → 800 GB
year 50:  10¹³ cumulative   → 10¹³ nullifiers → 80 TB
```

non-membership proof cost grows logarithmically but never stops. the claimed 4× circuit advantage of polynomial-over-Merkle evaporates when you include nullifier non-membership proofs. the polynomial approach saves on inclusion but spends those savings on exclusion.

## mutator set advantage

the mutator set replaces both structures:

- AOCL (append-only commitment list): stores additions in an MMR. O(log N) peaks. membership proof via Merkle path.
- SWBF (sliding-window Bloom filter): tracks removals by setting bit positions. old chunks compact into MMR. growth: O(log N) regardless of chain age.

the sliding window bounds the active state. old removal data compacts into an MMR that grows logarithmically. no unbounded nullifier accumulation.

## unlinkability

addition record = `H_commit(record ‖ ρ)` — a hash commitment.
removal record = SWBF bit positions derived from `H_nullifier(record ‖ aocl_index ‖ ρ)`.

these share zero structural similarity visible to any observer. the ZK proof establishes validity without revealing which AOCL entry is being spent. this is architectural unlinkability — not a protocol property that could be undermined by implementation bugs.

## production heritage

Neptune launched with a mutator set in February 2025. the architecture is proven in production, not theoretical.

## universal privacy primitive

in the current architecture, the mutator set handles ALL private records — both economic UTXOs and individual cyberlinks. the 7-tuple cyberlink (neuron, from, to, token, amount, valence, time) is a private record committed to cyberlinks.root (AOCL). this means the mutator set is the universal privacy primitive for bbg: every piece of data that must remain unlinkable to its creator passes through the same AOCL + SWBF mechanism, whether it represents a coin transfer or a knowledge graph edge.

see [[privacy]] for the full mutator set specification
