---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0004083582138740482
heat: 0.00032074780743811715
focus: 0.00024026884999283105
gravity: 0
density: 0.47
---
# data availability

bbg without data availability is incomplete. authenticated state means nothing if the data behind it cannot be retrieved. DAS (Data Availability Sampling) allows light clients to verify that block data is available without downloading the full block.

## 2D Reed-Solomon erasure coding

block data arranged in a √n × √n grid, erasure-coded in both dimensions:

```
ORIGINAL DATA (k × k):          EXTENDED DATA (2k × 2k):

┌─────┬─────┬─────┐            ┌─────┬─────┬─────┬─────┐
│ d₀₀ │ d₀₁ │ d₀₂ │            │ d₀₀ │ d₀₁ │ d₀₂ │ p₀₃ │
├─────┼─────┼─────┤            ├─────┼─────┼─────┼─────┤
│ d₁₀ │ d₁₁ │ d₁₂ │  ──RS──►  │ d₁₀ │ d₁₁ │ d₁₂ │ p₁₃ │
├─────┼─────┼─────┤            ├─────┼─────┼─────┼─────┤
│ d₂₀ │ d₂₁ │ d₂₂ │            │ d₂₀ │ d₂₁ │ d₂₂ │ p₂₃ │
└─────┴─────┴─────┘            ├─────┼─────┼─────┼─────┤
                                │ p₃₀ │ p₃₁ │ p₃₂ │ p₃₃ │
                                └─────┴─────┴─────┴─────┘

RS encoding over Goldilocks field.
any k of 2k values in a row → reconstructs the row.
any k of 2k values in a column → reconstructs the column.
```

## NMT commitment structure

```
for each row i:
  row_nmt_root_i = NMT_commit(row_i_cells, sorted by namespace)

column NMT:
  col_nmt_root = NMT_commit([row_nmt_root_0, ..., row_nmt_root_{2k-1}])

block data commitment:
  data_root = col_nmt_root
```

with [[hemera]]-2: each NMT node is 64 bytes (two 32-byte children), hashed in 1 permutation call. the entire NMT commitment tree hashes at 2× the throughput of hemera-1.

## namespace-aware sampling

```
light client interested in particle P:
  1. col_nmt tells which rows contain namespace P
  2. sample random cells from THOSE rows
  3. each cell comes with:
     a) row NMT inclusion proof (proves cell belongs to row)
     b) column NMT inclusion proof (proves row belongs to block)
     c) namespace proof (proves cell is in correct namespace)
  4. if enough cells available → data is available with high probability

sampling complexity: O(√n) cells for 99.9% confidence
each sample: O(log n) × 32 bytes proof size
```

## fraud proofs for bad encoding

```
if a block producer encodes a row incorrectly:

1. obtain enough cells from the row (k+1 out of 2k)
2. attempt Reed-Solomon decoding
3. if decoded polynomial doesn't match claimed row NMT root:
   → fraud proof = the k+1 cells with their NMT proofs
   → any verifier can check: decode(cells) ≠ row commitment
   → block rejected

size of fraud proof: O(k) cells with O(log n) proofs each
verification: O(k log n) — linear in row size, logarithmic in block size
```

## relationship to storage tiers

DAS covers files.root — the content availability commitment. files.root is an NMT committing to particle content stored at L3 (content store). DAS proves that particle content is retrievable, not just that CIDs exist in particles.root. without files.root and DAS, the knowledge graph is a collection of hashes pointing to nothing.

storage tier mapping:

- L1 (hot state): NMT roots, aggregate data, mutator set state — guaranteed by validators running the chain
- L2 (particle data): full particle/axon data indexed by CID — SSD, milliseconds
- L3 (content store): particle content (files) indexed by CID — DAS availability proofs via files.root
- L4 (archival): historical state snapshots, old proofs — DAS ensures availability during active window

the DAS active window must be long enough for light clients to sample and reconstruct any namespace they care about. after the window, data relies on archival nodes and incentivized storage.

see [[architecture]] for the layer model, [[storage-proofs]] for retention proofs across tiers