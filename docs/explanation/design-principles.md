---
tags: cyber
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0009058271674072284
heat: 0.0006804941257485766
focus: 0.00046145879971487417
gravity: 0
density: 2.3
---
# design principles

the three laws that govern every design decision in [[bbg]].

## law 1: bounded locality

no global recompute for local change. every operation's cost is proportional to what it touches, not to the total graph size.

at 10^15 nodes, global operations are physically impossible. light-speed delays across Earth exceed any acceptable latency bound. a PageRank recomputation over the full graph would take longer than the time between blocks.

this eliminates most graph algorithms. what survives: operators with provable decay guarantees — [[diffusion]] (geometric via teleport), [[springs]] (exponential via screening), [[heat kernel]] (Gaussian tail). the [[tri-kernel]] is the complete set of local operators. see nox for the computation model.

bbg separates operations into two classes with distinct locality profiles:

### graph operations (cyberlinks)

a cyberlink creates a private record and updates public aggregates — bounded-local by construction:

- private record: append to cyberlinks.root AOCL, O(1) amortized
- axon update: update aggregate weight in particles.root, O(log n)
- directional indexes: update axons_out.root and axons_in.root, O(log n) each
- neuron focus: deduct focus in neurons.root, O(log n)
- particle energy: update energy in particles.root, O(log n)
- LogUp cross-index: O(k log k) proportional to axons touched, not |G|
- sync: O(|my_namespace|) data transfer, not O(|all_data|)

focus is public — it must be visible for [[cyberank]] computation. a cyberlink deducts focus proportional to weight, updates the axon aggregate (weight, market state, meta-score), updates neuron focus and particle energy, and recomputes O(1) polynomial evaluations in the affected dimensions. no global structure is touched. individual cyberlinks remain private — only aggregates are committed publicly.

### economic operations (private transfers)

touch the global mutator set — O(log N) where N is total UTXO count:

- AOCL append: O(1) amortized (MMR peak merge)
- SWBF bit set: O(1) per index in active window
- SWBF inactive proof: O(log N) MMR membership path
- proof maintenance: O(log L · log N) per UTXO lifetime

the mutator set (AOCL + SWBF) is a global structure. private transfers touch it. this is the cost of privacy — unlinkability requires a shared accumulator. the bound is logarithmic, not linear, so it scales to 10^9 UTXOs.

### bridge operations (coin → focus)

explicit conversion crosses the private→public boundary:

- spend coin UTXO via mutator set (economic, O(log N))
- credit focus polynomial dimension (graph, O(1) field ops)

this is the only operation that touches both layers. it is explicit — a neuron chooses when to convert private coins into public focus. the two worlds do not leak into each other otherwise.

## law 2: constant-cost verification

verification cost is O(1) — bounded by a constant independent of computation size.

any computation, regardless of how many steps it takes, produces a proof verifiable in 10-50 μs. this is stronger than proportional verification (cost ≤ c × computation). with [[zheng]]-2 folding, verification cost does not grow at all:

- N computation steps → fold into 1 constant-size accumulator → 1 decider (~6K constraints) → 1 proof (1-5 KiB)
- verification: 10-50 μs whether N = 100 or N = 10^9
- [[hemera]]-2 tree hashing: 1 permutation call per node (32-byte children fit in one rate block)

this enables planetary-scale delegation: any node can verify any other node's computation at fixed cost. the verifier's work is independent of the prover's work.

## law 3: structural security

security guarantees emerge from data structure invariants, not from protocol correctness.

a protocol can have bugs. a polynomial commitment whose binding property prevents fabricating evaluation points cannot lie about completeness — the algebraic structure itself prevents it.

examples in bbg:
- PCS binding: polynomial commitment uniquely determines all evaluation points (structural)
- SWBF double-spend: second spend sets already-set bits → rejection (structural)
- content addressing: H(e) → e means identity IS hash (structural)
- AOCL append-only: MMR structure physically prevents modification of past entries (structural)

the ZK circuit enforces the privacy boundary (see [[privacy]]). but the completeness guarantees come from PCS binding, not the circuit.

see [[architecture]] for the full layer model