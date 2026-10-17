---
title: BBG audits
tags: bbg, audit
crystal-type: entity
crystal-domain: cyber
---

# BBG audits

Implementation reviews and executable validation evidence live here.
The storage contract lives in [specs/storage.md](../specs/storage.md).

- [Persistence](persistence.md) — RAM, fjall, redb, tier routing and the
  initial Cybergraph/Cyber integration boundary review.
- [Fjall batch journal-write repair](fjall-batch-write.md) — vendored 2.11.2
  error-path fix, source provenance and partial-journal regression evidence.
- [Durable shard storage](durable-shard-storage.md) — implemented fallible
  contract, atomic backend commits, recovery and tier failure tests; remaining
  archive and node integration work.
- [Shared application and shard database](shared-database.md) — one transaction
  owner, Fjall working storage, combined commits and explicit legacy redb import.
- [Commit cost, launch row 43](commit-cost-2026-10-15.md) — flat recommit vs
  incremental keyed-trie commitments, 10k–1M links and the pussy replay.
- [Dimension version 2](dimension-version-2-2026-10-15.md) — the bostrom
  genesis root unblocked by the trie default (eager trie: 1,790 s, 8.57 GB),
  the two inflations measured, the bbg-only bostrom link replay; superseded as
  a rule set by version 3, kept as evidence.
- [Dimension version 3](dimension-version-3-2026-10-16.md) — the root chain's
  genesis version (nonce, versioned trie leaves, no zero rows), deferred
  hashing, chunked Brakedown measured and rejected, both burials committed.
- [Signals key, dimension version 3 rule 7](signals-key-2026-10-17.md) —
  launch row 9: signals keyed by H(neuron ‖ step), the step-0 collision and
  the acceptance-order dependence removed, no blessed root moved.
