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

bbg without data availability is incomplete. authenticated state means nothing if the data behind it cannot be retrieved. algebraic DAS (Data Availability Sampling) allows light clients to verify that block data is available without downloading the full block, using Lens openings instead of Merkle paths.

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

the erasure coding structure is independent of the commitment scheme. Reed-Solomon operates over the Goldilocks field regardless of whether cells are committed via trees or polynomials.

## native DAS for polynomial particles

when particles ARE polynomials, DAS becomes native to the data model. the particle's content polynomial already exists as a Lens commitment. extending that polynomial beyond the Boolean hypercube produces the erasure code — no separate 2D Reed-Solomon encoding step is needed. the particle's Lens commitment IS the DAS commitment.

```
particle content         → polynomial f(x) over Goldilocks
Lens.commit(f)            → particle identity (via hemera wrapping)
f evaluated beyond 2^v   → erasure-coded extension (automatic)
sampling                 = Lens.open(commitment, random position)    ~75 bytes proof
reconstruction           = polynomial interpolation from k evaluations
```

to obtain 2D RS structure for row/column fraud proofs, reshape the noun polynomial as a √N x √N bivariate: f(x) → g(row, col) where row and col index a grid over the evaluation domain. this is a coordinate remapping, not a separate encoding. the polynomial is the same object.

the implication: every polynomial particle is automatically erasure-coded by construction. DAS sampling is Lens opening at random positions. no encoding pipeline, no separate commitment. the particle IS the erasure code.

the 2D RS description below remains the concrete mechanism for block-level DAS where multiple particles are batched into a single block commitment.

## algebraic DAS commitment

```
each row → polynomial over Goldilocks
all row polynomials → bivariate polynomial P(row, col)
block_commitment = Lens.commit(P)    one commitment, 32 bytes

structure: bivariate polynomial
commitment: one Lens commitment (32 bytes)
opening: Lens evaluation proof per sample
```

the bivariate structure P(row, col) encodes the 2D erasure grid. the commitment is algebraic (polynomial binding) instead of structural (tree hashing).

## algebraic sampling

```
sampler picks random (row, col):
  requests: cell value + Lens opening for P(row, col)
  verifies: Lens.verify(block_commitment, (row, col), value, proof)

one Lens verification per sample instead of tree path walks.

per-sample proof:
  cell data:         ~256 bytes (chunk content)
  Lens opening:       ~75 bytes (recursive Brakedown evaluation proof)
  verification:      O(λ log log N) field operations

20 samples for 99.9999% confidence (batch opening):
  bandwidth:  ~1.5 KiB (batch Lens opening, amortized across 20 samples)
  compute:    20 × O(λ log log N) field verifications
  constraints: ~20 × 150 = ~3K constraints
```

## comparison with NMT-based DAS

```
                        NMT-based DAS           algebraic DAS
────────────────────    ─────────────           ─────────────
commitment:             hemera tree              Lens commitment
  size:                 32 bytes (root)          32 bytes (Lens)

per-sample proof:       NMT auth path            Lens opening (recursive)
  size:                 ~1 KiB                   ~75 bytes
  verification:         O(log n) hemera          O(λ log log N) field ops

20-sample verification:
  bandwidth:            ~25 KiB                  ~1.5 KiB (batch opening)
  hemera calls:         640                      0
  constraints:          ~471K                    ~3K

fraud proof (k=64):
  size:                 ~80 KiB                  ~30 KiB
  verification:         O(k log n) hemera        O(k) field ops

namespace completeness:
  mechanism:            NMT sorting invariant    Lens binding
  proof size:           O(range × log n)         O(range)
  verification:         O(range × log n) hemera  O(range) field ops

improvement: 17× bandwidth, 157× verification constraints
```

## fraud proofs for bad encoding

```
if a block producer encodes a row incorrectly:

1. obtain enough cells from the row (k+1 out of 2k)
2. attempt Reed-Solomon decoding
3. if decoded polynomial doesn't match claimed commitment:
   → fraud proof = the k+1 cells with their Lens openings
   → any verifier can check: decode(cells) ≠ row polynomial
   → block rejected

size of fraud proof: O(k) cells with O(1) Lens openings each
verification: O(k) field operations — linear in row size
```

## namespace-aware sampling

```
light client interested in particle P:
  1. Lens opening tells which evaluation region contains namespace P
  2. sample random cells from THAT region
  3. each cell comes with:
     a) Lens opening (proves cell belongs to committed polynomial)
     b) namespace inclusion (proves cell is in correct namespace)
  4. if enough cells available → data is available with high probability

completeness and availability compose:
  completeness: Lens opening proves namespace content is complete
  availability: Lens opening proves erasure-coded cells are committed
  both are polynomial evaluations against the same commitment
```

## relationship to storage tiers

DAS covers the files dimension of BBG_poly — the content availability commitment. files dimension commits to particle content stored at L3 (content store). DAS proves that particle content is retrievable, not just that CIDs exist in the particles dimension. without files and DAS, the knowledge graph is a collection of hashes pointing to nothing.

storage tier mapping:

- L1 (hot state): polynomial commitments, aggregate data, mutator set state — guaranteed by validators running the chain
- L2 (particle data): full particle/axon data indexed by CID — SSD, milliseconds
- L3 (content store): particle content (files) indexed by CID — DAS availability proofs via files dimension
- L4 (archival): historical state via time dimension of BBG_poly — DAS ensures availability during active window

the DAS active window must be long enough for light clients to sample and reconstruct any namespace they care about. after the window, data relies on archival nodes and incentivized storage.

see [[architecture]] for the layer model, [[storage]] for π-weighted replication and storage tiers, [[sync]] for DAS in the sync protocol
