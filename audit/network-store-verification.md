---
title: network store fetch is unauthenticated; build blocked on lens drift
tags: bbg, storage, network, audit
crystal-type: entity
crystal-domain: cyber
date: 2026-09-21
status: finding
---

# Network store fetch is unauthenticated; build blocked on lens drift

Property [launch #23](../../cyber/launch.md) — "a file store answers by
particle on the three network machines, seeded from the burial" — needs
`TieredStore::fetch_content` as its on-demand path: a local storage miss
falls to `NetworkStore::fetch`, injected by cybergraph/radio. Reviewing
that path at BBG commit `450b579` (`origin/master`, 2026-09-11) surfaced
two problems, one in the code and one in this environment that blocks
verifying a fix.

## finding 1 — fetched content is never checked against the particle

[specs/storage.md](../specs/storage.md) line 155 states the L3 content
store is "self-authenticating: `H(content) = particle`";
[`storage/mod.rs`](../rs/src/storage/mod.rs) lines 16–19 place
`NetworkStore` at L3 as the transport injected by cybergraph.
[`storage/tiered.rs`](../rs/src/storage/tiered.rs) line 83 does not enforce
the claim:

```rust
pub fn fetch_content(&self, particle: &Particle) -> Option<Vec<u8>> {
    self.network.as_ref()?.fetch(particle)
}
```

`NetworkStore` is an injected trait object — untrusted transport by the
module's own doc comment ("Transport is not owned by BBG"). Whatever a
peer returns for a requested particle is handed back unchecked. A buggy
or adversarial peer can answer any fetch with arbitrary bytes and BBG
will treat them as the content of the requested particle. There is no
test exercising `fetch_content` or `NetworkStore` at all — `grep -rn
"NetworkStore" rs/src rs/tests` outside `network.rs` and `tiered.rs` hits
only the module table and re-export in `storage/mod.rs` (lines 16–19, 36);
`rs/tests` has no mention.

Separately, the module header for `tiered.rs` documents the read path as
"HOT → WARM → COLD → NETWORK (cascade...)", but `ShardStore::get`/`read`
never touch `self.network` — only the separate `fetch_content` method
does, and only for raw content bytes (`Vec<u8>`), which cannot flow
through `get`'s `Option<&[Goldilocks]>` return without a byte↔field
codec that does not exist yet. The comment describes a cascade that
isn't implemented; `fetch_content` is a second, disconnected leg.

The fix is small and does not touch the `NetworkStore` trait signature:
hash the fetched bytes with `hemera::hash` (imported at
[`storage/mod.rs:123`](../rs/src/storage/mod.rs), called at
[`state.rs:31`](../rs/src/state.rs)) and reject a mismatch as if the
peer were unreachable — `fetch_content` already returns `Option`, so no
caller-visible error type changes. The module comment should stop
claiming a cascade `get`/`read` do not implement.

## finding 2 — the crate does not build in this run's environment

Confirming the fix with `cargo check --tests` is not possible right now.
`bbg`'s `origin/master` `rs/Cargo.toml` pins `lens = { package =
"cyber-lens", version = "0.1.3", path = "../../lens/src" }`, matching
`lens`'s own `origin/master` (`src/Cargo.toml` also `0.1.3`) and `zheng`'s
`origin/master` (`rs/Cargo.toml` also requires `0.1.3`) — all three
agree on `0.1.3` as pushed.

The owner's local `~/cyber/lens` checkout, which the launch worker
mirror symlinks to for path-dependency resolution, has an uncommitted,
unpushed bump to `0.2.0` (`src/Cargo.toml`, modified since 2026-09-11
per `stat`, `git status` shows it dirty). `~/cyber/zheng/rs/Cargo.toml`
has a matching uncommitted bump to `lens = "0.2.0"`, but `~/cyber/bbg`
does not (checked with `git status --short` in each tree — read-only,
no edits made there). Building any fresh `origin/master` worktree of
`bbg` (or `zheng`, or anything path-depending on either) inside this
mirror fails version resolution:

```
$ cargo check --tests
error: failed to select a version for the requirement `cyber-lens = "^0.1.3"`
candidate versions found which didn't match: 0.2.0
location searched: /private/tmp/launch-work-c/lens/src
required by package `bbg v0.2.1 (/private/tmp/launch-work-c/bbg/rs)`
```

This is not caused by this launch worker and the fix above is not
included in this PR: the auditor-mindset rule in this repo's `CLAUDE.md`
is "never open a green-looking PR over a red run," and there is no way
to get a build here to check green or red. Once the lens version bump
lands on `origin/master` for `lens` and its dependents (or the local
`lens`/`zheng` bump is committed and pushed), `fetch_content` can be
fixed and tested in one slice: a `MockNetworkStore` returning content
that hashes to the requested particle (accepted) and content that
does not (rejected as `None`).

## update 2026-09-22

- the owner's local `~/cyber/bbg` now also carries the uncommitted
  `lens = "0.2.0"` bump (`rs/Cargo.toml` dirty), so the drift is between
  the owner's working trees and what is pushed, not inside `bbg`.
- `origin/master` `bbg` builds and tests clean in a mirror whose `lens` and
  `nox` are worktrees at their own `origin/master` (0.1.3 and 0.2.0, the
  versions `rs/Cargo.lock` pins): `cargo check --tests` ok, `cargo test`
  51 + 2 passed. The `fetch_content` fix and its tests can land against
  `origin/master` today; finding 2 is a mirror-layout problem, not a
  repository one.
- the owner's open bbg#9 (`feat/atomic-application-storage`) rewrites
  `tiered.rs`: it drops the `HOT → WARM → COLD → NETWORK` header (the
  module-doc mismatch below is resolved by it) and leaves `fetch_content`
  exactly as quoted above (finding 1 stands after bbg#9). The code fix
  should be written against bbg#9's `tiered.rs`, not master's, to avoid a
  conflict.

## remains

- land the lens 0.1.3 → 0.2.0 bump (or revert the local one) so
  `bbg`/`zheng`/`foculus` agree with what's on disk in this mirror
- `fetch_content` hash check + tests, once the crate builds again
- the module doc mismatch (`tiered.rs` header claims a HOT→WARM→COLD→NETWORK
  cascade `get`/`read` don't implement)
- no byte↔field codec exists to route network-fetched content through the
  `ShardStore` cascade at all; `fetch_content` staying a separate,
  Vec<u8>-typed leg is the smaller change, not a gap to close here
