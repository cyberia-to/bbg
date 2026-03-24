---
tags: cyber
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.00025146291475061286
heat: 0.00021215453851177208
focus: 0.00017148160647053237
gravity: 0
density: 0.22
---
# data availability and erasure coding

DAS (Data Availability Sampling) and erasure coding are the availability layer in [[bbg]]. they answer a question no other layer answers: is the data physically present across the device set? validity proves correctness. ordering proves sequence. completeness proves nothing was withheld. availability proves the data survives.

## the availability problem

a device stores a file. the device dies. the file is gone. this is the simplest availability failure — and the one every sync system must solve.

the naive solution: replicate everything everywhere. every device holds a full copy. this works for small data sets but fails at scale — 3 devices × 1 TB = 3 TB total storage. a phone cannot hold what a workstation holds. full replication is wasteful for devices that overlap heavily and catastrophic for devices with limited storage.

the sophisticated failure: a device is online but selectively withholds data. it responds to some requests and ignores others. a CRDT merge converges — to incomplete state. the receiving device has no way to know data is missing. withholding is invisible without a completeness proof.

## erasure coding

erasure coding splits data into $k$ data chunks and generates $n - k$ parity chunks such that any $k$ of $n$ chunks reconstruct the original. the encoding is 2D Reed-Solomon over [[Goldilocks field]].

```
ENCODING:

  file → content-defined chunks → each chunk is a particle
  chunks arranged in √n × √n matrix

  row encoding:   k data elements → n elements per row (Reed-Solomon)
  column encoding: k data elements → n elements per column (Reed-Solomon)

  result: 2D encoded matrix where any √n × √n submatrix
          of original-dimension is sufficient for reconstruction

PARAMETERS:
  chunk size:     256 KiB (content-defined boundaries)
  rate:           k/n configurable per device group (default 1/2)
  field:          Goldilocks (p = 2^64 - 2^32 + 1)

  at rate 1/2: any 50% of chunks → full reconstruction
  storage overhead: 2× per file (distributed across N devices)
  per-device overhead: 2/N × file_size (much less than full replication)
```

### why 2D Reed-Solomon

1D erasure coding (k-of-n chunks) provides reconstruction but not efficient fraud proofs. if a device publishes a commitment to encoded data and the encoding is incorrect, detecting the error requires downloading the entire row.

2D encoding enables row-level and column-level fraud proofs. a single incorrectly encoded cell can be proven invalid by downloading one row and one column — O(√n) data instead of O(n). this is the foundation of efficient DAS.

### distribution across devices

chunks are distributed across the device set (for local sync) or the neuron set (for global sync). each device holds a subset of chunks — not a full replica.

```
DISTRIBUTION (local sync, 3 devices):

  file: 100 chunks → 200 encoded chunks (rate 1/2)

  device A (server):   chunks 0-199  (full set, always-on)
  device B (laptop):   chunks 0-99   (data chunks, work device)
  device C (phone):    chunks 100-149 (50 parity chunks, light device)

  any 100 chunks reconstruct the file
  device A dies → B + C have 150 chunks → reconstruction succeeds
  device B dies → A has everything → reconstruction trivial
  device C dies → A has everything → reconstruction trivial
  all three alive → verification without full download
```

the distribution strategy adapts to device capacity. a server holds more chunks. a phone holds fewer. the erasure coding guarantee is that any $k$ chunks from any combination of surviving devices is sufficient.

## data availability sampling (DAS)

DAS answers: "is the data available?" without downloading it. a verifier samples random chunk positions, requests each chunk with a Merkle proof, and checks consistency. if all samples verify, the data is available with high probability.

```
DAS PROTOCOL:

  1. device commits encoded chunks to a per-device polynomial commitment
     (chunk positions encoded as evaluation points)

  2. device publishes PCS commitment (signed by device key)

  3. verifier selects s random positions from [0, n)

  4. for each position i:
     request chunk[i] + PCS opening from device
     verify: H(chunk[i]) matches commitment
     verify: PCS opening valid against published commitment

  5. if all s samples verify:
     data is available with probability ≥ 1 - (1/2)^s

     s = 20 → 99.9999% confidence
     s = 30 → 99.99999999% confidence

COST:
  verifier downloads: s chunks + s PCS openings
  = O(s × chunk_size)
  = O(√n) for s = O(√n) samples

  vs full download: O(n × chunk_size)

  a phone verifies 1 TB of data with ~30 random samples
```

