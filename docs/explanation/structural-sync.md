---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-22
---
# structural sync — consensus without consensus

## the problem with consensus

distributed systems need agreement. the standard solution is consensus: PBFT, Tendermint, Nakamoto, HotStuff. these protocols share a structure: nodes exchange messages, a leader proposes, voters approve, finality follows some quorum rule.

the cost is structural:

```
PBFT:           O(N²) messages per round, 3f+1 nodes required
Tendermint:     O(N) messages per round, leader rotation, view changes
Nakamoto:       O(N) block propagation, probabilistic finality, energy cost
HotStuff:       O(N) messages, but still leader-based, still quorum-based
```

every consensus protocol pays for COORDINATION. nodes must COMMUNICATE to agree. the cost is in the communication pattern, not in the computation. a solo node doing the same computation would be free. consensus makes it expensive.

the question: is coordination the only path to agreement?

## three layers, three disciplines

consider three independent mechanisms, each from a different branch of mathematics:

**algebra: conflict-free replicated data types (CRDTs)**

a CRDT is an algebraic structure — a join-semilattice — where the merge operation is commutative, associative, and idempotent:

```
a ⊔ b = b ⊔ a                    (commutative — order doesn't matter)
(a ⊔ b) ⊔ c = a ⊔ (b ⊔ c)      (associative — grouping doesn't matter)
a ⊔ a = a                        (idempotent — duplicates don't matter)
```

consequence: any two nodes that receive the SAME SET of operations converge to the SAME state, regardless of the order they received them. no coordination needed. convergence is a mathematical property of the merge function, not a protocol outcome.

the gap: CRDTs guarantee convergence IF you have the same set. they cannot prove you DO have the same set. a Byzantine peer that withholds some operations causes you to converge on a different (incomplete) state. you converge, but on the wrong answer.

**logic: Namespace Merkle Trees (NMTs)**

an NMT is a binary hash tree where each internal node carries the minimum and maximum namespace labels of its subtree. this sorting invariant is structural — enforced by the tree construction itself.

consequence: a completeness proof for namespace N is the set of sibling hashes along paths to all leaves in namespace N. the sorting invariant guarantees: if a leaf with namespace N exists anywhere in the tree, it MUST appear in this proof. omission is structurally impossible — the tree cannot represent a state where a leaf exists but is absent from the proof.

the guarantee is unconditional. it does not depend on honest majorities, cryptographic assumptions beyond collision resistance, or correct protocol execution. the DATA STRUCTURE prevents lying.

the gap: NMTs prove completeness but say nothing about whether the data is physically available. a node can prove "here is everything in namespace N" while the actual data has been lost or destroyed.

**probability: Data Availability Sampling (DAS)**

data is erasure-coded using 2D Reed-Solomon: an n-element dataset is arranged in a √n × √n grid, extended with parity rows and columns. any √n × √n submatrix suffices to reconstruct the original.

consequence: a verifier samples O(√n) random cells and checks each against an NMT inclusion proof. if all samples pass, the data is available with probability 1 - (1/2)^k where k is the sample count. at k=20: 99.9999% confidence without downloading the full dataset.

the guarantee is probabilistic but information-theoretic — it does not depend on computational hardness, only on sampling randomness.

the gap: DAS proves existence but provides no merge semantics. multiple versions of the data might exist with no resolution rule.

## the composition

each layer alone is incomplete. each pair covers more:

```
CRDT + NMT:     converge on provably complete data — but if data is lost, gone
CRDT + DAS:     converge on available data — but can't prove completeness
NMT  + DAS:     provably complete and available — but no merge semantics
```

all three together:

```
CRDT + NMT + DAS:  converge on provably complete, provably available data
                    with zero coordination
```

the three gaps cancel:

| failure | CRDT alone | + NMT | + DAS | all three |
|---------|-----------|-------|-------|-----------|
| merge conflict | resolved | resolved | resolved | resolved |
| selective withholding | undetectable | **provable** | detectable | **provable** |
| incomplete transfer | undetectable | **provable** | no help | **provable** |
| data loss | no help | no help | **recoverable** | **recoverable** |
| byzantine sender | wrong state | **caught** | detected | **caught + recovered** |

