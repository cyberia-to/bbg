---
tags: cyber, bbg
crystal-type: spec
crystal-domain: cyber
---
# sync

the structural sync protocol is a cross-cutting spec spanning [[radio]], [[cybergraph]], [[bbg]], [[cyb/sync]], and [[foculus]].

see [[cybergraph/specs/structural-sync]] for the full protocol.

## bbg's role in sync

bbg is a pure state library. it has no network code and no sync protocol.

what bbg provides to the sync protocol:

**layer 3 (completeness):** Lens polynomial commitments. each neuron commits its signal chain; bbg commits the finalized signals dimension of BBG_poly. withholding is algebraically detectable.

**query wire:** `prove(key, dimension) → QueryProof`. cybergraph serves these proofs over the query wire protocol. bbg builds them.

**BBG_root:** the checkpoint used by the local sync protocol and the light client. `BBG_root = H(Lens.commit(BBG_poly) ‖ Lens.commit(A) ‖ Lens.commit(N))`.

**signal finalization:** after cybergraph validates a signal (layers 1–2) it calls `bbg.apply(tx)`. bbg applies the state transition and recomputes BBG_root.

what bbg does NOT own: signal structure, hash chains, equivocation detection, VDF, CRDT merge, transport. those live in cybergraph and cyb/sync.
