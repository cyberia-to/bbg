---
tags: bbg, audit, verify, soundness
crystal-type: report
crystal-domain: cyber
---
# verify_query ignores which dimension and root it answers

property #40 — a verifier checks every argument it takes.

`query::verify_query(proof: &QueryProof) -> bool` takes only the proof
itself. It re-checks that `proof.opening` is a valid Brakedown opening of
`proof.commitment` at `proof.point` to `proof.value_bytes` — nothing more.
The function has no `root` parameter and no `Dim` parameter, so it cannot
bind the proof it accepts to a particular BBG state or a particular
dimension of that state. Its own doc comment says as much: "Does not
re-check which dimension the proof came from — the caller binds that
context." Row 75/bbg#14 found the identical shape in `verify_particle`,
whose parameters were named `_root`/`_particle` (never read); row
142/bbg#19 found it again in `verify_opening`, which reads neither
`leaves` nor `namespace`. `verify_query` is the third instance.

`verify_query_ignores_which_dimension_and_root_it_answers` in
`rs/src/query.rs` demonstrates it directly: a proof for the same key under
two states with different roots and different linked data both verify
`true`, and a proof for a different dimension of the same state also
verifies `true`. The bool `verify_query` returns is identical in every
case — nothing in it lets a caller conclude which root or which dimension
the proof actually answers.

## why this one is not `verify_particle` again

`verify_particle`'s own audit (row 75) found no production caller reaching
it yet. `verify_query` does have one: `bbg-cli`'s `cmd_prove` (`cli/src/
main.rs`) calls `bbg::verify_query(&proof)` on a proof it just produced
with `bbg::bbg_query(&bbg.state, d, &k)` in the same process, against its
own in-memory `bbg.state`, and prints `verified: ok` on `true`. Today this
is safe only because prover and verifier share one process and one state —
the CLI's own note says so ("proof held in-process; portable proof export
awaits lens serde"). The moment a `QueryProof` crosses a wire boundary —
row 16's particle availability fetch, row 34's DAS sampling, any future
`bbg-cli` remote mode — `cmd_prove`'s pattern is exactly the shape that
lets a peer serve a proof from a stale, forked, or entirely different
state and have it print `verified ok`.

## the fix this PR does not attempt

Same reason row 142/bbg#19 held off: `BbgState::root_leaves()` packs each
dimension's Brakedown commitment into a fixed-position limb of a 14-leaf
struct that gets hashed into the root in one shot, not through a Merkle
tree. There is no per-dimension inclusion proof today — a light client
cannot check "this commitment is dimension D of root R" without being
handed every other dimension's commitment too. Closing this needs one of:

- extend `QueryProof` with a `Dim` tag and thread `root_leaves()`-shaped
  sibling commitments through as an inclusion proof, so `verify_query`
  can take `(proof, dim, root)` and recompute the root from `proof.
  commitment` plus the siblings; or
- Merkleize `root_leaves()` itself, so a per-dimension opening is O(log 14)
  instead of O(14).

Either is a structural change to `QueryProof`/`root_leaves()` shared with
row 75 and row 142's open follow-up, not a one-file fix, and getting the
limb-packing convention subtly wrong would produce a check that looks
sound against self-consistent fixtures while remaining unsound — worse
than the documented gap. Left for whoever picks up rows 75/142's shared
remains.

## verified

```
$ cd rs && cargo test --lib -- query::tests::verify_query_ignores_which_dimension_and_root_it_answers
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 53 filtered out
```