each layer contributes a guarantee from a different mathematical universe:

```
algebra:      deterministic convergence    (CRDT)
logic:        unconditional completeness   (NMT)
probability:  physical availability        (DAS)
```

## what this IS

this composition is not consensus. it is something that does not have a standard name in the distributed systems literature.

**consensus** solves STATE MACHINE REPLICATION: given a stream of operations, agree on their total order, execute in that order, arrive at the same state. the cost is coordination — nodes must agree on ORDER before they can agree on STATE.

the CRDT+NMT+DAS composition solves a different problem: **VERIFIED SET CONVERGENCE**. given a universe of operations (unordered), every participant:

1. merges them independently (CRDT — order is irrelevant)
2. proves they have the complete universe (NMT — nothing missing)
3. proves the universe physically exists (DAS — data survives)

the result: identical state on all participants with cryptographic certainty, without agreeing on any ordering, without any leader or quorum. data transfer still happens (gossip, pull, DAS sampling) — what vanishes is the ordering coordination. nodes exchange data and proofs, but never negotiate sequence.

```
consensus:                    structural sync:
─────────                     ────────────────
agree on order → compute      compute → prove completeness
O(N²) or O(N) messages        0 ordering messages (data still transferred)
leader/quorum required         no leader, no quorum
finality after protocol        convergence after receipt
Byzantine tolerance: 3f+1     Byzantine detection: structural
protocol correctness           mathematical properties
```

the distinction is fundamental: consensus derives correctness from PROTOCOL EXECUTION (did the right messages arrive in the right order?). structural sync derives correctness from MATHEMATICAL STRUCTURE (is the merge commutative? does the tree satisfy the invariant? do the samples verify?).

## why ordering is unnecessary

CRDTs make ordering irrelevant because the merge function is commutative. if every operation commutes with every other operation, total order does not affect the final state. any permutation of the same set produces the same result.

this is not a limitation — it is a FEATURE. ordering is the primary cost of consensus. eliminate the need for ordering and the coordination cost vanishes.

but commutativity alone is insufficient for a knowledge graph:

```
scenario: neuron A creates cyberlink L₁. neuron B creates cyberlink L₂.

CRDT merge: {L₁} ⊔ {L₂} = {L₁, L₂}   (set union — commutative)

correct. no ordering needed. both cyberlinks exist in the merged state.
```

```
scenario: neuron A names ~/doc → CID_v1. neuron A names ~/doc → CID_v2.

these operations are NOT commutative in the naive sense:
  apply v1 then v2 → ~/doc = CID_v2
  apply v2 then v1 → ~/doc = CID_v1

resolution: signals carry ordering fields (prev, VDF, step).
deterministic tiebreak: causal order > VDF time > hash.
both nodes compute the SAME total order from the SAME set of signals.
ordering is local (derived from signal metadata), not global (derived from consensus).
```

the key: ordering is embedded IN THE DATA (signal structure), not imposed BY THE PROTOCOL (consensus). signals carry their own causal relationships. the merge function extracts a deterministic total order from the signal set without any communication.

ordering is necessary for mutable state. what is unnecessary is COORDINATED ordering — the expensive part where nodes exchange messages to agree on sequence. structural sync replaces coordinated ordering with deterministic local ordering: given the same signal set, every node independently computes the same total order. the ordering algorithm is local. the agreement is a consequence of set equality (guaranteed by NMT + DAS), not of protocol execution.

## relationship to impossibility results

**FLP impossibility** (Fischer, Lynch, Paterson 1985): no deterministic consensus protocol can guarantee termination in an asynchronous network with even one crash failure.

structural sync does not violate FLP because it solves a different problem. FLP constrains AGREEMENT: nodes must decide on the same value. structural sync provides VERIFIED CONVERGENCE: nodes that receive the same set converge to the same state (CRDT property), and can verify they have the same set (NMT property).