### why sampling works

the key insight: if a device withholds even a single chunk, the 2D erasure coding means that chunk's absence affects an entire row and column. the probability that a random sample misses all affected positions decreases exponentially with sample count.

with rate 1/2 encoding, withholding more than 50% of chunks means reconstruction fails. but withholding 50% of chunks means a random sample has 50% chance of hitting a withheld position. after 20 independent samples, the probability of missing all withheld positions is $(0.5)^{20} < 10^{-6}$.

withholding less than 50% means reconstruction succeeds anyway — the withheld chunks are recoverable from parity. the device gains nothing by withholding.

this creates a binary: either the data is available (and sampling confirms it) or reconstruction fails (and sampling detects it). there is no middle ground where partial withholding goes undetected.

## PCS completeness proofs

DAS proves chunks exist. PCS completeness proves the full set was committed — no chunks were omitted from the commitment.

```
PCS COMMITMENT:

  device commits all chunks to a polynomial:
    content_commitment = PCS.commit(chunk_poly)

  chunk_poly encodes chunk positions and hashes as evaluation points
  commitment: signed by device key

COMPLETENESS PROOF:

  verifier requests: "all chunks in positions [a, b]"
  device returns: chunks + PCS opening for the range

  PCS guarantee:
    polynomial binding means the commitment uniquely determines
    all evaluation points. a PCS opening for range [a, b]
    cannot omit a point that the polynomial encodes.
    the device cannot hide a chunk position that was committed.

  if the device committed 200 chunks but only reveals 150:
    the PCS opening for range [0, 199] will fail
    the missing positions violate polynomial binding
    algebraic — the commitment cannot lie about its contents
```

## three layers composed

each layer solves a different failure mode. no single layer is sufficient:

```
FAILURE                  CRDT alone    DAS alone    PCS alone    ALL THREE
─────────                ──────────    ─────────    ─────────    ─────────
device dies              data lost     recoverable  no help      recoverable
selective withholding    undetectable  detectable   provable     provable + detectable
merge conflict           resolved      no help      no help      resolved
incomplete transfer      undetectable  no help      provable     provable
verification cost        O(n)          O(√n)        O(1)         O(√n)
```

**CRDT (G-Set merge)** — handles merge semantics. content-addressed chunks have no conflicts. the grow-only set union is commutative, associative, idempotent. "do you have CID X? no? here." deduplication automatic. verification: H(received) == CID.

**PCS completeness** — handles provable completeness per namespace. a device commits its chunk set to a polynomial commitment. a peer requests a namespace range and gets an algebraic proof that nothing was omitted. O(1) proof size.

**DAS + erasure coding** — handles physical availability. data survives permanent device failure through k-of-n reconstruction. O(√n) sampling cost to verify availability without downloading. no single point of failure.

together: **provably complete, provably available, correctly merged.**

## DAS in local sync vs global sync

the mechanism is identical at both scales. the parameters differ:

| parameter | local (devices) | global (neurons) |
|---|---|---|
| participants | 1-20 devices | 10^3-10^6 neurons |
| chunk distribution | capacity-weighted | stake-weighted |
| sampling peers | all devices sample each other | random validator subset |
| reconstruction | any k-of-n devices | any k-of-n neurons in namespace |
| commitment | per-device polynomial commitment | per-neuron polynomial commitment (files.root) |
| fraud proofs | device key revocation | stake slashing |

at local scale, DAS ensures a phone can verify its server has all files without downloading them. at global scale, DAS ensures light clients can verify block data availability without running a full node.

the same erasure coding over [[Goldilocks field]], the same 2D Reed-Solomon structure, the same PCS commitment scheme. the availability layer is scale-invariant.

see [[sync]] for the full sync specification, [[data-availability]] for the bbg reference spec, [[design-principles]] for the three laws, [[nmt]] for NMT's surviving role in cold storage