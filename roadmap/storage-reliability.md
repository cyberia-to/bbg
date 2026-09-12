---
title: durable storage for the Cyber node
tags: bbg, cybergraph, cyber, roadmap, storage
crystal-type: plan
crystal-domain: cyber
status: active
priority: P0
date: 2026-09-11
---

# P0 — durable storage for the Cyber node

Highest BBG delivery priority, assigned by the owner on 2026-09-11.
The Cyber node's reliability depends on completing BBG's existing disk storage
and exposing it through Cybergraph. This work blocks
[Cyber A1](../../cyber/roadmap/a-local-node.md#a1-complete-the-existing-storage-contract)
and the dependent local-node acceptance gates.

BBG owns the persistence API, backend transactions and tier routing.
Cybergraph owns validated graph transitions and their durable publication.
Cyber/Soft3 owns host integration and migration of existing node homes.
Reuse Fjall, redb and the existing application transaction machinery where
their semantics fit. Keep one authoritative commit path per history domain.

## delivery invariant

Every operation acknowledged as durably accepted must survive restart with
the same history position, resulting state and request receipt. A retry of
the same request returns its original result; a conflicting payload fails.
An uncertain commit outcome remains explicit until recovery resolves it.
Local durability and network finality retain separate meanings.

The [persistence audit](../audit/persistence.md) supplies the starting evidence.
The presence of a backend, successful cache access or a change-set hash alone
does not establish this invariant.

D1 is implemented, and the local RAM/Fjall profile completes D3 and D4.
Executed component tests
and their limits are recorded in the
[durable shard storage audit](../audit/durable-shard-storage.md). D2 archival
recovery and the remaining D5 disk/power-loss evidence keep this P0 active.
The [native acceptance audit](../../cyber/audit/native-acceptance.md) records
the real-binary integration and its supported local scope.

## ordered work

### D1: fallible durable storage contract

Owner: BBG. Implemented; see the linked audit for component validation.

- [x] Specify disk reads, bounded scans, write/delete batches, commit outcomes
  and recovery in [storage](../specs/storage.md), then update implementations
  and supported callers together.
- [x] Distinguish absent data, malformed encoding, I/O failure and unknown commit
  outcome. Read and iterate persisted values after reopening with an empty cache.
- [x] Propagate write/flush errors. Preserve the pending transaction and its
  recovery identity until commit success or explicit outcome resolution.
- [x] Batch related updates and deletions atomically within the selected backend;
  define the durability barrier and the receipt returned after it succeeds.

Exit: the shared interface, exercised against Fjall and redb, restores committed
values and deletions after reopen and reports injected read/write/flush failures.
Canonical decoding rejects malformed and noncanonical values. Recovery never
turns an unresolved write into an acknowledged success or a silently lost update.

### D2: tier consistency and recovery

Owner: BBG. Depends on D1.

- [x] Carry the existing HOT mutation and last-copy eviction repairs through
  the fallible API, including failure during commit and retry.
- [ ] Define WARM/COLD population, archival progress and recovery boundaries.
  Keep the last required copy until the destination's durability is established.
- [x] Keep EPHEMERAL local to memory. Specify the selected durable backend,
  participating tiers and format/version identity for each supported profile.

Exit: interruption during tier movement preserves an authoritative copy;
recovery returns one logical value for each key. Each supported profile states
which durable tier acknowledges acceptance and how archive progress is resumed.

### D3: native graph publication

Owners: BBG and Cybergraph. The local SSD profile depends on D1 and the
single-authority D2 profile; multi-tier archive qualification remains in D2.

- [x] Expose a BBG-owned atomic boundary for accepted native signal bytes,
  chain position, derived state/head and stable request receipt. A replay-based
  design must bind the authoritative history and recoverable state position.
- [x] Reuse [application transactions](../specs/application-storage.md) for local
  application history. Preserve their distinction from neuron SignalChain
  positions, native authorization and network finality.
- [x] Publish the new in-memory head only under the specified commit outcome;
  block dependent acceptance while an unknown outcome is being resolved.

The shared [Database owner](../specs/database.md) provides one atomic boundary
for application receipts/history and explicit shard changes. Working application
storage selects Fjall, and legacy redb application stores have explicit import.
The [native state preparation](../specs/native-state.md) and
[Cybergraph coordinator](../../cybergraph/specs/native-storage.md) extend this
boundary to complete native operations, exact state, economics, globally
indexed signal history and original request receipts. Preparation restores
its touched records on failure and publishes only after the common commit.

Exit: a new process restores the accepted history and independently recomputes
the same state root. Lost replies and repeated requests produce one accepted
operation. Conflicting requests and competing writers have explicit outcomes.

### D4: node integration and existing homes

Owners: Cybergraph, Cyber and Soft3. Depends on D3.

- [x] Route the node's live state and receipts through Cybergraph/BBG.
- [x] Replace the host's independent journal ownership with this path and
  provide explicit import/recovery for existing development homes.
- [x] Propagate storage failure and recovery status to node readiness and callers.

Exit: the real Cyber binary uses the same durable path exercised by BBG and
Cybergraph tests. Import preserves valid history and reports truncation or
corruption explicitly. Node readiness waits for successful recovery.

### D5: failure acceptance evidence

Owners: BBG for backend tests; Cybergraph/Cyber for integration. Test each
preceding task as it lands; close this gate after D4.

- [x] Exercise actual Fjall and redb files in subprocess kill/restart tests
  after staging and after commit returns, including deletes and multi-record
  writes. Native client acknowledgement testing remains part of the node gate.
- [ ] Cover disk-full/write/flush failures, lost replies, identical/conflicting
  retries, malformed/truncated data and exclusive-writer conflicts.
- [ ] Record filesystem, operating system and durability-barrier assumptions;
  distinguish process crashes, injected faults and power-loss validation.
- [ ] Run the same accepted-operation recovery scenario against the pinned node
  binary. Store commands, source/artifact identities and results in `audit/`.

Exit: evidence covers every claimed guarantee of each supported storage profile.
All D1–D5 gates must close before this P0 is complete. Passing backend tests alone
closes only the corresponding component checks; node reliability requires D4
and the end-to-end failure evidence.
