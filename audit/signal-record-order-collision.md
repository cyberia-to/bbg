---
tags: bbg, audit, launch
date: 2026-09-24
---
# signals-dimension order collision (property #9)

[launch.md](../../cyber/launch.md) property #9 requires nodes to converge on
the same committed state from different arrival orders. `BbgState::insert`
(`rs/src/state.rs`) applies cyberlinks into `particles`, keyed by
`axon_id(from, to)` — a content hash, so insertion order does not matter and
the graph itself converges.

`BbgState::apply_signal_record` is a separate call `cybergraph::Cybergraph::
commit_signal` makes right after `insert`, recording the signal's header into
`signals: BTreeMap<u64, SignalRecord>` keyed by `step` alone. `step` is a
per-neuron chain counter (`cybergraph::SignalChain`, one chain per
`NeuronId`), not a network-wide one, so every neuron's *first* signal is
`step = 0`. Two different neurons' first signals collide on that key: the
later `insert` on the `BTreeMap` silently overwrites the earlier record, and
which one survives depends on arrival order.

`tests/signal_record_order.rs` demonstrates this directly on `BbgState`
(no cybergraph dependency needed to reproduce it): two nodes receive the same
two neurons' first signals in opposite order. Both nodes agree on `particles`
(both axons present, both ways) but `signals.get(&0)` holds a different
neuron's record on each node, and `root()` — which folds `signals` in
(`commit_signals`, `state.rs:165`) — diverges.

Verified, in a worktree off `origin/master` (a3c9a0c):
```
$ cargo test --test signal_record_order -- --nocapture
test same_step_signals_from_different_neurons_collide_in_the_signals_dimension ... ok
$ cargo test
test result: ok. 54 passed (lib) + 1 passed (signal_record_order) + existing integration suites, 0 failed
```
This checkout's own `Cargo.toml` does not resolve against current sibling
checkouts independent of row 39's pin bump (`lens` 0.1.3 → 0.2.0, `nox` 0.2 →
0.3.0 — the same pre-existing gap bbg#12 documents): pins were bumped
locally only to compile and run the above, then `Cargo.toml`/`Cargo.lock`
reverted before this commit (confirmed red again on plain `origin/master`
after reverting, same version-selection error bbg#12 reports).

Remains: the fix. `signals` needs a key that a network-wide counter or a
`(NeuronId, step)` tuple provides instead of the bare per-neuron `step` —
a `BbgState` schema change with call-site updates in `cybergraph::api::
commit_signal` and every reader of `state.signals` (`rs/src/transition/
records.rs`, `rs/src/state/commits.rs`, `rs/src/proof.rs`,
`rs/src/transition/undo.rs`, `cybergraph/src/source.rs`,
`inf/rs/source/src/bbg.rs`), out of scope for this measurement-only slice.

Risk: additive only — one new test file, one new audit doc. No `src/`
touched. Revert is a clean single-commit revert.
