---
tags: bbg, audit, launch
crystal-type: audit
crystal-domain: cyber
---
# dimension version 2 — the bostrom genesis root unblocked

Date: 2026-10-15. Scope: bbg `feat/dimension-chunks` (PR #41, on master after
#40) and its integration onto the 0.3 line `int/genesis-dims` (PR #42), with
soft3 `genesis/dimension-v2` on top of soft3 #183 (`soft3 genesis build`).
Tracker: `cyber/launch.md` rows 14 (genesis bijective with the snapshot) and
43 (block cost). Prior finding: soft3 #183 — the bostrom state root was
`root_blocked`: lens/brakedown `MAX_VARIABLES = 20` caps a v1 dimension at
2^20 field elements and bostrom's particles dimension needed 2^27, files /
balances / axons 2^25, signals 2^24; two engine inflations made it worse.

## what changed

1. **root version 2 is the default.** The dimension leaf is the keyed-trie
   root (`rs/src/trie.rs`, #40). There is no size cap: a trie grows by one
   leaf and ≤ one branch per record. v1 (flat Brakedown polynomial) stays
   opt-in for the zheng look argument and the state certificates.
2. **injective leaves.** Keys enter the leaf chain as eight 32-bit limbs, every
   `u64` field as two 32-bit limbs (`dim::u64_limbs`, `dim::bytes32_limbs` —
   the same functions the 0.3 line's `DIMENSION_VERSION = 2` serialization
   uses). One Goldilocks limb per `u64` is not injective (`v` and `v − p`
   collide) and a raw 64-bit key limb ≥ p was craftable.
3. **a link without conviction writes topology only.** `BbgState::insert` with
   `amount = 0` touches `axons_out`, `axons_in` and the reverse edge map and
   nothing else: no axon-particle, no target energy, no neuron focus, no
   balance row. A genesis replay is all such links.
4. **a zero balance is an absent row.** `balances[k]` reaching 0 is removed.

Spec: `specs/state.md` § root versions → "dimension version 2", § dimension
commitment v2.

## the two inflations, measured on the snapshots

Counts from `soft3 genesis build` (`bbg:` lines of its report), before =
soft3 #183's run on the 0.3 line, after = `int/genesis-dims`:

| chain | dimension | before | after | why |
|---|---|---|---|---|
| pussy | particles | 16,271 content + axon-particles per link | 16,271 | rule 3 |
| pussy | balances | one zero row per link endpoint + 442 real | 442 | rules 3–4 |
| bostrom | particles | 6,086,630 (3,143,650 content + axon-particles) | 3,143,650 | rule 3 |
| bostrom | balances | 3,200,435 (56,785 real) | 56,785 | rules 3–4 |

## genesis build, end to end

`soft3 genesis build --chain <chain> --snapshot ~/cyber/<repo>/snapshot --out
/tmp/gen/<chain>` (cache `~/cyber/.cache/snapshots/<chain>`), release build,
M4 Max, one thread; peak memory = `/usr/bin/time -l`. `verify --root`
re-loads the manifest and recomputes the root.

| chain | links | genesis hash | state root (v2) | wall | peak memory | verify --root |
|---|---|---|---|---|---|---|
| pussy | 29,112 | `4d703ecc1d618eb4921de62d8bca4326db8b7cb9140dced576278e1d9eec70ef` | `47fcbd67ce96ecf965b77527a1fc0a01982f0ce7b76b275240d315fe8ce8d924` | 10.7 s | 74 MB | clean |
| bostrom | 2,949,732 | `d0965ed10f0fe29c33467d352e9ae5c66b59cd4ff04169fe143b9a06800c0fd2` | `7cbac3268e12e6d954ec477e6a2039b5697d4ba8bda85ca2a1c59e42fa8d025a` | 1,790 s (state load + root 1,750 s; the bbg link replay shared the machine for the first 20 min) | 8.57 GB | not run (a second 30-minute load; the pussy verify is clean and the loader is deterministic by #183's two-build check) |

Before: pussy `8339b4ad…` / root `f5b10b87…` (v1 on the 0.3 line, with the
inflations); bostrom hash `da45937f…`, root **blocked**, 104 s, 5.2 GB
(soft3 `audit/genesis-2026-10-15.md`). The genesis hash moves because the
manifest carries `state_counts` / `state_fields`, which the deflation changed;
the root moves because the derivation is v2. The new chain starts fresh, so
nothing deployed depends on the old values. No conformance `.snap` files exist
to bless (`soft3/conformance` is a scaffold).

## bbg alone: the bostrom link stream through `BbgState::insert`

`rs/examples/replay_pussy.rs /tmp/bostrom-cyberlinks.csv v2 0` — the 2,949,732
links as one-link signals with `amount = 0`, one block per source height,
root at every block edge (383,503 blocks), no genesis tables:

| links | wall | µs/link | peak memory |
|---|---|---|---|
| 2,949,732 | 1,168.6 s | 396 | 4.74 GB (the CSV rows stay resident) |

Per-10k cost: 587 µs at 500k, 328 µs at 1.5M, 353 µs at 2.5M — flat; the last 400k links ran at ≈ 1.2–1.6 ms while the genesis build shared the machine. max_degree 30,439 (a hub append is O(log degree)).

## what this does not do

- The zheng look argument and the state certificates still open v1 Brakedown
  cells; a read against a v2 root needs a trie-path opening in zheng. Tests
  that exercise them pin `RootVersion::V1` on small states.
- The per-neuron nonce field (soft3 `feat/chain-kernel` → bbg
  `feat/chain-dims`) was not on origin when this was written; it belongs in
  the same dimension version — add it to `NeuronRecord::leaf_vals` and
  `dim_entries`, re-pin `tests/root_vectors.rs`.
- Storage is untouched: the tries live in memory next to the maps.
