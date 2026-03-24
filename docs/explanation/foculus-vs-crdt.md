---
tags: cyber
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0018976628199509436
heat: 0.0013346945323019417
focus: 0.0008898495767886562
gravity: 0
density: 0.74
---
# foculus as merge layer

the five verification layers of [[sync]] share four layers between local sync and global network sync: validity ([[zheng]] proof), ordering (hash chain + VDF), completeness (per-neuron signal polynomial commitment), availability (DAS + erasure coding). the fifth layer — merge — is where local and global diverge. local sync uses a CRDT. global sync uses [[foculus]]. understanding why illuminates both.

## what the merge layer does

four layers guarantee that signals are valid, ordered, complete, and available. the merge layer answers the remaining question: when two valid, ordered, complete, available signals conflict — which one wins?

a conflict is two signals that cannot both be true. two neurons rename `~/paper.md` to different CIDs. two neurons spend the same token output. two cyberlinks claim the same unique resource. the signals are individually valid. both have zheng proofs. both are ordered in their respective chains. the conflict is semantic, not structural.

## CRDT: merge by structure

a CRDT (Conflict-free Replicated Data Type) resolves conflicts by algebraic properties of the data type. a G-Set (grow-only set) merges by union — all elements from all replicas are included. a LWW-Register (last-writer-wins) merges by timestamp — the most recent write wins.

for local sync, CRDTs work because:

- **the trust model is simple.** all neurons in the local group share one identity. there is no adversarial stake. a compromised neuron is revoked, not outvoted
- **content is content-addressed.** file blobs identified by CID have no conflicts — the G-Set union is the correct merge. two neurons adding the same file get the same CID. deduplication is automatic
- **name conflicts are rare.** concurrent name updates (two neurons editing the same binding offline) are the exception. deterministic ordering (hash tiebreak on concurrent signals) resolves them without semantic reasoning

CRDTs are sufficient for local merge because the failure modes they cannot handle (withholding, availability) are covered by other layers (PCS, DAS).

## foculus: merge by convergence

at global scale, CRDT merge breaks down. the problem is not technical — it is economic.

a CRDT merges everything. a G-Set union includes all elements from all replicas. this means every valid signal enters the canonical state. there is no mechanism to exclude a valid-but-unwanted signal — a spam signal, a low-quality cyberlink, a transaction that conflicts with another.

[[foculus]] replaces structural merge with convergent merge. the merge function is not union — it is $\pi$ convergence. the stationary distribution $\pi$ of a token-weighted [[random walk]] on the [[cybergraph]] assigns every [[particle]] a score. a particle is final when $\pi_i > \tau(t)$. conflicting particles compete for the same finite $\pi$ mass pool — only one crosses the threshold.

this works at global scale because:

- **stake makes conflicts expensive.** manipulating $\pi$ requires controlling cybergraph topology, which costs real tokens. a CRDT merge costs nothing — any replica can inject data
- **exclusive support splits mass.** when a neuron detects conflicting particles, it links to exactly one. the unsupported member receives zero $\pi$ from that neuron. conflicting particles cannot both accumulate majority mass
- **convergence is deterministic.** every neuron computes the same $\pi$ from the same graph. no coordination needed. the ordering emerges from the topology of attention — not from voting, not from timestamps, not from algebraic merge rules

## the unification

| property | local (CRDT) | global (foculus) |
|---|---|---|
| merge function | set union / deterministic tiebreak | $\pi$ convergence |
| conflict cost | free (any local neuron can create) | expensive (requires stake) |
| finality | immediate (deterministic order) | 1-3s ($\pi_i > \tau$) |
| trust model | same identity, local neurons trusted | adversarial, stake-weighted |
| what enters state | everything valid | only what crosses $\tau$ |

the other four layers are identical:

| layer | local | global |
|---|---|---|
| validity | zheng proof per signal | zheng proof per signal |
| ordering | prev chain + VDF | prev chain + VDF |
| completeness | per-neuron signal polynomial commitment | per-neuron signal polynomial commitment |
| availability | DAS + erasure coding | DAS + erasure coding |

the unit is always neuron. at local scale, a small group of neurons (1-20, same identity) syncs via CRDT. at global scale, neurons (10^3-10^9, different identities) converge via foculus. the same signal structure — $(\nu, \vec\ell, \pi_\Delta, \sigma)$ with ordering fields (prev, merkle_clock, vdf_proof, step) — works at both scales. the only difference is which merge function resolves conflicts.

## why not CRDT at global scale

a G-Set has no exclusion. once an element is added, it cannot be removed. this is a feature for content (CIDs should never disappear) but a failure for state (conflicting transactions must not both execute).

a LWW-Register has timestamps. timestamps require clocks. clocks require trust. VDF provides physical time without trust — but VDF ordering is partial (concurrent signals have no VDF relationship). $\pi$ convergence provides total ordering through topology, not time.

CRDTs also have no quality filter. every valid signal merges equally. foculus provides a natural quality filter: signals that attract more stake-weighted attention (cyberlinks) accumulate more $\pi$. the merge function itself is an attention mechanism — the network collectively decides what matters.

## why not foculus at local scale

foculus requires stake, adversarial assumptions, and convergence time. a laptop syncing with a phone needs none of this. the local neurons share one identity. there is no adversary to outweigh. deterministic ordering (causal -> VDF -> hash tiebreak) provides immediate finality without $\pi$ iteration.

the CRDT merge is O(1) — union of sets. foculus merge is O(iterations x edges) — sparse matrix-vector multiply until convergence. for 3 local neurons syncing files, the CRDT is the right tool. for 10^6 neurons reaching consensus on the cybergraph, foculus is the right tool.

## the spectrum

local sync and global consensus are not separate protocols with a bridge between them. they are the same protocol with a different merge layer. a signal created on a neuron, synced to other local neurons via CRDT merge, and submitted to the network via foculus merge — traverses the full spectrum without transformation. the canonical signal fields ($\nu, \vec\ell, \pi_\Delta, \sigma$) are unchanged. the ordering fields (prev, merkle_clock, vdf_proof, step) serve both scales. only the merge semantics change at the boundary between private sync and public consensus.

see [[sync]] for the full sync specification, [[foculus]] for consensus details, [[design-principles]] for the three laws