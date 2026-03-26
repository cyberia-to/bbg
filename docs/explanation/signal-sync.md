---
tags: cyber
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.00007019991600688145
heat: 0.00003419142694206788
focus: 0.00008151008453347325
gravity: 0
density: 0
---
# why signal sync works

the file sync problem has been solved badly for decades. Syncthing shows conflict files. Nextcloud requires a server. Dropbox is centralized. IPFS has the right primitives but never delivered practical sync. the common failure: they treat sync as a data replication problem. it is an ordering problem.

## the ordering problem

content-addressed data doesn't conflict. if two neurons store the same file, it gets the same CID. deduplication is automatic. transfer is trivial: "do you have this hash? no? here." this is a solved problem (IPFS, BitTorrent, git objects).

the unsolved problem: mutable name resolution. when two neurons both rename `~/paper.md` to different content while offline, which binding wins? this is the question every sync tool answers differently:

- Syncthing: creates conflict copies, asks the user
- Dropbox: last-write-wins using server timestamps
- Willow: last-write-wins using local timestamps (breaks with clock skew)
- CRDTs: merge automatically but cannot express "replace" semantics cleanly

bbg's answer: **deterministic ordering of a signal DAG.** no timestamps, no server, no conflict files. every neuron independently computes the same total order and arrives at the same state.

## why signals, not files

the unit of sync is not a file. it is a signal — a batch of cyberlinks with a zheng proof. a file write is a cyberlink (bind name to new CID). a file delete is a cyberlink (remove name binding). a rename is two cyberlinks. every file operation is already a signal in bbg's architecture.

this means file sync is not a separate protocol. it is the same mechanism the network uses for all state transitions. the sync protocol for files, for knowledge graph operations, for token transfers, for card updates — is one protocol. signals in, state out.

## why VDF matters in the age of agents

the traditional argument for VDF in sync is "replace trusted clocks with physics." this is true but insufficient motivation for personal neurons that you control.

the real argument: **you do not control your neurons.** an LLM agent running on your laptop is Byzantine. it optimizes for its objective, not yours. a compromised npm package is Byzantine. a browser extension with too many permissions is Byzantine. the attack surface on a personal neuron is not "malicious remote attacker" — it is "software running locally with your credentials."

VDF provides a physical rate limit that no software can bypass. a signal requires T_min sequential computation. an agent trying to flood the DAG with garbage cyberlinks is physically bounded — it cannot create signals faster than the VDF allows, regardless of how many cores or how much memory it has. this is not a software rate limit (bypassable) or a reputation system (gameable). it is physics.

the same property makes equivocation expensive. producing two conflicting signals from the same predecessor requires computing the VDF twice. the total elapsed VDF time exceeds wall-clock time — detectable by any peer. a compromised process cannot fork the signal chain faster than honest neurons can detect it.

## how structural security replaces BFT

traditional distributed systems handle Byzantine faults with protocol: 2/3 majority, rounds of voting, leader election, view changes. this requires a quorum (breaks with 2 out of 3 laptops closed), a stable leader (breaks with intermittent connectivity), and O(n^2) message complexity (unnecessary for 1-20 neurons).

bbg eliminates each fault class with a data structure property:

**forging** — every signal carries a zheng proof. the proof is ~40,000 constraints verifiable in ~5 μs. an invalid state transition (overspending focus, creating energy from nothing, corrupting a polynomial commitment) cannot produce a valid proof. this is not "the network rejects invalid signals" — it is "invalid signals cannot be constructed." the constraint system is the security boundary, not the protocol.

**equivocation** — each neuron chains its signals via `prev = H(previous_signal)`. two signals with the same `prev` is a cryptographic proof of double-signaling. detection is O(1) — compare `prev` fields. no protocol rounds needed. VDF adds physical cost: equivocating requires computing VDF twice, which takes twice the wall time. the extra time is measurable.

**reordering** — the hash chain makes neuron-local history immutable. inserting, removing, or reordering a signal breaks the chain (subsequent hashes no longer verify). any peer walking the chain detects the break in O(n) where n is chain length.

