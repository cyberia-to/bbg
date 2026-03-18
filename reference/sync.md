---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# sync

trustless namespace synchronization. individual [[cyberlinks]] are private — sync operates on public aggregate data: [[axons]], [[particles]], [[neurons]], and temporal state.

## sync operations

five query types, each backed by a different NMT namespace proof.

### outgoing axons from particle P

```
client wants: "all outgoing axons from particle P"

1. REQUEST
   client → peer: (axons_out, namespace=P, state_root=BBG_root)

2. RESPONSE
   peer → client:
     - NMT completeness proof π for namespace P in axons_out.root
     - all axon-particle entries in namespace P
     - each entry: axon CID, aggregate weight, market state, meta-score

3. VERIFY
   client checks:
     a) π valid against axons_out.root (extracted from BBG_root)
     b) all returned axon-particles fall within namespace P
     c) NMT completeness: no axon in namespace P was withheld

4. GUARANTEE
   "I have ALL outgoing axons from particle P. Nothing hidden."
```

### incoming axons to particle Q

```
client wants: "all incoming axons to particle Q"

1. REQUEST
   client → peer: (axons_in, namespace=Q, state_root=BBG_root)

2. RESPONSE
   peer → client:
     - NMT completeness proof π for namespace Q in axons_in.root
     - all axon-particle entries in namespace Q

3. VERIFY
   same structure as outgoing: NMT completeness against axons_in.root.

4. GUARANTEE
   "I have ALL incoming axons to particle Q. Nothing hidden."
```

### neuron public state

```
client wants: "neuron N's public state"

1. REQUEST
   client → peer: (neurons, namespace=N, state_root=BBG_root)

2. RESPONSE
   peer → client:
     - NMT proof for namespace N in neurons.root
     - neuron data: focus, karma, stake

3. VERIFY
   NMT proof against neurons.root.

4. GUARANTEE
   "I have neuron N's complete public state."
```

### particle data

```
client wants: "particle P's data"

1. REQUEST
   client → peer: (particles, namespace=P, state_root=BBG_root)

2. RESPONSE
   peer → client:
     - NMT proof for namespace P in particles.root
     - particle data: energy, π*
     - if axon-particle: weight, market state, meta-score

3. VERIFY
   NMT proof against particles.root.

4. GUARANTEE
   "I have particle P's complete public data."
```

### state at time T

```
client wants: "state at time T"

1. REQUEST
   client → peer: (time, namespace=<unit>, key=T, state_root=BBG_root)

2. RESPONSE
   peer → client:
     - NMT proof in time.root for the requested time namespace
     - BBG_root snapshot at time T

3. VERIFY
   NMT proof against time.root. the returned BBG_root is the
   authenticated state at time T — client can then sync any
   namespace against that historical root.

4. GUARANTEE
   "I have the authenticated state root at time T."
```

## incremental sync

```
client has: state at block height h₁
client wants: updates through block height h₂

1. REQUEST
   client → peer: (time, range=[h₁, h₂], state_root=BBG_root_h₂)

2. RESPONSE
   peer → client:
     - time.root entries between h₁ and h₂
     - for each monitored namespace: diff of added/removed/updated entries
     - updated NMT proofs at height h₂

3. VERIFY
   - time.root NMT completeness: all boundaries in range returned
   - each namespace diff verified against updated NMT roots
   - client reconstructs local state by applying diffs

data transferred: O(|changes since h₁|) — NOT O(|total state|)
```

## light client protocol

```
new client joins with no history:

1. obtain latest CHECKPOINT = (BBG_root, folding_acc, height) from any peer

2. verify folding_acc:
   final_proof = decide(folding_acc)        ← zheng-2 decision boundary
   verify(final_proof, BBG_root)
   → this single verification proves ALL history from genesis is valid
   → verification cost: 10-50 μs

3. sync namespaces of interest (any of the five query types above)

4. maintain:
   - fold each new block into local folding_acc (~30 field ops per block)
   - update mutator set proofs for owned private records (O(log N) per block)
   - update NMT proofs for monitored namespaces (O(log n) per block)

join cost: ONE zheng verification + namespace sync
ongoing cost: O(log N) per block
storage: O(|monitored_namespaces| + |owned_records| × log N)
```

trust requirement: only BBG_root (from consensus). peer is completely untrusted.

see [[architecture]] for the checkpoint structure, [[cross-index]] for NMT consistency, [[storage]] for tiered storage
