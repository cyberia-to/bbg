---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-23
depends: [algebraic-nmt.md]
---
# algebraic DAS — polynomial availability sampling

when NMTs become polynomials ([[algebraic-nmt]]), Data Availability Sampling inherits the algebraic structure automatically. DAS inclusion proofs become PCS openings instead of Merkle paths. this is not a separate optimization — it is the natural consequence of the completeness layer becoming algebraic.

## the insight

DAS currently uses NMT inclusion proofs for each sampled cell. the sampler picks a random cell position, requests the cell data plus an NMT authentication path, and verifies the path against the committed root.

the NMT path IS the bottleneck:

```
current DAS per sample:
  cell data:            ~256 bytes (chunk content)
  NMT inclusion proof:  O(log n) × 32 bytes ≈ 1 KiB (at n = 2³²)
  verification:         O(log n) hemera hash calls

20 samples for 99.9999% confidence:
  bandwidth:  20 × (256 + 1024) = ~25 KiB
  compute:    20 × 32 hemera verifications = 640 hemera calls
  constraints: 640 × 736 = ~471K constraints (hemera)
```

when the commitment structure is a polynomial instead of a tree, the inclusion proof is a PCS opening:

```
algebraic DAS per sample:
  cell data:            ~256 bytes (chunk content)
  PCS opening:          ~200 bytes (polynomial evaluation proof)
  verification:         O(1) field operations

20 samples for 99.9999% confidence:
  bandwidth:  20 × (256 + 200) = ~9 KiB
  compute:    20 × O(1) field verifications
  constraints: ~20 × 150 = ~3K constraints (estimated)
```

## how it works

### erasure coding (unchanged)

the 2D Reed-Solomon structure is preserved:

```
original data: √n × √n grid of field elements (Goldilocks)
extended:      2√n × 2√n with parity rows and columns
any √n × √n submatrix sufficient for reconstruction
```

Reed-Solomon operates over the Goldilocks field. this is unchanged — the encoding is independent of the commitment scheme.

### commitment (changed)

```
current (NMT):
  each row → NMT (sorted by namespace)
  all row NMT roots → column NMT
  block_commitment = column_nmt.root

  structure: tree of trees
  commitment: hemera hash chain
  opening: Merkle authentication path

algebraic:
  each row → polynomial over Goldilocks
  all row polynomials → matrix polynomial P(row, col)
  block_commitment = PCS.commit(P)

  structure: bivariate polynomial
  commitment: one PCS commitment (32 bytes)
  opening: PCS evaluation proof
```

the bivariate structure P(row, col) encodes the same 2D grid. the difference: the commitment is algebraic (polynomial binding) instead of structural (tree hashing).

### sampling (changed)

```
current:
  sampler picks random (row, col)
  requests: cell value + NMT path for row + NMT path for column
  verifies: paths against row root and column root

algebraic:
  sampler picks random (row, col)
  requests: cell value + PCS opening for P(row, col)
  verifies: PCS.verify(block_commitment, (row, col), value, proof)

one PCS verification instead of two NMT path walks
```

### fraud proofs (changed)

```
current:
  bad encoding detected when k+1 cells on a row/column
  don't lie on a degree-k polynomial
  fraud proof: k+1 cells + NMT inclusion proofs
  size: (k+1) × (256 + ~1 KiB) ≈ several KiB

algebraic:
  bad encoding detected the same way (polynomial degree check)
  fraud proof: k+1 cells + PCS openings
  size: (k+1) × (256 + ~200B) ≈ smaller
  verification: O(k) field operations (no hemera)
```

### namespace completeness (preserved)

NMT's sorting invariant provides namespace completeness: "all data in namespace N." with algebraic DAS, namespace completeness uses the same mechanism as algebraic NMT — LogUp range check or sorted polynomial evaluation:

```
"give me all chunks in namespace N":
  algebraic NMT already provides this via PCS opening
  DAS adds: "and here is proof the chunks are correctly erasure-coded"

the two proofs compose:
  completeness: PCS opening proves namespace content is complete
  availability: PCS opening proves erasure-coded cells are committed
  both are polynomial evaluations against the same commitment
```

## cost comparison