**withholding** — each neuron commits its signal chain to a per-neuron polynomial commitment (evaluation points indexed by step). a peer requesting "all signals in steps [100, 200]" gets a Lens completeness proof — polynomial binding prevents omission. the neuron cannot hide signals in the requested range. content availability is verified via DAS — erasure-coded chunks sampled at O(√n) cost. withholding is not self-punishing — it is algebraically detectable.

the result: no leader election, no quorum, no voting. a single neuron online is a fully functional system. any two neurons can sync bilaterally. any subset works. this is exactly the topology that personal infrastructure needs.

## five verification layers

no single layer is sufficient. each eliminates a different failure mode:

| layer | mechanism | eliminates |
|---|---|---|
| validity | zheng proof per signal | forging (invalid operations) |
| ordering | hash chain + VDF | reordering, equivocation, flooding |
| completeness | per-neuron polynomial commitment | signal withholding |
| availability | DAS + erasure coding | data loss from neuron failure |
| merge | CRDT (G-Set) | content conflicts |

together: **provably valid, provably ordered, provably complete, provably available, correctly merged.** that is extremely reliable sync.

## steps and snapshots

a step is a logical clock tick. steps serve three purposes:

**progress.** a step with cyberlinks means the neuron did work. the step counter is a measure of productive activity, not elapsed time (VDF handles time).

**liveness.** a heartbeat step (empty signal) proves the neuron is alive and has compute capacity. the VDF proof in the heartbeat is the liveness attestation — the neuron physically performed sequential computation. no heartbeat for a period means the neuron is offline.

**snapshots.** every K steps, the neuron emits a snapshot signal containing the BBG_root at that point. when a neuron reconnects after a long offline period, it finds the most recent common snapshot and replays only signals after it. without snapshots, reconnection requires replaying all signals from genesis. with snapshots, it requires replaying signals since the last common snapshot. this is the same trick that `time.root` uses for the network — periodic state commitments that enable fast catch-up.

the key: steps do not tick on their own. no cyberlinks and no heartbeat means no step. a neuron produces exactly as many steps as it has work or liveness attestations. there is no "empty block" waste.

## the composed protocol in practice

a user has a laptop, a phone, and a home server. the laptop is used during the day. the phone is used intermittently. the server is always on.

the server emits heartbeat steps continuously (high-capacity, always-on). the laptop creates cyberlink steps during work hours and heartbeat steps otherwise. the phone creates occasional cyberlink steps.

all three maintain independent signal chains with VDF proofs. when the laptop comes home and connects to the server, they compare Merkle clock roots (one hash comparison). if different, they walk the DAG to find divergence and exchange missing signals. each signal is verified independently (zheng proof, hash chain, VDF). the deterministic ordering integrates all signals into a single timeline. both neurons arrive at identical state.

the phone connects later. same protocol. it finds the laptop and server already in sync, downloads their signals, verifies, replays. three neurons, identical state, no server-centric architecture, no timestamp trust.

content (file blobs) syncs as three composed layers: CRDT merge (grow-only set of CIDs), Lens completeness proofs (provably all content received), and DAS availability (erasure-coded chunks survive neuron failure).

## why DAS, not just CRDTs

a CRDT guarantees convergence if all updates are delivered. delivery is assumed, not proven. a G-Set merge on incomplete data converges — to the wrong state.

DAS adds the guarantee CRDTs lack: **provable completeness.** content chunks are erasure-coded across neurons (2D Reed-Solomon). any neuron can sample O(√n) random chunks to verify that all data is available. the Lens commits each neuron's content set — a peer requests a polynomial opening and knows algebraically that nothing was withheld.

for personal sync this matters because:

- a compromised neuron might selectively withhold files. CRDT: undetectable. DAS + Lens: algebraically impossible.
- a neuron dies permanently. CRDT: data on that neuron is lost. DAS: erasure coding across the neuron set means any k-of-n surviving neurons reconstruct everything.
- a phone wants to verify all files exist without downloading them. CRDT: must download all. DAS: O(√n) samples for 99.9% confidence.

the three layers are orthogonal: CRDT handles merge semantics (no conflicts for content), Lens handles completeness (no withholding), DAS handles availability (no data loss). each solves a different failure mode. together they provide **extremely reliable sync** — provably complete, provably available, correctly merged.

see [[sync]] for the full specification, [[design-principles]] for the three laws, [[data-availability]] for DAS in bbg