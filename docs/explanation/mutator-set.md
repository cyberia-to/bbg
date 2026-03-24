---
tags: cyber, computer science, cryptography
crystal-type: entity
crystal-domain: cyber
alias: mutator sets
---
# polynomial mutator set

## what it does

every private record in [[BBG]] — individual [[cyberlinks]], UTXO transfers, market positions — is hidden from the public graph. the polynomial mutator set proves membership ("this record exists") and non-membership ("this record hasn't been spent") without revealing which record.

## two polynomials

**commitment polynomial A(x):** all committed private records. $A(c_i) = v_i$ for commitment $c_i$.

- membership proof: PCS opening of $A(c)$ → O(1), ~200 bytes
- new record: extend $A$ at new evaluation point → O(1)

**nullifier polynomial N(x):** all spent records. $N(x) = \prod(x - n_i)$ for all nullifiers $n_i$.

- non-membership proof: PCS opening showing $N(c) \neq 0$ → O(1), ~200 bytes
- double-spend detection: $N(n) = 0$ → record already spent → rejection
- new nullifier: $N'(x) = N(x) \times (x - n_{\text{new}})$ → O(1) polynomial extension

## the heritage

the polynomial mutator set descends from Neptune's AOCL + SWBF (Append-Only Commitment List + Sliding Window Bloom Filter). the original used:

- AOCL (MMR): O(log N) hemera hashes for membership
- SWBF (128 KB bitmap): constant-size non-membership check + O(log N) archived MMR walk

the polynomial replacement:

| operation | AOCL + SWBF | polynomial |
|---|---|---|
| membership proof | O(log N) hemera | O(1) PCS opening |
| non-membership proof | 128 KB witness + O(log N) MMR | ~200 bytes PCS opening |
| witness size | 128 KB | 32 bytes |
| update cost | O(1) bitmap + periodic archive | O(1) polynomial extend |
| constraints per spend | ~40,000 | ~5,000 |

## why polynomial over SWBF

the SWBF was a practical compromise: constant-size active window, but archived chunks grew unboundedly as an MMR. the polynomial has no window — $N(x)$ encodes ALL nullifiers at ALL times. no archival, no chunks, no growing MMR. the entire nullifier history is one 32-byte PCS commitment.

the tradeoff: the polynomial has degree equal to the number of nullifiers (at $10^9$ nullifiers, degree $10^9$). PCS commitment of such a polynomial is feasible but the initial commitment requires O(N log N) work. incremental updates are O(1). batch updates (1000 spends per block) use vanishing polynomial: $N' = N \times \prod(x - n_i)$ — one polynomial multiply instead of 1000 sequential extensions.

## privacy preservation

the PCS opening proofs are zero-knowledge. opening $A(c_i)$ reveals nothing about other commitments. opening $N(n)$ reveals nothing about other nullifiers. the privacy guarantees match or exceed SWBF — which leaked probabilistic density information via bloom filter bits. the polynomial commitment is one opaque 32-byte digest.

see [[privacy]] for the privacy boundary specification, [[state]] for how the mutator set integrates with BBG_poly, [[storage]] for how polynomials are stored