the distinction: FLP requires that all correct nodes eventually decide — a liveness guarantee. structural sync guarantees that any node with a complete set has the correct state — a safety guarantee conditional on receipt. NMT transforms the unconditional liveness question ("did everyone receive everything?") into a verifiable safety question ("do I have everything?"). a node can ANSWER the question locally, without waiting for other nodes to respond. this sidesteps FLP because no termination of a distributed protocol is required — only local verification.

**CAP theorem** (Brewer 2000): a distributed system cannot simultaneously provide Consistency, Availability, and Partition tolerance.

structural sync provides:
- eventual consistency (CRDT convergence — weaker than strong consistency)
- availability (DAS — data is reconstructible from any k-of-n)
- partition tolerance (CRDT operates during partitions, NMT verifies after)
- completeness (NMT — a fourth property not in CAP)

CAP constrains strong consistency. structural sync trades it for eventual consistency + verifiable completeness, gaining a property CAP doesn't even model.

## the five verification layers

structural sync extends to a five-layer verification stack where each layer is independently verifiable:

```
layer           discipline      mechanism            property
─────           ──────────      ─────────            ────────
1. validity     computation     zheng proof          state transition is correct
2. ordering     data structure  hash chain + VDF     operations carry their own order
3. completeness logic           NMT                  nothing was omitted
4. availability probability     DAS + erasure        data physically exists
5. merge        algebra         CRDT / foculus       convergence is deterministic
```

layers 1-4 are scale-invariant — they work identically for two devices or two billion nodes. layer 5 adapts to trust context: CRDT (same neuron, cooperative devices) or foculus (different neurons, adversarial).

the stack is modular: each layer can be verified independently. a verifier can check any subset:

```
check only layer 1:      "this signal is valid" (10-50 μs)
check layers 1+3:        "this signal is valid AND I have all signals"
check layers 1+3+4:      "valid, complete, and the data exists"
check all five:           "full structural sync guarantee"
```

each additional layer is an independent verification with independent mathematical guarantees. failures in one layer do not compromise the others.

## comparison with existing systems

```
system          consensus    completeness    availability    merge
──────          ─────────    ────────────    ────────────    ─────
Bitcoin         Nakamoto     no proof        full nodes      N/A
Ethereum        Casper       light clients   proposed DAS    N/A
Celestia        Tendermint   NMT proofs      DAS             N/A
IPFS            none         no proof        DHT             no merge
Automerge       none         no proof        no guarantee    CRDT
Git             none         hash chain      clone           manual merge

bbg             none*        NMT proof       DAS + erasure   CRDT / foculus
```

\* foculus is π convergence, not a consensus protocol. no leader, no quorum, no voting rounds. convergence is a mathematical fixed point, not a protocol outcome.

bbg is the first system to compose all three layers. Celestia has NMT + DAS but uses traditional consensus (Tendermint) for ordering and relies on external rollups for merge semantics. Automerge has CRDTs but no completeness proof and no availability guarantee. bbg's contribution is the composition and the recognition that the three layers are sufficient.

## what structural sync enables

**consensus-free light clients.** a light client joins the network by:
1. downloading a checkpoint (~200 bytes: universal accumulator)
2. verifying it (one zheng proof, 10-50 μs)
3. syncing namespaces of interest (NMT completeness proof per namespace)
4. sampling availability (O(√n) DAS samples)

no consensus participation. no block header chain traversal. no sync committee. one verification proves all history. NMT proves completeness. DAS proves availability.

**byzantine detection, not tolerance.** traditional BFT tolerates up to f Byzantine nodes among 3f+1. structural sync detects and prevents Byzantine faults through data structure properties:

