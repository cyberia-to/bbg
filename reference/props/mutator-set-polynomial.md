---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-18
---
# mutator set polynomial compression

replace the SWBF active window (128 KB bitmap) + archived MMR chunks (spent.root) with a single polynomial commitment. non-membership proof becomes O(1) field evaluation instead of O(log N) MMR walk.

## current cost

the mutator set uses two structures:

```
AOCL (Append-Only Commitment List):
  MMR of all commitments ever added
  membership proof: O(log N) hemera hashes
  cyberlinks.root = H(MMR peaks)

SWBF (Sliding Window Bloom Filter):
  128 KB active window (bitmap)
  archived chunks as MMR in spent.root
  non-membership proof:
    active window:   check bitmap positions (O(1) but 128 KB witness)
    archived chunks: O(log N) MMR walk per chunk
  balance.root = H(active SWBF window)
```

for a UTXO spend:
```
1. prove commitment exists in AOCL      O(log N) hemera hashes
2. prove nullifier not in SWBF          O(1) bitmap check + O(log N) archived chunks
3. add nullifier to SWBF                O(1) bitmap update
4. update cyberlinks.root and spent.root

total proof: ~50 hemera calls for AOCL + SWBF at 10^9 UTXOs
constraints: ~40,000 (current estimate)
```

the 128 KB SWBF window is a constant-size witness, but it must travel with every spend proof. archived chunks grow unboundedly.

## the construction

represent the nullifier set as a polynomial N(x) where N(n_i) = 0 for all committed nullifiers n_i:

```
N(x) = ∏(x - n_i) for all nullifiers n_i

non-membership proof for candidate nullifier c:
  prove N(c) ≠ 0
  = one polynomial evaluation + one non-zero check
  = O(1) via PCS opening

membership proof (double-spend detection):
  prove N(c) = 0
  = one polynomial evaluation
  = O(1) via PCS opening
```

commit N(x) via zheng PCS. the commitment is one 32-byte digest regardless of how many nullifiers exist.

## incremental updates

when a new nullifier n_new is added:

```
N'(x) = N(x) × (x - n_new)

commitment update:
  C' = C × commit(x - n_new)
  = one field multiply + one linear polynomial commitment
  = O(1) field operations
```

the polynomial grows by degree 1 per nullifier. commitment update is O(1). no window sliding, no chunk archival.

## cost comparison

```
                            current (SWBF + MMR)         polynomial
witness size:               128 KB (active window)       32 bytes (PCS commitment)
non-membership proof:       O(1) bitmap + O(log N) MMR   O(1) evaluation
membership proof:           O(log N) MMR                 O(1) evaluation
update (add nullifier):     O(1) bitmap + periodic archive  O(1) polynomial extend
archived state:             growing MMR chunks            absorbed in polynomial
constraints per spend:      ~40,000                      ~5,000 (estimated)

at 10^9 nullifiers:
  SWBF+MMR: 128 KB witness + ~30 hemera calls for archived chunks
  polynomial: 32-byte commitment + 1 PCS opening (~1-5 KiB proof)
```

## AOCL unification

the AOCL (commitment list) is also an MMR. replace it with a polynomial too:

```
A(x) = polynomial over all commitments
A(c_i) = v_i for commitment c_i with value v_i

membership: PCS.open(A, c_i) → v_i
non-membership: PCS.open(A, c) → 0 (no commitment at this point)
```

both cyberlinks.root and spent.root become polynomial commitments:

```
current:
  cyberlinks.root = H(AOCL MMR peaks)
  spent.root = H(archived SWBF MMR)
  balance.root = H(active SWBF window)

polynomial:
  cyberlinks.commit = PCS.commit(A)     commitment polynomial
  spent.commit = PCS.commit(N)          nullifier polynomial
  balance: derived from A and N         (unspent = in A, not in N)
```

three sub-roots collapse to two polynomial commitments. balance becomes implicit: a UTXO is unspent iff A(c) ≠ 0 and N(nullifier(c)) ≠ 0.

## interaction with proof-carrying

with proof-carrying computation ([[proof-carrying]]), each signal folds its UTXO effects into the running accumulator:

```
signal processing:
  for each spend in signal:
    verify A(c) ≠ 0           (commitment exists)
    verify N(nullifier(c)) ≠ 0 (not yet spent)
    fold: N' = N × (x - nullifier(c))
    fold into accumulator

one fold per spend: ~30 field ops + 1 hemera hash
```

the nullifier polynomial update is absorbed into the proof-carrying fold. zero additional proving cost.

## privacy preservation

the polynomial commitment reveals nothing about individual nullifiers. PCS opening proofs are zero-knowledge (when using appropriate PCS). the privacy guarantees match or exceed the current SWBF approach:

- SWBF: bloom filter bits leak probabilistic information about nullifier density
- polynomial: commitment is one opaque 32-byte digest. no density leakage

## open questions

1. **polynomial degree at scale**: at 10^9 nullifiers, N(x) has degree 10^9. PCS commitment of degree-10^9 polynomial is feasible but expensive to compute initially. incremental updates are O(1), but the first commitment requires O(N log N) work. amortizable?
2. **batch updates**: a block with 1000 spends adds 1000 nullifiers. batch update: N' = N × ∏(x - n_i) = N × Z where Z is the vanishing polynomial of the batch. Z has degree 1000 — one polynomial multiply instead of 1000 sequential updates
3. **SWBF removal timeline**: SWBF is well-understood and implemented in Neptune heritage code. polynomial replacement is a significant change to the privacy layer. phased migration (polynomial as redundant verification, then primary) recommended
4. **interaction with algebraic NMT**: if public indexes use polynomial commitments ([[algebraic-nmt]]) and private state also uses polynomial commitments, the entire BBG_root could become a single structured polynomial commitment

see [[storage-proofs]] for retention guarantees, [[proof-carrying]] for fold integration, [[algebraic-nmt]] for public index polynomials
