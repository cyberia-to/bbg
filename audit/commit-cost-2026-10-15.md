---
tags: bbg, audit, launch
crystal-type: audit
crystal-domain: cyber
---
# commit cost — before and after incremental dimension commitments

Date: 2026-10-15. Scope: bbg `441e3c9` (origin/master) → branch
`fix/launch-43-incremental-commit`. Tracker: `cyber/launch.md` row 43 (blocks
row 14, the bostrom genesis replay). Prior finding: cybergraph
`audit/import-commit-cost-2026-09-27.md` — every accepted operation recommitted
every dimension from its full record set, so a chain's cost was quadratic in its
length.

## what changed

- every committed dimension is a `Keyed` map (`KeyedSet` for N, `AxonIndex`
  for adjacency) that keeps a canonical keyed trie (`rs/src/trie.rs`) current
  on every mutation and caches its v1 Brakedown commitment until it mutates;
- `RootVersion::V1` (default) folds the Brakedown leaves — bit-exact with the
  previous root, only dimensions touched since the last read are recommitted;
- `RootVersion::V2` folds the trie roots — a block costs O(changes · log n);
- `max_degree` is maintained by the adjacency index, not scanned per root.

Spec: `specs/state.md` § BBG root, § root versions, § dimension commitment v2.

## method

`rs/examples/commit_cost.rs N B [v1|v2]`: N synthetic one-link signals from
1,000 neurons, sources uniform over N/2 particles, targets Zipf-like over N/4
(hubs), one block per B = 100 links, root materialized at every block edge.
`rs/examples/replay_pussy.rs CSV [v1|v2]`: the pussy snapshot
(`~/.cache/snapshots/pussy/cyberlinks_indexed.csv.gz`, 29,112 cyberlinks,
114 neurons), one block per source height (17,696 blocks). Release build, one
thread, M4 Max; peak memory = `/usr/bin/time -l` "peak memory footprint".
Wall time is the whole run including state construction.

## results

### synthetic, 100 links per block

| links | v1 (before) wall | v1 µs/link | v2 (after) wall | v2 µs/link | v2 peak memory |
|---|---|---|---|---|---|
| 10,000 | 39.6 s | 3,964 | 5.7 s | 572 | 20 MB |
| 100,000 | stopped at 50,000 links after 1,175 s (next 10k would cost ≈ 500 s; the curve below extrapolates the full run to ≈ 1.7 h) | 42,358 at 50k | 69.6 s | 696 | 182 MB |
| 1,000,000 | not run (the curve below extrapolates to > 1 day) | — | 847.8 s | 848 | 1.94 GB |

Per-10k-link cost along the run (the curve, not the average):

| links done | v1 µs/link | v2 µs/link |
|---|---|---|
| 10,000 | 5,834 | 757 |
| 20,000 | 14,938 | 796 |
| 30,000 | 24,216 | 819 |
| 40,000 | 30,138 | 813 |
| 50,000 | 42,358 | 830 |
| 100,000 | — (stopped) | 757 |
| 500,000 | — | 837 |
| 1,000,000 | — | 874 |

v1 grows linearly with the state (each 10k adds ≈ 8 s per 10k), so the chain
is quadratic; v2 grows with log n only (the trie depth), ≈ 10 % per decade.

(v1 per-10k numbers are from the 100k run, which shared the machine with
builds for its first minutes; the standalone 10k run is the 39.6 s above.)

### pussy snapshot, one block per height

| mode | links | blocks | wall | µs/link | peak memory | root |
|---|---|---|---|---|---|---|
| v1 (before) | 29,112 | 17,696 | stopped at 10,000 links after 183 s (20.3 s for the first 5k, 163 s for the next 5k) | 32,605 at 10k and rising | — | — |
| v2 (after) | 29,112 | 17,696 | 21.0 s | 723 | 63 MB | `90517c16…2218c` |

## cost model after

Per touched record: one leaf chain (1 + ⌈fields/4⌉ compressions) plus two
compressions per branch on its path (depth ≈ log₂ n). A one-link signal
touches particles ×2, axons_out (outer leaf + inner set), axons_in (same),
neurons ×1, balances ×2 ≈ 8 paths ≈ 300 hemera permutations ≈ 0.6–0.9 ms on
one M4 Max core. Projection for the bostrom replay (2,949,732 links): ≈ 45
minutes single-threaded; the spacepussy-test log (39,060 signals): under one
minute. Both inside the row-43 bounds (one day / ten minutes).

Headroom, in order: hash the branch with one permutation over
`[tag, bit, 0, 0 ‖ l ‖ r ‖ 0⁴]` instead of two `compress4` (−45 % per path;
a different in-circuit row shape than the root fold, so it needs zheng's
word); batch the `neurons[ν].focus` updates of a multi-link signal into one
leaf write; parallelize dimensions.

## root

v1 roots are unchanged: `tests/root_vectors.rs` pins five roots computed on
`441e3c9` before the change (empty, one link, one link finalized, every
dimension populated, same finalized) and they hold. All 64 unit tests, the
look-argument e2e and the wire tests pass without modification.

v2 roots differ from v1 by construction (different leaf derivation) and are
pinned too. Properties checked: incremental root == from-scratch root of the
final record set after 40 random batches with double-spend rejections and
block finalization; after pruning and reinsertion; replay reproduces every
per-block root; trie paths prove inclusion and non-inclusion against the
dimension leaf; v1 ↔ v2 is a view switch on the same records.

No conformance snapshots exist to bless: `soft3/conformance` is a scaffold
(no `.snap` files in soft3, bbg or cybergraph on 2026-10-15).

## what this does not do

- It does not flip the default. The zheng look argument binds
  `LookOpening.commitment` (a Brakedown cell opening) to `leaves.dims[ns]`;
  under v2 that leaf is a trie root and the binding fails by design. v2
  becomes the default when zheng opens trie paths (leaf chain + branch chain,
  the same `compress4` rows `build_root_steps` already emits). Until then a
  node opts in with `Bbg::with_root_version(RootVersion::V2)`.
- It does not change cybergraph. `prepare_native_batch` (cybergraph
  `feat/durable-applications`, bbg `feat/atomic-application-storage`) may
  keep calling `refresh_root` per accepted operation: under v2 that call costs
  the fold plus the operation's own changes.
- Storage is untouched: the tries live in memory next to the maps (v2 1M
  links: see peak memory above). A node that loads state from storage rebuilds
  them by inserting through the wrappers, O(n log n) once.
