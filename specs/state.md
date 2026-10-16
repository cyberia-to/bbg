---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# state

Current dimension encoding and public state authentication use [version 2](state-certificate.md). This changes roots and flat cell offsets; old roots and proofs must be recomputed.

all authenticated state committed under a single polynomial commitment. individual [[cyberlinks]] are private (polynomial mutator set). the public state contains only aggregates.

## BBG root

```
BBG_root = fold(compress4, [ROOT_TAG, 0, 0, 0], leaves)

leaves — 14 four-limb digests, order frozen (zheng::root_from_leaves):
  dims[0..11]   commitment of each public dimension, in Dim order
  A             commitment of the private commitment set A(x)
  N             commitment of the nullifier set N(x)
  stats         committed graph statistics (see [[statistics]])

compress4(a, b) = first 4 limbs of the hemera permutation over [a ‖ b ‖ 0⁸]
ROOT_TAG        = "bbg-root" as a little-endian u64

BBG_poly(index, key, t) — multivariate polynomial over Goldilocks field
11 public evaluation dimensions:
  particles     all particles: content + axons
  axons_out     directional index by source
  axons_in      directional index by target
  neurons       focus, karma, stake per neuron
  locations     proof of location
  coins         fungible token denominations
  cards         names and knowledge assets
  files         content availability (DAS)
  time          temporal snapshots (continuous, replaces 7-namespace NMT)
  signals       finalized signal batches
  balances      public balances: H(owner_id || token_id) → u64  (opt-in, plaintext)

2 independent private commitment sets (NOT dimensions of BBG_poly):
  A(x)          commitment polynomial — private record commitments
  N(x)          nullifier polynomial — spent record tracking
```

the fold and the stats leaf are the same under every root version; the
versions differ in how a dimension's leaf is derived from its records.

### root versions

| version | dimension leaf | cost of a block touching c records | status |
|---|---|---|---|
| v1 | Brakedown commitment of the flat sorted `[key(4) ‖ fields]` polynomial (`dim::commit_dim`), limbs of the 32-byte digest | O(state): every touched dimension is re-encoded and re-hashed in full; at most 2^20 field elements per dimension (lens `MAX_VARIABLES`) | kept for the zheng look argument, which binds `LookOpening.commitment` (a Brakedown cell opening) to `leaves.dims[ns]` in-circuit; opt-in (`RootVersion::V1`) |
| v2 | root of the keyed trie below | O(c · log n), no size cap | default since dimension version 2 (2026-10-15, the fresh chain) — launch rows 43 and 14 |

a v1 root recompute is linear in the state, so a chain's cost is quadratic in its length (measured: cybergraph `audit/import-commit-cost-2026-09-27.md`, bbg `audit/commit-cost-2026-10-15.md`). v2 makes a block cost proportional to what it changes. the version is selected per state (`BbgState::set_root_version`); the records are version-independent, with one consequence: `time[h]` stores the root in force at finalization, so a state that finalized blocks under v1 and the same operations replayed under v2 hold different `time` records.

**dimension version 2** (the fresh chain starts here; every node derives the same root only under the same version):

1. the dimension leaf is the keyed-trie root (v2);
2. field encoding is injective: a `u64` is two 32-bit limbs (low first), 32 bytes are eight 32-bit limbs — one Goldilocks limb per `u64` would make `v` and `v − p` collide;
3. a cyberlink without conviction (`a = 0`) writes topology only: `axons_out`, `axons_in` and the reverse edge map. no axon-particle, no energy, no focus, no balance row. (a genesis replay is all such links; writing zero records doubled the particles dimension and added a balance row per endpoint.)
4. a zero balance is an absent row: `balances[k]` reaching 0 is removed, so a state has one representation.

the zheng look argument still opens v1 Brakedown cells; it moves to trie paths (leaf chain + branch chain, all `compress4`) to prove reads against a v2 root. until then proofs pin `RootVersion::V1` on states small enough for it.

### dimension commitment v2 — keyed trie

every committed dimension is a set of `(key, fields)` records: `key` is 32 bytes (a particle, neuron, nullifier or commitment point; heights and steps little-endian in the first 8 bytes), `fields` is the record's Goldilocks vector in the frozen per-dimension order (`state/commits.rs`, same field order as the v1 polynomial) in the injective encoding: `u64` → two 32-bit limbs, 32 bytes → eight 32-bit limbs, a field element → itself. the commitment is the root of a canonical binary Merkle-Patricia trie over the keys:

```
EMPTY            = [0, 0, 0, 0]
leaf(k, v)       = fold(compress4, [LEAF_TAG, |v|, 0, 0], [k₀..k₃, k₄..k₇, chunk₁(v), chunk₂(v), …])
                   k as eight 32-bit limbs; chunks of 4 limbs, the last zero-padded
branch(i, l, r)  = compress4(compress4([BRANCH_TAG, i, 0, 0], l), r)
node(S)          = EMPTY                          if S = ∅
                 = leaf(k, v)                     if S = {(k, v)}
                 = branch(i, node(S₀), node(S₁))  otherwise
                   i  = first bit index (MSB-first: bit 0 = top bit of byte 0)
                        on which the keys of S differ
                   S_b = records whose key has bit i = b
commitment(S)    = node(S)

LEAF_TAG   = "bbg-leaf" as a little-endian u64
BRANCH_TAG = "bbg-node" as a little-endian u64
```

