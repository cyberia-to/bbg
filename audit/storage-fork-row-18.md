---
title: two unmerged storage architectures both claim row 18
tags: bbg, storage, audit, launch
crystal-type: entity
crystal-domain: cyber
date: 2026-09-23
status: found-in-review
---

# two unmerged storage architectures both claim row 18

Property #18 — durable storage survives failure and restart — has two
independent, unmerged implementations open against `bbg` right now, neither
aware of the other, both touching the same tier-routing files. Whichever
merges first forces a nontrivial rebase of the other.

## the two branches

1. [bbg#9](https://github.com/cyberia-to/bbg/pull/9) "share atomic durable
   storage across graph applications" — branch `feat/atomic-application-storage`,
   opened 2026-09-12, last updated 2026-09-23T12:03:38Z, +25214/-1415,
   `mergeable: MERGEABLE`. Introduces a new `rs/src/storage/database/` module
   (`backend.rs`, `backend_fjall.rs`, `backend_redb.rs`, `generation.rs`,
   `records.rs`, `transaction.rs`), a new `rs/src/storage/application/`
   submodule, and `rs/src/storage/disk.rs`, while also rewriting
   `rs/src/storage/tiered.rs` and `rs/src/storage/mod.rs` in place. Its head
   commit `011e5ab` is documented in `audit/durable-shard-storage.md` (dated
   2026-09-11, status `component-validation`), which cites test targets
   `rs/tests/storage_contract.rs`, `storage_persistence.rs`,
   `storage_tier_failures.rs` and `rs/src/storage/redb_tests.rs` — none of
   which exist on `origin/master` today. `011e5ab` is not an ancestor of
   `origin/master` (`git merge-base --is-ancestor 011e5ab origin/master` →
   false); it lives only on `feat/atomic-application-storage`,
   `chore/coordinated-release-20260916` and `fix/node-storage-dependency`.
   In other words: the audit that reports row 18's D1 gate passing describes
   a branch, not the tree the release train actually builds.

2. The launch-worker row-18 series —
   [bbg#10](https://github.com/cyberia-to/bbg/pull/10),
   [bbg#24](https://github.com/cyberia-to/bbg/pull/24),
   [bbg#25](https://github.com/cyberia-to/bbg/pull/25),
   [bbg#26](https://github.com/cyberia-to/bbg/pull/26), opened
   2026-09-19 through 2026-09-23 — build incrementally on `origin/master`'s
   current, simpler `rs/src/storage/tiered.rs` (345 lines, no `database/`
   module, no atomic commit marker, no fault-injection tests): WARM archival
   population, the checkpoint boundary and a demote/evict schedule
   (`rs/src/storage/schedule.rs`, new in bbg#26). None of these four PRs
   touch `rs/src/storage/database/` or `application/` — they do not know
   bbg#9 exists.

## the conflict

```
$ git diff --stat origin/master origin/feat/atomic-application-storage -- rs/src/storage/tiered.rs rs/src/storage/mod.rs
 rs/src/storage/mod.rs    | 154 ++++++++++++---
 rs/src/storage/tiered.rs | 481 +++++++++++++++++++++--------------------------
 2 files changed, 347 insertions(+), 288 deletions(-)
```

`tiered.rs` differs by 481 changed lines out of 345 on master — close to a
full rewrite, not an additive change. bbg#10/#24/#25/#26 also edit
`tiered.rs` and `mod.rs` (WARM population, demote/evict) against the
pre-bbg#9 shape of those files. Merging bbg#9 first orphans the WARM
archival work's file-level basis; merging the launch series first means
bbg#9 — sitting for 11+ days, +25k lines, with the only fault-injection
test suite recorded for row 18 — still has to be reconciled against
whatever the launch series left behind.

## what this means for row 18

The registry's evidence line for row 18 cites `bbg/specs/storage.md`
(D2 population and checkpoint boundary, launch #18) and `bbg#10`. It does
not cite bbg#9 or `audit/durable-shard-storage.md`, but that audit is the
only recorded evidence anywhere in this repo that D1 (fallible atomic
commits, reopened-read regression coverage, fault injection) was ever
implemented and tested — and it is stuck on an unmerged branch. Row 18's
close-by date (2026-10-09) is at risk twice over: once on the launch
series' own progress, and independently on an 11-day-old, 25k-line PR nobody
in the launch-worker pipeline has touched.

## recommendation

An owner decision on merge order is needed before more work lands on either
side: either bbg#9 merges first and the launch series' `tiered.rs` changes
get rebased onto `database/`, or bbg#9 is rebased onto master past the
launch series and its audit is re-run against the result. No code in this
PR takes a side; it only establishes the fork exists and where.

Verified (read-only, no source changed):
```
$ git merge-base --is-ancestor 011e5ab origin/master ; echo $?
1
$ git branch -r --contains 011e5ab
  origin/chore/coordinated-release-20260916
  origin/feat/atomic-application-storage
  origin/fix/node-storage-dependency
$ git diff --stat origin/master origin/feat/atomic-application-storage -- rs/src/storage/tiered.rs rs/src/storage/mod.rs
 rs/src/storage/mod.rs    | 154 ++++++++++++---
 rs/src/storage/tiered.rs | 481 +++++++++++++++++++++--------------------------
 2 files changed, 347 insertions(+), 288 deletions(-)
$ cargo test --manifest-path rs/Cargo.toml --locked --lib storage::
error: failed to select a version for the requirement `cyber-lens = "^0.1.3"`
candidate versions found which didn't match: 0.2.0
```
`origin/master` does not build at all right now with a local `lens` checkout
at 0.2.0 — this is the pin break row 39 already tracks (bbg#12, open,
unmerged): `cyber-lens = "^0.1.3"` in `rs/Cargo.toml` excludes the 0.2.0 that
is actually checked out. No storage test, on either branch, can be executed
against `origin/master` until that pin lands. This PR changes only
`audit/storage-fork-row-18.md`; it does not attempt the pin fix, which is
bbg#12's job and named in bbg's CLAUDE.md as a do-not-touch-without-discussion
zone (`Cargo.toml` dependency versions).
