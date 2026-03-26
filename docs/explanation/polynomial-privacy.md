---
tags: cyber, cryptography
crystal-type: entity
crystal-domain: cyber
---
# polynomial privacy

## the privacy boundary

the cybergraph has a hard boundary: individual cyberlinks are private, aggregates are public.

a neuron links particle P to particle Q with weight w. nobody learns that this specific neuron made this specific link. the public graph sees: particle P gained energy, axon (P, Q) has aggregate weight W, neuron N spent focus. the individual contribution is hidden. the aggregate is committed and queryable.

this boundary exists because knowledge graph edges are intimate data — they encode what a mind considers related. publishing individual edges is surveillance. publishing aggregates is collective intelligence. the mutator set enforces the boundary cryptographically.

## two polynomials

private state uses two polynomial commitments, separate from the public BBG_poly.

**commitment polynomial A(x):** encodes every private record ever created — cyberlinks, UTXO transfers, market positions. each record c_i has an evaluation point: A(c_i) = v_i.

- membership proof: Lens opening of A(c) produces value v. O(1) verification, ~200 bytes.
- new record: extend A at a new evaluation point. O(1) field operations.
- the commitment is one 32-byte digest. it reveals nothing about which records exist or how many.

**nullifier polynomial N(x):** encodes every spent record. N(x) = product of (x - n_i) for all nullifiers n_i. the roots of N are exactly the nullifiers.

- non-membership proof: Lens opening showing N(c) is not zero. the record has not been spent. O(1) verification, ~200 bytes.
- double-spend detection: N(n) = 0 means nullifier n is a root — the record is already spent. rejection is immediate.
- new nullifier: N'(x) = N(x) times (x - n_new). one polynomial extension, O(1) field operations.

## why two polynomials, not one

A(x) and N(x) serve opposite purposes. A proves existence ("this record was committed"). N proves absence ("this record was not spent"). combining them into one polynomial would conflate the two questions and require more complex opening arguments.

the separation is clean: a valid spend proves A(c) is nonzero (record exists) AND N(nullifier(c)) is nonzero (not yet spent). then N is extended: N' = N times (x - nullifier(c)). future attempts to spend the same record find N(nullifier(c)) = 0.

## why separate from BBG_poly

BBG_poly is public state — anyone can open any dimension and learn the aggregate. A(x) and N(x) are private state — openings are zero-knowledge (reveal the queried value but nothing about other evaluation points).

the authenticated root combines them:

```
BBG_root = H(commit(BBG_poly) ‖ commit(A) ‖ commit(N))    32 bytes
```

three commitments, one hash, one root. public queries open BBG_poly. private proofs open A and N under zero-knowledge. the polynomial structure of each commitment is independent — opening BBG_poly at (particles, P, t) reveals nothing about A or N.

## O(1) proofs, 32-byte witness

every private operation produces a constant-size proof regardless of how many records exist:

```
membership:       Lens.open(A, c) → v          ~200 bytes
non-membership:   Lens.open(N, c) → nonzero    ~200 bytes
spend:            both openings + N extension  ~400 bytes total
```

the witness a user carries is one 32-byte commitment (the current Lens digest). no bitmap, no Merkle path, no archived chunks. at 10^9 records, the witness is still 32 bytes.

proof constraints per spend: ~5,000 (two Lens openings plus one polynomial extension). this is the cost of one private operation in the proving circuit.

## batch operations

a block with 1000 spends adds 1000 nullifiers. batch update:

```
N' = N × ∏(x - n_i)    for all new nullifiers n_i in the block
```

the product of 1000 linear factors is a degree-1000 polynomial Z. one polynomial multiply: N' = N times Z. one batch Lens recommit. total cost scales with batch size, not with total nullifier count.

## the economic model

privacy costs focus. creating a cyberlink spends focus proportional to weight. focus is the public resource that funds private operations. a neuron acquires focus by converting coins (explicit bridge from private economic layer to public graph layer) or by receiving delegation.

the bridge is the only point where private and public layers touch:

```
spend coin UTXO:   prove A(c) ≠ 0 and N(nullifier(c)) ≠ 0    (private layer)
credit focus:      update BBG_poly(neurons, N, t)               (public layer)
```

one direction: private coins become public focus. the reverse does not exist — focus cannot become coins. this asymmetry prevents the public layer from leaking back into the private layer.

## privacy guarantees

Lens opening proofs are zero-knowledge. opening A(c_i) reveals c_i and v_i but nothing about other commitments. opening N(n) reveals whether n is a root but nothing about other nullifiers. the commitment digests are opaque 32-byte values — they leak no information about the number of records, the distribution of nullifiers, or the density of the set.

this is stronger than bitmap-based approaches, which leak probabilistic density information through bit patterns. the polynomial commitment is algebraic — its structure reveals nothing beyond what is explicitly opened.

see [[privacy]] for the full specification, [[why-polynomial-state]] for public state, [[architecture-overview]] for the full pipeline
