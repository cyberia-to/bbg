---
tags: bbg, audit, launch
crystal-type: audit
crystal-domain: cyber
---
# dimension version 4 — positions (the focus economy, bbg side)

Date: 2026-10-17. Scope: bbg `feat/positions-v4` on `origin/master`
`ceca41a` (dimension version 3 with rule 7). Contract: soft3
`specs/chain-economy.md` §3 (merged 2026-10-17, soft3#225) — the change it
requests of bbg: `CardRecord` gains `author`, `valence`, `since`,
`DIMENSION_VERSION` 4, genesis roots re-blessed. Spec: `specs/state.md`
§ dimension versions, rules 8–10. PR: bbg#51. Owner rule: everything may
change until 2026-11-02. Machine: M4 Max, shared with other agents' builds.

## what changed

1. `CardRecord` = `[owner, particle, author, valence, since]` — a position:
   one author's locked stake on one axon, at
   `card_id(author, p, q) = H(D_CARD ‖ author ‖ p ‖ q)`. `valence` is the
   `Valence` enum stored as the byte `v + 1` ∈ {0, 1, 2} (the spec's `u8`;
   any other byte is no record). Widths: trie leaf and flat table 16 → 28
   limbs (`valence` and `since` as u64, two limbs each — rule 2's injective
   encoding); native record (domain 7) 64 → 105 bytes.
2. Reserved balance keys `stake_key(card) = H(D_STAKE ‖ card)` and
   `focus_key(τ) = H(D_FOCUS ‖ τ)`, 53-byte preimages, beside
   `balance_key(owner, token)` (64 bytes) — the only key any bbg transfer path
   writes. bbg implements the three identities (`bbg::position`) with the
   chain's tags so the kernel, the CLI and the tests share one derivation.
3. `BbgState.cards` is `position::Cards`: the committed map plus an
   uncommitted index `axon → {card → stake_key(card)}`, maintained by
   `insert` / `remove` (a changed `particle` moves the card), rebuilt by
   loading the records. `BbgState::positions()` returns an owned
   `PositionSnapshot` (axons ascending, cards ascending, stake per position,
   side sums S⁺/S⁻ per axon, total and active stake); `side_sums(axon)`,
   `stake(card)`, `focus_pool(token)` read one axon / key. No hash per card at
   snapshot time (the stake key is cached in the index at insert).
4. `DIMENSION_VERSION` = 4 in every trie leaf IV. `TABLE_VERSION` stays 2
   (record-agnostic layout).

Not in bbg: the new-high marks `H⁺, H⁻` (chain-economy §5.3) and the epoch
records — chain-economy places neither in bbg state; they are the kernel's,
a function of chain history. Settlement reaches the root through the balances
it pays and `time[settle(E)]`.

## evidence

`rs/tests/positions.rs`:

| test | property |
|---|---|
| `identities_are_the_tagged_hemera_preimages_of_chain_economy` | the three tags byte for byte as chain-economy §3.1; `card_id`, `stake_key`, `focus_key` = hemera over the tagged preimage; every input binds; stake ≠ focus ≠ balance key |
| `valence_bytes_round_trip_and_reject_everything_else` | every i8 and every byte: only −1/0/+1 and 0/1/2 decode; byte = v + 1; side and activity |
| `card_record_round_trips_through_every_encoding` | native 105-byte record decodes to the record; v2 trie path verifies under the cards leaf; each of the five fields binds the leaf and the root; v1 flat table: 28 cells equal to the hand encoding, nothing past them, entity query authenticates |
| `root_and_snapshot_are_independent_of_arrival_order` | 60 positions, 4 shuffles + reverse, with intermediate records on another axon, decoy cards inserted and removed: identical root and snapshot, v1 and v2 |
| `enumeration_per_edge_matches_the_model` | 120 blocks of random locks (three valences), partial/full unlocks, payout changes, pool accruals: after every block snapshot, side sums, `on_axon`, `find`, totals, stakes equal an independent model; boundary snapshots frozen |
| `the_index_follows_moves_and_drops_empty_axons` | a record moved to another axon moves in the index; an emptied axon is dropped; an emptied dimension has the empty root |
| `recovery_replay_reproduces_every_stored_root_and_the_index` | 80 blocks replayed from genesis: every stored root `time[h]`; the cards and balances rebuilt from the persisted native record stream give the live snapshot and the same commitments |
| `transfer_paths_never_move_a_stake_or_the_pool` | 400 random transfers — `insert` with conviction, the native signal path (published or dropped), native `LocalCredit` — with owners, targets and tokens drawn from the card ids, the stake keys, the pool keys and the token: every reserved balance and every card unchanged after every step; no owner/token pair of the pool hashes to a reserved key |

Also: `root_vectors.rs` reference derivation lays cards out by rule 8
(`all_dims` holds three positions, both sides of one axon);
`query_authentication.rs` walks a 28-limb card entry; the trie unit test
pins the version word 4 in the leaf IV.

Spec vectors (chain-economy §10, hemera 0.4): the three identity vectors
were recomputed through the same construction against hemera
`release/0.4` `7237a23` (a throwaway crate in `/tmp`, hemera exported by
`git archive`): `card(0x66 × 32, token("boot"), token("pussy"))` =
`74f0a211…b5b4`, `stake_key` = `f7e0cbf8…b08a`, `focus_key(token("pussy"))` =
`0fae8671…d4b9` — all three equal to the spec. bbg's own gates run on hemera
`fd55f99` (0.3.1, the recorded sibling), where the tests are hash-agnostic.

Gates (siblings: hemera `fd55f99`, lens `b1040c2`, nox `12adaf8`, zheng
`8f20262`, strata `12a7a6d`, neuron `9df467d`, tade `778dc6f` — as recorded
in `signals-key-2026-10-17.md`):

- `rs/`: `cargo test --locked` exit 0 (lib 81, positions 8, every suite
  green), `cargo test --locked --all-features` exit 0 (lib 108, every suite
  green); zero `warning:` lines. One earlier all-features run failed
  `shared_application::applications_and_shards_share_one_owner_and_publication_boundary`
  with `Busy` from `Database::open` (a storage file lock, machine load 8–12);
  the test passed in the first gate run, five isolated reruns and the full
  rerun recorded here. It does not touch cards.
- `cli/`: `cargo test --locked` exit 0, 17 passed (adds `card_key_arg`:
  `bbg prove cards <author>:<p>:<q>`).
- `cargo clippy --all-targets`: 8 warnings, the same count as `master`; none
  in the new code.
- Mutation checks, each restored after: the index not moving a card whose
  axon changes → `the_index_follows_moves_and_drops_empty_axons` fails;
  `since` dropped from the leaf → `card_record_round_trips_through_every_encoding`
  fails; `stake_key` derived as `balance_key(card, 0)` →
  `identities_…` and `transfer_paths_never_move_a_stake_or_the_pool` fail.

## roots

`DIMENSION_VERSION` enters every trie leaf, so every root of a state with at
least one record moves; the empty state root does not (empty tries are
`[0, 0, 0, 0]` and the stats leaf carries no version). Concretely:

- both genesis roots — pussy `a3607987…` and bostrom `c50a6d84…`
  (dimension version 3, `signals-key-2026-10-17.md`) — move; no genesis frame
  writes a card (chain-economy §4.1: genesis links have amount 0 and genesis
  weights are not stored), so the move is the version word alone;
- every blessed chain vector of soft3 that carries a state root
  (`conformance/chain.snap`, chain-conformance), and the block-0 ids that
  commit to them;
- any state holding cards: the record is wider (rule 8).

New genesis roots are re-blessed by the soft3 kernel change that adopts
dimension version 4 (chain-economy §13); this branch does not build them.
