---
tags: bbg, audit, launch
crystal-type: audit
crystal-domain: cyber
---
# dimension version 3 — the root chain's genesis version, deferred hashing, both burials committed

Date: 2026-10-16. Scope: bbg `feat/dimension-v3` — the 0.3 line
(`chore/coordinated-release-20260916`, `ccda7a2`) + #40 (launch row 43,
incremental commitments) + #41/#42 (dimension version 2) + the per-neuron
nonce of `feat/chain-dims` (`97c6272`) + this branch. Consumer: soft3
`feat/bbg-nonce-on` (the chain kernel and `soft3 genesis build`). Trackers:
`cyber/launch.md` rows 43 and 14. Machine: M4 Max, 16 cores, 48 GB, release
builds; the machine was shared with other agents' test runs (load average
30–70), so single-thread costs are reported as **user CPU time** beside wall
time.

## what changed

1. `NeuronRecord.nonce: u64` — the chain's replay counter under the root
   (soft3 `specs/chain.md` §2.5, §3.2); fourth neuron value in the trie leaf,
   the flat table (width 8 limbs) and the native record (32 bytes).
2. `DIMENSION_VERSION = 3` names the rule set (`specs/state.md` § dimension
   versions) and enters every trie leaf IV: `[LEAF_TAG, |v|, 3, 0]`. The flat
   table layout keeps its own word, `TABLE_VERSION = 2` — the zheng
   state-evidence contract, record-agnostic — so zheng `release/0.4` reads
   bbg certificates unchanged.
3. Deferred hashing: a mutation marks the key (`Keyed`, `AxonIndex`) and the
   trie path (`Trie`); the next root read hashes each touched leaf once and
   each dirty branch once, bottom-up. Above 2^14 touched records the
   dimensions hash on parallel threads (a bulk load). Roots unchanged.
4. `AxonIndex`: degree histogram (max degree without a scan), `truncate`
   (the kernel's revert of a block's appends), the list mirrors the set.
5. Vendored fjall builds warning-free.
6. Tests are hash-agnostic: every expected root is recomputed by an
   independent reference of `specs/state.md` over hemera (no hand-pinned
   digest), so the vectors follow hemera 0.4 without re-pinning.

7. (amended 2026-10-17, rule 7) the signals dimension is keyed by
   `signal_key(neuron, step) = H(neuron ‖ step)` and `SignalRecord` carries
   `step` — two neurons' step-0 signals no longer share a record (launch
   row 9). Version 3 kept; no blessed root moves. Evidence:
   [signals-key-2026-10-17.md](signals-key-2026-10-17.md).

## why not chunked Brakedown commitments

The alternative considered for the size cap: split each flat dimension into
Brakedown-committed chunks of ≤ 2^20 fields, hemera-hash the chunk roots into
the dimension leaf, recommit only touched chunks. Brakedown commit cost,
`lens release/0.2`, one thread:

| fields | commit | per field |
|---|---|---|
| 2^10 | 2.3 ms | 2.2 µs |
| 2^12 | 7.3 ms | 1.8 µs |
| 2^14 | 25 ms | 1.5 µs |
| 2^16 | 92 ms | 1.4 µs |
| 2^18 | 355 ms | 1.4 µs |
| 2^20 | 1.39 s | 1.3 µs |

bostrom's adjacency dimensions are 2^25 fields each (26–27 chunks). A sorted
flat layout shifts every chunk after an insertion point; with key-bucketed
chunks a block of 100 random links still touches most chunks of each
adjacency dimension: ≈ 50 chunk commits × 1.39 s ≈ 70 s per block against a
30 s block target. The trie costs ≈ 0.3–0.7 ms per link, has no cap, and its
worst case under adversarial keys is bounded (depth ≤ 256; chunks under
adversarial clustering are not). Chunks would keep Brakedown cell openings,
but the zheng look argument cannot open a chunked leaf without a circuit
change either; when zheng opens state it opens trie paths (`compress4`, the
primitive the root fold already replays). One hemera `compress4` costs
2.7 µs here.

## row 43 — commit cost, before and after

