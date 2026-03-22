---
tags: cyber, computer science, cryptography
crystal-type: entity
crystal-domain: cyber
alias: mutator sets
diffusion: 0.00010722364868599256
springs: 0.00046254928756903513
heat: 0.00035816643606952395
focus: 0.00026400989782760816
gravity: 0
density: 4.48
---
# mutator set

a privacy primitive that replaces both UTXO commitment sets and nullifier sets with two linked structures: [[AOCL]] and [[SWBF]]. invented by the [[neptune]] team (Alan Szepieniec, COSIC/KU Leuven). [[neptune]] launched mainnet February 2025.

## the problem it solves

the standard approach (Zcash model) uses polynomial commitments for the UTXO set and a sorted nullifier set for double-spend prevention. the nullifier set grows monotonically with every spend, forever. the mutator set eliminates unbounded nullifier growth by replacing the nullifier set with a sliding-window [[bloom filter]] that compacts old data into an [[MMR]].

## architecture

AOCL (Append-Only Commitment List) — an MMR storing addition records. appended when a UTXO is created, never modified. accumulator = O(log N) peak hashes. membership proof = Merkle path from leaf to peak.

SWBF (Sliding-Window Bloom Filter) — tracks which UTXOs have been spent by setting pseudorandom bit positions derived from the record. double-spend = all bits already set = verifier rejects. active window (128 KB) handles recent removals directly; older chunks compact into an MMR.

## unlinkability

addition record = `H_commit(record ‖ ρ)`, removal record = SWBF bit positions derived from `H_nullifier(record ‖ aocl_index ‖ ρ)`. these share zero structural similarity. the ZK proof establishes validity without revealing which AOCL entry is being spent.

## use in cyber

[[cyber]] inherits the primitive with its own hash ([[hemera]]-2 instead of Tip5) and its own VM ([[nox]] instead of Triton VM). same field ([[Goldilocks field]]). same architecture. different instantiation. the mutator set is the unified privacy layer for all private records in the [[BBG]]: cyberlinks, coin transfers, card operations.

see [[privacy]] for the full specification, [[BBG]] for graph architecture, [[AOCL]] for the commitment list, [[SWBF]] for the bloom filter