| fault | mechanism | guarantee | after detection |
|---|---|---|---|
| forging | zheng proof | structurally impossible — invalid proof cannot be constructed | rejected by any peer |
| equivocation | hash chain + VDF | O(1) detectable — two signals with same prev is cryptographic proof | misbehavior proof → key revocation at local scale, stake slashing at global scale |
| withholding | NMT completeness | structurally provable — the tree cannot hide leaves in a requested range | peer excluded, data reconstructed from erasure coding |
| data loss | DAS + erasure | recoverable from any k-of-n surviving chunks | automatic reconstruction |
| flooding | VDF rate limit | physically bounded — each signal costs T_min sequential time | rate exceeded → signals queued, not processed |

forging is structurally impossible. equivocation and withholding are structurally detectable with cryptographic proof of misbehavior. the key distinction from BFT: detection does not require a protocol round — it is a property of the data received. any single peer can independently verify and produce a misbehavior proof. no honest majority assumption.

**scale-invariant sync.** the same protocol works for:
- 2 devices of one user (local CRDT sync)
- 1000 validators (global foculus convergence)
- 10^9 light clients (query sync with proofs)

layers 1-4 are identical at all scales. layer 5 adapts the merge function. no separate protocol for each scale.

**offline-first operation.** devices operate independently, accumulate signals, sync when they reconnect. convergence is guaranteed by CRDT. completeness is verifiable by NMT after reconnection. availability survives through erasure coding. no "online requirement" for correctness.

## who builds the tree

a common objection: NMT proves completeness relative to what is IN the tree. who constructs the tree? if the builder is Byzantine, they can build a valid tree that omits operations.

the answer: each source builds its own tree. at local scale, each device commits its signal chain to a per-device NMT (namespaced by step). at global scale, each neuron commits its signals to a per-neuron NMT. no central tree builder exists.

completeness verification works peer-to-peer:

```
device A requests: "all signals from device B in steps [100, 200]"
device B returns:  NMT completeness proof for range [100, 200]

the proof guarantees: within B's tree, nothing in [100, 200] was omitted.
the proof does NOT guarantee: B's tree contains all signals B ever created.
```

the remaining question — "did B create signals it did not include in its tree?" — is answered by the ordering layer. B's hash chain is append-only. if B published signal at step 150 to device C but omitted it from the tree shown to device A, the hash chain fork is detectable: A and C compare chain hashes and discover divergence. equivocation = two trees from the same source with different contents at the same step range = cryptographic misbehavior proof.

the composition: NMT proves per-tree completeness. hash chain + equivocation detection proves cross-tree consistency. together: completeness is not relative to one tree — it is global.

see [[sync]] for the full protocol, [[signal-sync explained|signal-sync]] for ordering design rationale.

## cost model

structural sync replaces coordination cost with computation and bandwidth:

```
COMPUTATION (per signal):
  zheng proof generation:    ~100 ms (on device, amortized across batch)
  zheng proof verification:  10-50 μs (on any peer)
  VDF computation:           T_min configurable (default ~1s)
  NMT update:                O(log n) hashes per signal
  erasure encoding:          O(n log² n) field ops per file

BANDWIDTH (per sync):
  merkle clock comparison:   O(1) — single hash (32 bytes)
  signal exchange:           O(|missing signals| × signal_size)
                             signal_size: 1-5 KiB (proof + impulse + metadata)
  NMT completeness proof:    O(log n) × 32 bytes per namespace range
  DAS sampling:              O(√n) samples × (chunk_size + proof_size)
                             ~20 samples for 99.9999% confidence

STORAGE (per device):
  signal chain:              O(signals × signal_size)
  NMT:                       O(signals × 32 bytes) for tree nodes
  erasure parity:            (n-k)/k overhead per file (default 2× at rate 1/2)
  total per-device:          2/N × total_data (distributed, not replicated)

COMPARISON:
  Tendermint:     O(N) messages per round × round_time × validators
  structural sync: 0 rounds. verification cost = signal_count × proof_verify_time
  break-even:     structural sync wins when proof verification is cheaper
                  than consensus round (almost always, since 10-50 μs < 1s round)
```

the dominant cost is zheng proof generation (~100 ms). this is paid once by the signal creator. verification by all peers is 10-50 μs each — three orders of magnitude cheaper than creating the signal. the asymmetry is intentional: creating state is expensive (prevents spam), verifying state is cheap (enables light clients).

