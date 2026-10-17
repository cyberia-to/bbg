---
tags: bbg, audit, launch
crystal-type: audit
crystal-domain: cyber
---
# signals key — dimension version 3 rule 7, launch row 9

Date: 2026-10-17. Scope: bbg `launch/9-signals-key-v3` on `origin/master`
`1db1db6` (dimension version 3). Re-implements the fix proposed in bbg#32
(branch `launch/9-signal-record-key`, written against the flat-commitment
state before #43) on the keyed-trie state; bbg#31 holds the original
demonstration (`refs/pull/31/head`). Decision: `cyber/launch.md`, 2026-10-17 —
everything may change until 2026-11-02, the signals key lands in dimension
version 3. Machine: M4 Max, shared with other agents' builds.

## the defect

`BbgState::signals` was `Keyed<u64, SignalRecord>` keyed by the step a
caller passed in:

- `Cybergraph::commit_signal` passed the neuron's own signal-chain step.
  Every neuron's first signal is step 0, so two neurons' first signals shared
  one record; the later insert replaced the earlier one, and which record
  survived (and the signals leaf, and the root) depended on arrival order.
- the native transition path (`prepare_native_batch`, cybergraph's durable
  node) passed a node-global acceptance counter. No record was lost, but the
  record set depended on the order this node accepted signals in, so two
  nodes holding the same signals held different signals dimensions.

Launch row 9 requires nodes to converge from different starting states;
both keyings break it.

## the rule

`specs/state.md`, dimension version 3, rule 7:

- a signal record lives at `signal_key(neuron, step) = H(neuron ‖ step)` —
  hemera over the 32-byte neuron id and the step as 8 little-endian bytes
  (`state::signal_key`). The 40-byte preimage cannot coincide with the
  64-byte preimages of `axon_id` / `balance_key` or the 72-byte intent key;
- `SignalRecord` is `[neuron, network, link_count, block_height, proof_hash,
  step]` — `step` appended, so no earlier field offset moves (the primary
  cell of a signals entry stays at value offset 16, `link_count`).

Widths: trie leaf values and flat-table entries 28 → 30 limbs; native
record (domain 10) 108 → 116 bytes, key `10 ‖ signal_key` (was
`10 ‖ position_be`). `TABLE_VERSION` stays 2 (record-agnostic layout, rule 6);
`DIMENSION_VERSION` stays 3.

API: `apply_signal_record(record) -> key`; `prove_signal(state, neuron,
step)`, `prove_signal_key(state, key)`; `bbg_query(Dim::Signals, key)` takes
the signal key; `NativeChange::Signal { signal, header }` and
`prepare_native(signal, Option<SignalRecord>, intent)`, a repeated
`(neuron, step)` is `Error::SignalExists` (was `PositionExists`). The query
authenticator no longer treats signals as a numeric dimension. CLI: `bbg
prove signals <neuron-hex>:<step>`.

## evidence

`rs/tests/signal_key.rs`:

| test | property |
|---|---|
| `signal_key_binds_neuron_and_step` | key = hemera(neuron ‖ step_le); neurons and steps separate |
| `two_neurons_first_signals_both_persist` | both step-0 records survive, under v1 and v2 |
| `arrival_order_does_not_move_the_root` | 8 neurons × 6 steps (8 records per step value), 4 shuffles + reverse: identical root and signals leaf, v1 and v2 |
| `every_record_is_under_the_root_and_provable` | trie path per record under the signals leaf; v1 entity read by (neuron, step) authenticates only that key; the step is under the root |
| `native_batch_keeps_both_step_zero_signals_and_rejects_a_repeat` | two native records in one batch, value carries the step; a dropped preparation restores; a repeat is `SignalExists`, state untouched; the next step is accepted |
| `replay_is_deterministic_and_recovery_matches_every_stored_root` | 40 native batches over 8 interleaved chains: no record lost, the persisted change stream equals the state's records, a replay from genesis reproduces every snapshot, every stored root `time[h]` and the record stream |

Also: `root_vectors.rs` computes the key from hemera directly in its
independent reference derivation (all-dimension vector now has two step-0
records), and `recovery_replay_reproduces_every_stored_root` writes a signal
record per signal (six chains from step 0; asserts no record lost).

Mutation check: with the neuron removed from the key preimage (the pre-fix
collision), all six `signal_key.rs` tests fail and so do
`all_dimension_roots_match_the_reference` and
`recovery_replay_reproduces_every_stored_root`.

Gates (siblings: hemera `origin/main` `fd55f99`, lens `origin/release/0.2`
`b1040c2`, nox `origin/release/0.4` `12adaf8`, zheng `origin/release/0.4`
`8f20262`, strata `origin/main` `12a7a6d`, neuron `origin/main` `9df467d`,
tade `origin/main` `778dc6f`):

- `rs/`: `cargo test --locked` exit 0 (lib 81, every suite green), and
  `cargo test --locked --all-features` exit 0 (lib 108); zero `warning:` lines.
- `cli/`: `cargo test --locked` exit 0, 16 passed.

## roots

No version-3 genesis state carries a signal record: soft3's chain kernel
(`chain/bbg_state.rs`) and genesis builder (`genesis/`) never write the
signals dimension, so the empty signals trie (`EMPTY`) and every blessed root
are unchanged. Rebuilt in the soft3 stand on this branch (hemera
`release/0.4` `3cdaa7f`; recorded in soft3 terms in `cyber/launch.md`
row 14):

| chain | state root | block-0 id | file |
|---|---|---|---|
| pussy | `a360798987fb821f3bb7e07c66947e91e1eacb4278a834972c32ea8b3054b8b2` (unchanged) | `bcb1e9b0…` (unchanged) | byte-identical, sha256 `ec7aee79…`; 69 MB peak |
| bostrom | `c50a6d8438ed8b0ef72450e7f83cab81e78bfd4e833106add9d62a0831bcdf13` (unchanged) | `9ab9fc49…` (unchanged) | byte-identical, sha256 `ffa3d6dd…`; 88 s wall, 107 s user, 7.3 GB peak |

Both files compared with `~/.cache/cyber/genesis/` (built 2026-10-16): equal sha256, equal genesis particle, state root and block-0 id; only `root_seconds` in the side-car JSON differs. `soft3 conformance check chain --genesis ~/.cache/cyber/genesis/pussy.tade`: 30 entries, no drift. soft3 `cargo test --locked --release --no-default-features --features chain-conformance` in the stand: lib 252 passed, all suites green; one pre-existing warning (deprecated `GenericArray::as_slice`, `chain/conformance.rs`).

What does move: any state holding signal records — cybergraph's
`link`/`seal` path and its durable native node (cybergraph
`launch/9-signals-key-v3`), and soft3's stack conformance vector
`cybergraph::link@v1 "neuron-7/two-signals/bbg-root"`
(`conformance/snapshots/mechanism.snap`, blessed 2026-10-10 on hemera 0.3
and pre-v3 bbg; already due for re-bless with S2, not re-blessed here).
