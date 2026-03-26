---
tags: cyber
crystal-type: entity
crystal-domain: cyber
---
# data availability and algebraic DAS

DAS (Data Availability Sampling) proves data physically exists across the neuron set without downloading it. validity proves correctness. ordering proves sequence. completeness proves nothing was withheld. availability proves the data survives.

## polynomial nouns make DAS native

when particles ARE polynomials, DAS is no longer a separate encoding step. the particle's content is already a polynomial with a Lens commitment. extending that polynomial beyond its evaluation domain produces the erasure code automatically. the particle IS a polynomial. the extension IS the erasure code. no separate encoding pipeline.

sampling = Lens opening at random positions on the extended polynomial. reconstruction = polynomial interpolation from enough evaluations. the Lens commitment that identifies the particle is simultaneously the DAS commitment. one object serves both purposes.

this collapses the traditional DAS pipeline (data → encode → commit → sample → verify) to (polynomial noun → sample → verify). the encoding step vanishes because polynomial nouns are born erasure-coded.

## the availability problem

a neuron stores a file. the neuron dies. the file is gone. no amount of proof verifies data that no longer exists.

the harder failure: a neuron is online but selectively withholds data. it responds to some requests, ignores others. a CRDT merge converges — to incomplete state. the receiving neuron has no way to know data is missing without a completeness proof and an availability check.

## erasure coding

erasure coding splits data into k data chunks and generates n - k parity chunks such that any k of n chunks reconstruct the original. the encoding is 2D Reed-Solomon over Goldilocks field (p = 2^64 - 2^32 + 1).

```
original data:  √n × √n grid of field elements
extended:       2√n × 2√n with parity rows and columns
reconstruction: any √n × √n submatrix of original dimension is sufficient

parameters:
  chunk size:   256 KiB (content-defined boundaries)
  rate:         k/n configurable per neuron group (default 1/2)
  at rate 1/2:  any 50% of chunks → full reconstruction
  per-neuron:   2/N × file_size (distributed, not replicated)
```

2D encoding enables row-level and column-level fraud proofs. a single incorrectly encoded cell is provable by downloading one row and one column — O(sqrt(n)) data instead of O(n). this is the foundation of efficient sampling.

## algebraic DAS

the 2D Reed-Solomon grid maps naturally to a bivariate polynomial P(row, col). each cell in the extended grid is an evaluation of P at a grid point. the commitment is a single Lens digest:

```
each row     → polynomial over Goldilocks
all rows     → bivariate polynomial P(row, col)
commitment   = Lens.commit(P)    32 bytes
```

### sampling

a verifier picks random grid positions and requests each cell with a Lens opening proof:

```
per sample:
  request:      cell value at (row, col)
  proof:        Lens opening of P(row, col)    ~200 bytes
  verification: Lens.verify(commitment, (row, col), value, proof)    O(1) field ops

20 samples → 99.9999% confidence data is available
30 samples → 99.99999999% confidence

total bandwidth:    20 × (256 + 200) = ~9 KiB
total computation:  20 × O(1) field verifications
total constraints:  ~3K
```

a phone verifies 1 TB of data with ~20 random samples and ~9 KiB of bandwidth.

### why sampling works

if a neuron withholds even one chunk, the 2D erasure structure means the chunk's absence affects an entire row and column. the probability that random samples miss all affected positions decreases exponentially.

with rate 1/2 encoding, withholding more than 50% means reconstruction fails — and a random sample has 50% chance per try of hitting a withheld position. after 20 samples: probability of missing all withheld = (0.5)^20 < 10^-6.

withholding less than 50% means reconstruction succeeds anyway — the withheld chunks are recoverable from parity. the neuron gains nothing.

binary outcome: either data is available (sampling confirms) or reconstruction fails (sampling detects). no middle ground where partial withholding goes undetected.

### fraud proofs

incorrect encoding is detected when k+1 cells on a row or column fail to lie on a degree-k polynomial:

```
fraud proof:    k+1 cells + Lens openings
size:           (k+1) × (256 + ~200 bytes)
verification:   O(k) field operations
```

### namespace completeness

"give me all chunks in namespace N" uses the same Lens mechanism as algebraic NMT — a completeness proof is a Lens opening over the evaluation points in that namespace. DAS adds: "and here is proof the chunks are correctly erasure-coded." both proofs are polynomial evaluations against the same commitment.

## Lens completeness

DAS proves chunks exist. Lens completeness proves the full set was committed — no chunks omitted from the commitment.

```
neuron commits all chunks to polynomial:
  content_commitment = Lens.commit(chunk_poly)

completeness proof:
  verifier requests: "all chunks in positions [a, b]"
  neuron returns: chunks + Lens opening for the range

polynomial binding guarantees:
  the commitment uniquely determines all evaluation points.
  a Lens opening for range [a, b] cannot omit a point
  that the polynomial encodes. algebraic — the commitment
  cannot lie about its contents.
```

## three layers composed

each layer solves a different failure mode:

| failure | CRDT alone | DAS alone | Lens alone | all three |
|---------|-----------|-----------|-----------|-----------|
| neuron dies | data lost | recoverable | no help | recoverable |
| selective withholding | undetectable | detectable | provable | provable + detectable |
| merge conflict | resolved | no help | no help | resolved |
| incomplete transfer | undetectable | no help | provable | provable |

**CRDT** — merge semantics. content-addressed chunks have no conflicts. grow-only set union is commutative, associative, idempotent.

**Lens completeness** — provable completeness per namespace. one commitment, one opening, algebraic proof that nothing was omitted. O(1) proof size.

**DAS + erasure coding** — physical availability. data survives permanent neuron failure through k-of-n reconstruction. O(sqrt(n)) sampling cost.

together: provably complete, provably available, correctly merged.

## local vs global

the mechanism is identical at both scales. parameters differ:

| parameter | local (1-20 neurons) | global (network) |
|-----------|---------------------|------------------|
| participants | same identity | different identities |
| chunk distribution | capacity-weighted | stake-weighted |
| reconstruction | any k-of-n local neurons | any k-of-n in namespace |
| fraud response | neuron key revocation | stake slashing |

the same erasure coding, the same 2D Reed-Solomon, the same Lens commitment scheme. the availability layer is scale-invariant.

## cost summary

```
                        algebraic DAS
commitment:             32 bytes (one Lens)
per-sample proof:       ~200 bytes (Lens opening)
per-sample verify:      O(1) field operations
20-sample verification: ~9 KiB bandwidth, ~3K constraints
fraud proof (k=64):     ~30 KiB
namespace completeness: O(range) field operations
```

see [[data-availability]] for the reference specification, [[why-polynomial-state]] for why polynomial over hash tree, [[architecture-overview]] for the full pipeline
