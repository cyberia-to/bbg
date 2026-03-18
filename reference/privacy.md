---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# privacy

the [[cyberlink]] is private — who linked what is never disclosed. individual linking decisions are protected because surveillance kills the freedom to link.

the [[cybergraph]] is public — it is the aggregate. [[axons]] (total weight between particle pairs), [[neuron]] summaries (total focus, karma), [[particle]] energy, [[token]] supplies, π distribution — all derived from cyberlinks but revealing no individual contribution.

the [[mutator set]] provides: private ownership, unlinkable transactions, no trusted parties, and logarithmic verification — simultaneously.

## privacy boundary

```
                   PUBLIC (aggregate)              PRIVATE (individual)
─────────────────  ──────────────────────────────  ─────────────────────────────
CYBERLINK                                          7-tuple (ν, p, q, τ, a, v, t)
                                                   who linked what
                                                   individual conviction amount
                                                   individual valence
NEURON             total focus                     linking history
                   karma κ                         individual cyberlinks
                   total stake
PARTICLE           CID exists
                   total energy (Σ weight)
                   π* ranking
AXON               H(from, to) exists              which neurons contributed
                   aggregate weight A_{pq}          individual weights
TOKEN              denominations                   individual UTXO values
                   total supply per τ               owner identity
RECORD                                             value, owner, nonce, randomness
TRANSACTION        SWBF bit indices                which records spent
                   addition records                who spent them
                   Δ per particle                  new owners
                   proof validity                  link between add & remove
FOCUS              π distribution
                   rankings
```

the tri-kernel operates on axons — aggregate weights — not individual cyberlinks. the effective adjacency A^eff_{pq} sums contributions from many neurons. no individual contribution is visible. enough transparency for [[consensus]], enough privacy for participation.

## mutator set architecture

replaces both UTXO commitment polynomials and nullifier sets with two linked structures.

### AOCL — Append-Only Commitment List

an MMR (Merkle Mountain Range) storing addition records:

```
addition record for UTXO u:
  ar = H_commit(u.particle ‖ u.value ‖ u.owner ‖ u.nonce ‖ ρ)

  where ρ is hiding randomness contributed by the recipient

properties:
  - appended when a UTXO is created. never modified.
  - MMR structure: forest of perfect binary trees
  - peaks = O(log N) hash digests (each 32 bytes)
  - membership proof: Merkle path from leaf to peak, O(log N) hashes
  - append cost: O(1) amortized (merge adjacent equal-height peaks)
```

### SWBF — Sliding-Window Bloom Filter

tracks which UTXOs have been spent by setting pseudorandom bit positions:

```
removal record for UTXO u (with AOCL index l, randomness ρ):
  bit_indices = derive_indices(H_nullifier(u ‖ l ‖ ρ))

spending u:
  1. compute bit_indices from (u, l, ρ)
  2. for each index in active window: set the bit
  3. for each index in inactive window: provide MMR membership proof
  4. provide ZK proof that indices were correctly derived from a valid AOCL entry

double-spend prevention:
  second spend attempt → all bits already set → verifier rejects

unlinkability:
  addition record: H_commit(record ‖ ρ) — hash commitment
  removal record: bit positions in Bloom filter
  these share ZERO structural similarity visible to any observer
```

### sliding window

```
◄──── Inactive (compacted in MMR) ────►◄── Active Window ──►

┌──────┬──────┬──────┬──────┐  ┌──────────────────────────┐
│chunk₀│chunk₁│chunk₂│chunk₃│  │   2²⁰ bits (128 KB)     │
│(MMR) │(MMR) │(MMR) │(MMR) │  │   directly accessible    │
└──────┴──────┴──────┴──────┘  └──────────────────────────┘

window slides forward periodically.
oldest active chunk → compacted into MMR.
growth: O(log N) peaks regardless of chain age.
```

## record model

```
Record:
  particle: F_p⁴    32 bytes   content identifier
  value:    u64       8 bytes   energy amount
  owner:    F_p⁴    32 bytes   owner public key hash
  nonce:    F_p       8 bytes   random for uniqueness

commitment(r, ρ) = H_commit(r.particle ‖ r.value ‖ r.owner ‖ r.nonce ‖ ρ)
```

## private transfer circuit

```
PUBLIC INPUTS:
  aocl_peaks:    [F_p⁴; log(N)]     AOCL MMR peak hashes
  swbf_root:     F_p⁴               SWBF inactive chunks MMR root
  swbf_window:   F_p⁴               hash of active SWBF window
  removal_data:  [BitIndices; 4]     SWBF bit positions per input
  additions:     [F_p⁴; 4]          new addition records
  deltas:        [(F_p⁴, i64); 8]   per-particle value changes
  fee:           u64                 transaction fee

PRIVATE WITNESS:
  input_records, input_secrets, input_randomness
  aocl_indices, aocl_paths, swbf_paths
  output_records, output_randomness
  input_enabled, output_enabled

CONSTRAINTS (hemera-2, 32-byte hashes, 1 perm/node):
  input validation (4 inputs):              ~36,000
    commitment correctness: ~736 per input
    AOCL membership (MMR path): ~4,000 per input (half depth vs hemera-1)
    SWBF index derivation: ~500 per input
    SWBF bit verification: ~3,000 per input
    ownership proof: ~736 per input
  output validation (4 outputs):             ~3,500
  conservation:                                ~100
  delta consistency:                           ~300
  uniqueness:                                   ~50

TOTAL:                                       ~40,000 constraints
proof generation (zheng-2):                  sub-second
proof size:                                  1-5 KiB
verification:                                10-50 μs
```

## proof maintenance

every UTXO holder keeps proofs synchronized as the mutator set evolves:

```
new UTXO created:     AOCL path may need update, O(log N) hashes, O(1) expected/block
old UTXO spent:       SWBF MMR proofs may need update, O(log N) average
window slides:        new MMR path if your bits were in compacted chunk, periodic

total user cost:
  average: O(log L · log N) per UTXO lifetime
  for 10⁹ users, 10-year UTXO: ~50 hemera calls per block
  ~50 × 736 = ~37,000 constraints per block for maintenance
```

see [[architecture]] for the layer model, [[state]] for transaction types, [[cross-index]] for LogUp
