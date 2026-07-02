---
tags: cyber, cip
crystal-type: spec
crystal-domain: cyber
status: draft
date: 2026-06-11
---
# bbg cli

a command-line tool for the authenticated state layer: inspect the committed state, apply signals, finalize blocks, and prove/verify records. `bbg`'s verb is **store** — the CLI is the store's console, the way `redb`/`sqlite` ship a shell for their engine.

## scope — what it is, and is not

bbg is a pure state library; [[cybergraph]] drives the signal lifecycle (validate → order → apply). the CLI honors that split:

- it **is** a low-level state console for developers and validators — apply a signal, advance a block, read a dimension, prove a key, check the root.
- it is **not** the signal lifecycle. it does not validate proofs, order a signal chain, detect equivocation, or route networks — those are [[cybergraph]] and [[foculus]]. `bbg apply` is the raw `insert`, the same one cybergraph calls after its gate passes.

think of it as the layer beneath `cy` and the `soft3` CLI: they orchestrate; `bbg` inspects and mutates the store directly.

## store — signal-first persistence

bbg state is a deterministic fold of the signal log (see [[storage]]): `state(h) = fold(genesis, signals[0..h])`. the CLI persists the **log**, not the state — and the log is **[[tape]] frames**, the stack's one wire format:

```
bbg --store <dir> <command>
  <dir>/log        append-only tape stream — cyber-dialect signal/intent frames
  <dir>/checkpoint latest root + accumulator + height (~232 B cache)
```

the log is a concatenation of tape frames (ZAP = signal, KET = intent — the same frames [[foculus]] mints and [[radio]] carries). tape frames are self-delimiting and resynchronizable, so the log is just frames back-to-back; `decode_signals` walks it in order. `open` replays the log into a `BbgState`; `apply` folds one signal and appends its frame; `finalize` advances the checkpoint. no `--store` → an ephemeral in-memory instance (useful for one-shot `apply … | prove …` pipelines and tests). the log is the source of truth; the checkpoint is a replay-skip cache, rebuilt if absent.

there is no bespoke serialization: the CLI reads and writes the cyber-dialect tape codec that already exists in the stack. JSON appears only as a `--json` *output* view for inspecting state (`dump`, `stats`) — never as the canonical log or wire.

## commands

### inspect

```
bbg root                 print BBG_root (hex, 32 bytes)
bbg height               print current block height
bbg stats                print GraphStats: node_count, relation_sizes[..], max_degree, diameter_bound
bbg get <dim> <key>      print the raw record at (dimension, key), or nothing if absent
bbg dump [dim]           dump one dimension (or all) as JSON, sorted by key
```

### mutate

```
bbg apply <signal>       fold a signal (JSON file, or - for stdin) → append to log, update state
                         the raw insert; rejects only on structural DoubleSpend
bbg intend <intent>      persist an unsealed intent (scope declaration)
bbg finalize             finalize a block: snapshot time[h]=root, advance root + height,
                         prune at epoch boundaries
bbg prune                run pruning now (rank-floor / half-life / gravity)
```

### prove / verify

```
bbg prove <dim> <key>    generate a query proof for the key; print the opened value and the
                         in-process verify verdict (ok / fail)
bbg verify <proof>       verify a serialized query proof against its embedded root   [blocked]
```

`<dim>` is one of: `particles axons-out axons-in neurons locations coins cards files time signals balances commitments nullifiers` (the [[cross-index|Dim]] set). `<key>` is a hex particle/neuron id, or a u64 for `time`/`signals`.

## output

- default: human-readable lines (hex roots, aligned tables for stats/dump).
- `--json`: machine output for every command, so the CLI composes into scripts and the `soft3` SDK.
- exit codes: `0` ok, `1` not-found (get/prove absent key), `2` rejected (DoubleSpend on apply), `3` invalid input.

## implementable now vs blocked

buildable against today's API: `root`, `height`, `stats`, `get`, `dump`, `apply`, `intend`, `finalize`, `prune`, and `prove` (generate + in-process verify + print value). the enabler is not new serialization — it is **reusing the [[tape]] cyber-dialect codec**. foculus's `src/frames.rs` already encodes/decodes signal and intent frames (`encode_signal_frame` / `decode_signals`, `encode_intent_frame`); the tape signal frame carries the state-application shape (neuron, links, height) the CLI folds via `bbg.insert`, box_moves empty (Release 0). the codec should be canonicalized so `bbg-cli` and `foculus` share one implementation — it depends only on [[tape]] + bbg, so the CLI pulls no network stack.

blocked on lens serde (soft3 blocker #1): serializing a `QueryProof` to a file (`prove` → portable proof) and `bbg verify <proof>` from that file. `QueryProof` wraps lens `Commitment`/`Opening`, which are lens-internal with no serde. until that lands, `prove` proves-and-checks in one process and prints the value + a proof digest; cross-invocation proof exchange waits.

## crate + conventions

- new binary crate `bbg/rs` gains `[[bin]] name = "bbg"` at `src/bin/bbg.rs`, or a thin `bbg-cli` member — the library stays untouched.
- args via `clap` (matches `foculus`'s bin).
- installed by `scripts/install-bins.sh` alongside `cy`/`cyb`/`mudra`; built with `RUSTC_BOOTSTRAP=1`.
- one file per command group if `src/bin/bbg.rs` exceeds 500 lines (quality file-size rule).

## open questions

1. codec home — the cyber-dialect signal/intent tape codec lives in `foculus/src/frames.rs` today, operating on foculus's envelope `Signal`. `bbg-cli` needs the same frames but only bbg's state-application shape. proposal: lift the cyber-dialect codec into a shared spot (a `cyber-dialect` module in [[tape]], or a tiny shared crate) that both `foculus` and `bbg-cli` import — one frame format, one implementation, no duplication. the CLI depends on [[tape]] + bbg only.
2. does `apply` require the neuron to exist (focus debit) or auto-seed? bbg's `insert` debits focus only if the neuron is present; the CLI should surface a warning when applying links from an unknown neuron.
3. a `replay <log>` command for auditing — fold a foreign tape log and print the resulting root, for fraud detection against a claimed root.