the shape is a function of the key set alone (a branch exists exactly where two keys first differ), so the root is a function of the record set: insertion order, deletions and reinsertions leave no trace. depth is ≈ log₂ n for hashed keys, bounded by 256. a record update re-hashes its leaf and its path: O(depth) compressions, two per branch.

adjacency dimensions (`axons_out`, `axons_in`) are sets of sets: the leaf of particle p is `leaf(p, [degree (2 limbs), node(axons of p) (4 limbs)])` where the inner trie has leaves `leaf(axon_id, [])`. appending to a hub costs O(log degree); the committed value is the set of axon ids (the v1 polynomial committed the list in insertion order).

the private sets: `A` has leaves `leaf(point, [value])` (one field element), `N` has leaves `leaf(nullifier, [])`.

a read against a v2 leaf is a trie path: `(bit, sibling)` pairs root-to-leaf plus the terminal leaf (`trie::TrieProof`). inclusion = the terminal is the key's leaf with the claimed digest; non-inclusion = the terminal is another key's leaf or `EMPTY` — the canonical trie has exactly one node at the end of every key's path. verification is the same `compress4` chain the root fold uses, so the look argument can bind a path to the root with the primitive it already replays.

## state diagram

```
                ┌──────────────────────────────────────────────────────────┐
                │                         BBG_root                         │
                │   fold(compress4, [ROOT_TAG,0,0,0], 14 leaves) — 32 B    │
                └──┬────┬────┬────┬────┬────┬────┬────┬────┬────┬────┬──┬──┬──┘
                   │    │    │    │    │    │    │    │    │    │    │  │  │
                   p   a_o  a_i   n   loc  coin card file time  sig  bal A  N  stats
                   └──────────── 11 public dimensions ──────────────┘ └ private ┘
                        each leaf: v1 Brakedown commitment limbs
                                   v2 keyed-trie root
```

BBG_poly: 11 public evaluation dimensions (particles, axons_out, axons_in, neurons, locations, coins, cards, files, time, signals, balances).
A(x), N(x): independent private commitment sets, each its own leaf.
cross-index consistency: structural — same polynomial, different evaluation dimensions. LogUp eliminated.

## checkpoint

```
CHECKPOINT = (
  BBG_root,           ← fold over the 14 leaves above, 32 bytes
  folding_acc,        ← zheng-2 accumulator (constant size, ~30 field elements)
  block_height        ← current height
)

checkpoint size: O(1) — ~232 bytes
contains: proof that ALL history from genesis is valid
updated: O(1) per block via accumulation
proof size and verification: set by soft3 proposals/proof-system-repair
```

## state transitions

```
TRANSITION: W × Signal → W' | ⊥

BBG has one operation: insert(signal).
BBG_poly updated at all affected evaluation dimensions.
cross-index consistency is free — same polynomial, no LogUp needed.

INPUT: Signal s = (ν, net, ℓ⃗, Δφ*, σ, t)
  ν     — neuron (signing agent)
  net   — destination network (a card id). each BBG instance holds ONE
          network's state; net is recorded in the signals dimension and is
          network-local — insert effects do not cross networks
  ℓ⃗    — one or more cyberlinks, each ℓ = (p, q, τ, a, v)
  Δφ*  — proven focus shift (sparse update)
  σ     — single zheng proof covering the entire signal
  t     — block height

EFFECT per cyberlink ℓ = (p, q, τ, a, v):

  public (BBG_poly):
    particles[H(p,q)]:              weight += a       ← axon-particle conviction
    particles[q]:                   energy += a       ← target particle energy
    axons_out[p]:                   insert H(p,q)     ← directional index
    axons_in[q]:                    insert H(p,q)     ← directional index
    neurons[ν]:                     focus -= cost(ℓ)  ← focus consumed
    balances[H(to || token)] += a   (public output, to = direct neuron_id or card_id)
    balances[H(from || token)] -= a (public input spend, auth signature required)

  private (independent polynomials):
    A(x): extend at new commitment point      ← O(1) Lens update (private output)
    N(x): absorb nullifiers for spent boxes    ← N'(x) = N(x) × (x - n_new) (private spend)

  output mode determined by `to` address type:
    to = stealth address  →  private output (A_live, RLWE-encrypted)
    to = direct id        →  public output  (BBG_poly balances, plaintext)

  at block finalization:
    time[height]:       BBG_root snapshot
    the root is read once per block; every dimension keeps its leaf current
    (v2) or cached until its next mutation (v1), so a block costs its changes

STRUCTURAL INVARIANT (BBG enforces):
  non-duplication: N(nullifier) = 0 → reject (double-spend)
  all other validity is pre-checked by cybergraph before insert

constraints: ~3,200 per cyberlink (polynomial updates, no hemera path rehash)
```

see [[cybergraph/validation]] for the full validation protocol, [[architecture]] for evaluation dimensions, [[privacy]] for the polynomial mutator set, [[cross-index]] for why LogUp is eliminated
