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
