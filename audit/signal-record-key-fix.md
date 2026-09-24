# signals dimension: fix the same-step key collision

date: 2026-09-24 · revision: a3c9a0c (origin/master) · property 9 (nodes
converge from different starting states)

## the bug

`BbgState::signals` was `BTreeMap<u64, SignalRecord>`, keyed by the
neuron's own per-chain `step` counter. Every neuron's *first* signal is
step 0, so two different neurons' first signals collided on that key:
`apply_signal_record` silently overwrote whichever record arrived
first with whichever arrived second, and `commit_signals`/`dim_entries`
folded only the survivor into the root. Two nodes receiving the same two
neurons' first signals in opposite order held a different survivor each,
so `BbgState::root()` diverged — a direct violation of property 9.

## the fix

`rs/src/state.rs` adds `signal_key(neuron, step) -> Particle`
(`H(neuron ‖ step)`, the same construction as `axon_id`/`balance_key`).
`signals` is now keyed by this hash. `SignalRecord` gains a `step: u64`
field so the human-readable step survives once the key itself is opaque;
`apply_signal_record`'s `step` argument is still the source of truth
(`SignalRecord { step, ..record }`). `commit_signals` (`state/commits.rs`)
and `dim_entries`'s `Dim::Signals` arm (`proof.rs`) both fold `v.step`
into the committed values, appended after the existing fields so no
existing value offset shifts.

`bbg_query`'s `Dim::Signals` arm now returns `None`, matching the
existing `Balances` precedent (`query.rs`'s own doc comment already
documented that pattern for a two-input key) — callers use
`prove_signal(state, neuron, step)` directly. `prove_signal` gained a
`neuron: &NeuronId` parameter for the same reason.

## verified

```
$ cargo test --offline --test signal_record_key
running 2 tests
test signal_key_binds_neuron_and_step ... ok
test two_neurons_first_signal_both_survive_regardless_of_arrival_order ... ok
test result: ok. 2 passed; 0 failed

$ cargo test --offline --lib
test result: ok. 54 passed; 0 failed
```

`cargo test --offline` (workspace, all targets) also runs `tests/look_e2e.rs`,
which fails on plain `origin/master` too, before and independent of this
change: `look_proof_verifies_against_state_root` panics with
`UnsupportedRecursiveOpening` once the workspace's sibling pins are bumped
to build at all (`lens` 0.1.3→0.2.0, `nox` 0.2→0.3.0, `zheng` 0.3→0.4.0 —
this checkout does not resolve on plain `origin/master`, the same
pre-existing gap `bbg#12`/row 39 documents). Pins were bumped locally only
to compile and run the tests above, then `Cargo.toml`/`Cargo.lock` reverted
before this commit — `git diff --stat origin/master` for this branch
touches only `rs/src/{lib,proof,query,state,state/commits,types}.rs`,
`rs/tests/signal_record_key.rs` and this file.

## remains

`cybergraph::api::commit_signal` and two `bbg::SignalRecord { .. }`
literals (`cybergraph/src/api.rs:298`, `cybergraph/src/native/commit.rs:63`)
need a `step: <value>` field added; `cybergraph/src/source.rs:81` and
`cybergraph/src/api.rs:502,510` and `inf/rs/source/src/bbg.rs:104` read
`state.signals` keyed by the old bare `step` and need to read `v.step`
instead of the map key. None of these are load-bearing for `apply_signal_record`
itself — its signature is unchanged — so bbg builds and tests green on its
own; cybergraph and inf need a follow-up PR once they resolve against this
bbg revision.
