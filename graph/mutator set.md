---
tags: cyber, computer science, cryptography
crystal-type: entity
crystal-domain: cyber
alias: mutator sets
---
# mutator set

a privacy primitive that replaces both UTXO commitment sets and nullifier sets with two linked structures: [[AOCL]] and [[SWBF]]. invented by the [[neptune]] team. used in [[cyber]] for private record tracking within the [[cybergraph]] — both economic UTXOs and individual cyberlinks are private records committed to cyberlinks.root (AOCL). the mutator set is the unified privacy layer for all private records.

## the problem it solves

the standard approach (Zcash model) uses polynomial commitments for the UTXO set and a sorted nullifier set for double-spend prevention. the nullifier set grows monotonically with every spend, forever:

```
Year 1:   10⁸ transactions  → 10⁸ nullifiers → 800 MB
Year 10:  10¹¹ cumulative   → 10¹¹ nullifiers → 800 GB
Year 50:  10¹³ cumulative   → 10¹³ nullifiers → 80 TB
```

the mutator set eliminates unbounded nullifier growth by replacing the nullifier set with a [[SWBF]] — a sliding-window [[bloom filter]] that compacts old data into an [[MMR]].

## architecture

```
┌─────────────────────────────────────────────────────┐
│                   MUTATOR SET                        │
├──────────────────────┬──────────────────────────────┤
│        AOCL          │           SWBF               │
│  (append-only        │  (sliding-window             │
│   commitment list)   │   bloom filter)              │
│                      │                              │
│  records creation    │  records spending            │
│  MMR structure       │  active window + inactive    │
│  O(log N) peaks      │  MMR for old chunks          │
│  append-only         │  O(log N) growth             │
└──────────────────────┴──────────────────────────────┘
```

creating a record appends a commitment to the [[AOCL]]. spending a record sets bits in the [[SWBF]]. the two structures share zero structural similarity visible to any observer — unlinkability is architectural. this applies to all private records: cyberlinks, coin transfers, card operations, and any future record type.

## unlinkability

```
addition record:  H_commit(record ‖ ρ)     → hash commitment
removal record:   bit positions in SWBF    → derived from H_nullifier(record ‖ aocl_index ‖ ρ)

these share ZERO structural similarity
the ZK proof establishes validity without revealing which AOCL entry is being spent
```

an observer sees a hash being appended (creation) and bits being set (spending). there is no way to correlate the two without the private witness.

## transaction circuit

```
PRIVATE TRANSFER CIRCUIT (~40,000 constraints)
═══════════════════════════════════════════════

Per input (4 max):
  commitment correctness (hemera-2):       ~736 constraints
  AOCL membership (MMR Merkle path):     ~4,000 constraints (half depth vs hemera-1)
  SWBF index derivation:                   ~500 constraints
  SWBF bit verification:                 ~3,000 constraints
  ownership proof:                         ~736 constraints
  ─────────────────────────────────────────────
  per input total:                        ~9,000 constraints

Output validation:                        ~3,500 constraints
Conservation (inputs = outputs + fee):      ~100 constraints
Delta consistency:                          ~300 constraints
Uniqueness:                                  ~50 constraints
═══════════════════════════════════════════════
TOTAL:                                   ~40,000 constraints
Proof size:                              1-5 KiB
Verification:                            10-50 μs
```

## proof maintenance

every record holder keeps proofs synchronized as the mutator set evolves. this is the cost of privacy.

| event | impact | cost |
|---|---|---|
| new record created | AOCL MMR may gain new peak | O(log N) hashes, O(1) expected per block |
| old record spent | SWBF bits set, inactive chunks affected | O(log L × log N) average |
| window slides | active chunk compacted into inactive MMR | O(log N) per affected proof |

for practical parameters (10⁹ users, 10-year UTXO): ~50 hemera-2 calls per block average. ~50 × 736 = ~37,000 constraints per block for maintenance.

## the [[neptune]] precedent

[[neptune]] (Alan Szepieniec, COSIC/KU Leuven) invented the mutator set and launched mainnet February 2025. [[cyber]] inherits the primitive with its own hash ([[hemera]]-2 instead of Tip5) and its own VM ([[nox]] instead of Triton VM). same field ([[Goldilocks field]]). same architecture. different instantiation.

see [[AOCL]] for the append-only commitment list, [[SWBF]] for the sliding-window bloom filter, [[BBG]] for how the mutator set fits into the graph privacy architecture, [[record]] for the UTXO data model