```
                        NMT-based DAS           algebraic DAS
────────────────────    ─────────────           ─────────────
commitment:             hemera tree              PCS commitment
  size:                 32 bytes (root)          32 bytes (PCS)
  cost:                 O(n log n) hemera        O(n log n) field ops

per-sample proof:       NMT auth path            PCS opening
  size:                 ~1 KiB                   ~200 bytes
  verification:         O(log n) hemera          O(1) field ops

20-sample verification:
  bandwidth:            ~25 KiB                  ~9 KiB
  hemera calls:         640                      0
  field operations:     0                        ~3,000
  constraints:          ~471K                    ~3K

fraud proof (k=64):
  size:                 ~80 KiB                  ~30 KiB
  verification:         O(k log n) hemera        O(k) field ops

namespace completeness:
  mechanism:            NMT sorting invariant    LogUp / sorted poly
  proof size:           O(range × log n)         O(range)
  verification:         O(range × log n) hemera  O(range) field ops
```

### savings at scale

```
block with 1000 signals, each ~3 KiB:
  total data: ~3 MB
  grid: ~1732 × 1732 (√3M elements)
  extended: ~3464 × 3464

  NMT-based DAS:
    commitment: ~3464 row NMTs + 1 column NMT
    hemera calls: ~3464 × 11 + 3464 × 12 ≈ ~80K hemera
    constraints for commitment: ~80K × 736 ≈ ~59M

  algebraic DAS:
    commitment: PCS.commit(P) for bivariate polynomial
    hemera calls: 0
    field operations: O(n log n) ≈ ~70M field ops
    constraints for commitment: ~70M (field ops are cheaper than hemera)

  verification (20 samples):
    NMT: 640 hemera = ~471K constraints
    algebraic: ~3K constraints

  verification improvement: 157×
```

## interaction with other proposals

### algebraic NMT

algebraic DAS is the layer-4 consequence of [[algebraic-nmt]] (layer 3). when the completeness layer becomes algebraic, the availability layer follows. one polynomial commitment serves both:

```
BBG_poly covers:
  layer 3 (completeness): PCS opening proves namespace is complete
  layer 4 (availability): PCS opening proves data is erasure-committed

same polynomial, same commitment, different query types
```

### universal accumulator

DAS commitment proofs fold into the [[universal-accumulator]]:

```
per block:
  fold state update proof (layer 3)        ~30 field ops
  fold DAS commitment proof (layer 4)      ~30 field ops
  fold signal validity proofs (layer 1)    ~30 field ops per signal

the accumulator covers all layers in one object
```

### gravity commitment

[[gravity-commitment]] applies to DAS: high-π data uses hot-layer polynomial (fewer WHIR rounds), low-π data uses cold-layer polynomial. DAS sampling cost follows the same power law as verification cost.

```
DAS for high-π block: ~5 KiB, ~1K constraints
DAS for cold block:   ~9 KiB, ~3K constraints
average (power-law):  ~6 KiB, ~1.5K constraints
```

### pi-weighted replication

[[pi-weighted-replication]] determines how many replicas exist. more replicas = higher base availability = fewer DAS samples needed for the same confidence:

```
high-π particle (1000 replicas):
  base availability very high
  DAS: 5 samples sufficient for 99.99%
  bandwidth: ~2.3 KiB

low-π particle (3 replicas):
  base availability minimal
  DAS: 20 samples for 99.9999%
  bandwidth: ~9 KiB
```

## migration path

```
phase 1 (current):          NMT-based DAS as specified
phase 2 (algebraic NMT):    NMT → polynomial for completeness
                            DAS still uses NMT paths for samples
phase 3 (algebraic DAS):    DAS samples use PCS openings
                            NMT paths eliminated from DAS entirely
phase 4 (unified):          one BBG polynomial serves completeness + availability
                            single commitment, two query types
```

phase 3 requires: PCS that supports efficient random-position openings on the bivariate erasure-coded polynomial. WHIR and Brakedown both support this — the erasure-coded grid IS a polynomial evaluation domain.

## open questions

1. **bivariate PCS efficiency**: the erasure-coded grid is naturally bivariate P(row, col). do WHIR/Brakedown support efficient bivariate openings, or must the grid be flattened to univariate? flattening works but may lose structure
2. **namespace-aware polynomial**: current NMT sorts by namespace, enabling namespace completeness by construction. the polynomial equivalent needs sorted evaluation domain or LogUp range check. is the sorted domain natural for the erasure grid?
3. **fraud proof size**: algebraic fraud proofs require k+1 PCS openings. if k is large (high-rate erasure code), this may exceed NMT fraud proof size. optimal erasure rate for algebraic DAS?
4. **streaming DAS**: current DAS can verify samples as they arrive (each NMT path is independent). with PCS openings, can samples be batch-verified more efficiently than independently? batch PCS opening may give additional savings

see [[algebraic-nmt]] for layer 3, [[data-availability]] for current DAS spec, [[universal-accumulator]] for folding DAS into accumulator, [[structural-sync]] for the five-layer framework