`rs/examples/commit_cost.rs N 100 [v2|v1]` (synthetic one-link signals with
conviction, 1,000 neurons, Zipf-like targets, root at every block edge).
"v1" is the flat derivation that was the default before #40; "trie, eager"
is #40 (`c95924b`, hashes every path on every write); "trie, deferred" is
this branch (`cad9f40`+). Same roots in both trie columns
(`bb22fc74…` at 10k, `4ad5342e…` at 100k).

| links | v1 flat (before #40) | trie, eager (#40) | trie, deferred (this) | peak memory |
|---|---|---|---|---|
| 10,000 | 3.96 ms/link (#40 audit); 9.05 ms/link wall today | 594 µs/link user (5.94 s) | **294 µs/link** user (2.94 s) | 20 MB |
| 100,000 | > 1 h — 42 ms/link at 50k and rising linearly (#40 audit) | 951 µs/link user (95.2 s) | **580 µs/link** user (58.0 s) | 155 MB |
| 1,000,000 | — | 0.85 ms/link wall (#40 audit, unloaded) | **687 µs/link** user (686.6 s; 993.9 s wall loaded) | 1.48 GB |

Per-link cost grows with log n only (depth of the touched paths), never with
n: v1 is linear per block, so quadratic per chain. Deferred hashing pays per
distinct record per block and shares the upper trie levels: a single
10,000-link block costs 134 µs/link, one link per block 2.2 ms/link (three
root reads per block: before `time[h]`, after, checkpoint).

Root bit-exactness: dimension version 3 moves every root on purpose (nonce,
versioned leaf IV). Within version 3 the eager (#40 design) and deferred
implementations give identical roots on every vector, every property test and
both burials (pussy: identical genesis root `93e4660f…` from both binaries).

## recovery

- bbg `recovery_replay_reproduces_every_stored_root`: 60 random blocks
  (conviction-free and paying links, nonces bumped, zero balances dropped),
  replayed from scratch — every stored block root `time[h]` reproduced, and
  a from-scratch rebuild of the final records gives the same root.
- bbg `bulk_and_incremental_commitment_reads_agree`: one root read after
  12,000 writes (parallel flush) == a read every 97 writes == a rebuild.
- soft3 `reopen_replays_bodies_and_verifies_roots` (chain kernel recovery:
  replay every main-chain body on genesis and check every header's
  `state_root`) and the fork/reorg suite pass on this bbg.

## property tests

- trie: boundary keys (all-zero, all-one, first-bit and last-bit splits, the
  256-deep single-bit chain, u64 edges) under 1,500 random set/remove steps
  vs from-scratch, with inclusion/non-inclusion proofs for every key every 25
  steps; random batches vs from-scratch; the dimension version is in every
  leaf (recomputed from `compress4`).
- `AxonIndex`: 2,000 random add / remove_axon / truncate / remove / insert
  vs a model, commitment == a fresh build, max degree == the model's.
- state: incremental == from-scratch after random batches, pruning and
  replay; nonce under the root under v1 and v2 (including the `get_mut`
  guard); both root versions == the reference derivation (empty, one link,
  every dimension, before/after a block).

## genesis, both burials (soft3 `genesis build`)

See soft3 `audit/genesis-dimension-v3-2026-10-16.md` for the full table.

| chain | state root (v3) | build wall | peak memory |
|---|---|---|---|
| pussy | `93e4660f0ef0f5216d14554188cb8aded53d598d8c80d87e4e1a811949e61fe8` | 2.5 s (root 1 s) | 75 MB |
| bostrom | `ce3052f4f2e100a2d2e8edb8a7d38183e2fd0a5ca809ade693f34238df3da8d6` | 204 s (state load + root 147 s); `verify --root` 157 s | 7.5 GB (verify 4.9 GB) |

Before (dimension version 2, eager trie, #41's run): bostrom 1,790 s and
8.57 GB; before that (flat v1) the bostrom root was blocked by the 2^20 cap.

## what this does not do

- zheng opens no trie paths yet: look proofs and state certificates pin
  `RootVersion::V1` on small states.
- The tries live in memory beside the maps; a node rebuilds them on load (the
  deferred flush makes that one hash per node).
- hemera 0.4 (profile #15) will move every root once more; no bbg code pins a
  digest, so only consumers' blessed vectors re-bless.