## the minimum structure theorem

conjecture: CRDT + NMT + DAS is the MINIMUM composition that achieves verified convergence without coordination.

argument by elimination:
- remove CRDT: no convergence guarantee (multiple valid states, no resolution)
- remove NMT: no completeness guarantee (convergence on incomplete data)
- remove DAS: no availability guarantee (complete but lost data)

each removal loses a property that the other two cannot provide. no layer is redundant.

argument by coverage: every sync failure is a failure of exactly one layer:
- wrong state → merge failure (layer 5)
- incomplete state → completeness failure (layer 3)
- lost state → availability failure (layer 4)
- invalid state → validity failure (layer 1)
- reordered state → ordering failure (layer 2)

no failure falls outside these categories. the five layers cover the failure space. the three core layers (CRDT, NMT, DAS) are the irreducible minimum for the agreement/completeness/availability properties.

## verified eventual consistency

structural sync achieves a consistency model that warrants a precise definition:

```
VERIFIED EVENTUAL CONSISTENCY (VEC):

safety:     if two nodes have verified-complete signal sets S_A and S_B,
            and S_A = S_B, then state(S_A) = state(S_B)
            (CRDT convergence — deterministic merge on equal sets)

completeness: a node can verify in O(log n) whether its signal set
              for a given source is complete
              (NMT structural completeness proof)

availability: a node can verify in O(√n) whether the data underlying
              its signal set physically exists across the network
              (DAS probabilistic availability proof)

liveness:   if a signal is created and the network is eventually connected,
            every correct node eventually receives it
            (gossip + DAS reconstruction from erasure coding)
```

VEC is stronger than eventual consistency (adds verifiable completeness and availability) but weaker than strong consistency (no real-time ordering guarantee). the key property: a node does not need to TRUST that it has converged — it can VERIFY convergence locally.

this differs from existing consistency models:
- **eventual consistency**: convergence assumed, not verifiable
- **causal consistency**: ordering guaranteed, completeness not verifiable
- **strong eventual consistency** (SEC, the CRDT model): convergence guaranteed on equal message sets, but no mechanism to verify set equality
- **VEC**: convergence guaranteed (CRDT) + set equality verifiable (NMT) + data existence verifiable (DAS)

## open questions

1. **formalization**: VEC needs formal treatment as a consistency model. the safety, completeness, availability, and liveness properties above are a starting point. a full formalization should define the adversary model, prove the properties hold under stated assumptions, and establish the relationship to existing consistency models (SEC, causal+, linearizability)
2. **lower bounds**: is there a proof that three layers are necessary? or could a single mathematical structure provide all three guarantees simultaneously? [[algebraic-nmt]] with polynomial commitments may unify completeness and merge into one algebraic structure — reducing three layers to two. if completeness and merge can share a single polynomial commitment scheme, the minimum composition might be algebra+probability rather than algebra+logic+probability
3. **composability**: do structural sync protocols compose? if system A uses structural sync and system B uses structural sync, does their combination automatically inherit all guarantees? this matters for cross-graph sync (multiple knowledge graphs sharing particles) and for bbg's relationship to external chains. the NMT namespace structure suggests natural composability — each system occupies a namespace, cross-system proofs are namespace inclusion proofs
4. **sybil resistance without consensus**: consensus protocols use stake/PoW for sybil resistance. structural sync at local scale has no sybil problem (devices share one neuron). at global scale, foculus uses stake-weighted π convergence. but the general question remains: is sybil resistance possible without any form of resource commitment? VDF provides rate limiting but does not prevent identity multiplication

see [[sync]] for the protocol specification, [[foculus-vs-crdt]] for the merge layer comparison (resolves the "foculus as CRDT" question — foculus is not a CRDT but serves the same role at global scale with different tradeoffs), [[algebraic-nmt]] for the completeness layer evolution, [[data-availability explained|data-availability]] for the availability layer
