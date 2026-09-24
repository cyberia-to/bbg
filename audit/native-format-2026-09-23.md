---
title: Native record format boundary
tags: bbg, audit, storage
date: 2026-09-23
status: withdrawn
---

> Recorded from cyberia-to/bbg#22 (branch `fix/node-storage-dependency`), closed 2026-09-24 as superseded by #9; evidence preserved as written.
# Native record format boundary

Withdrawn on 2026-09-23 following the owner's correction: the project is
pre-production and these unchanged root semantics do not justify a migration
framework or a new reader exclusion. The version bump, new metadata API and
associated contract were reverted. The existing record layout is retained.
The following text preserves the experiment's original evidence, not policy.

Native metadata now emits version 2, identifying its typed layout and the
current dimension/commitment semantics. The old version 1 was reused across
root changes and therefore cannot identify a commitment generation by itself.
`native_metadata_version` validates framing and distinguishes malformed metadata
from an unsupported version before an adapter replays history.

The layout following the version word and all root computations are unchanged.
Cybergraph owns conditional legacy acceptance: full replay and exact comparison
must succeed before a legacy store becomes usable. Its first newly accepted
operation promotes the version in the same transaction; read-only open and
idempotent retry preserve the old version. Incompatible roots are preserved and
refused. No automatic rewrite of historical commitments is provided.

Validation: all-feature BBG suite passed 166 tests with zero ignored tests.
The new framing test covers versions 1/2, unsupported versions, every truncated
prefix, trailing bytes and both diameter encodings. The existing transition,
storage, migration and failure suites also passed. Three existing vendored
Fjall warnings remain.

The companion [Cybergraph audit](../../cybergraph/audit/native-format-2026-09-23.md)
records replay, atomic promotion, failure injection and previous-binary evidence.
Version-1 history predating the pinned node's root semantics still requires
separate explicit recovery; a successful modern-root replay is the acceptance
criterion, not a blanket claim for every former version-1 writer.
