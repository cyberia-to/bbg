# audit: verify_particle does not bind its proof to root or particle

date: 2026-09-22 · revision: 450b579ac1335f2a3efb12ce56885d4f5205f3df (origin/master)

## finding

`rs/src/proof.rs`'s `verify_particle` takes a claimed root and a claimed
particle but discards both:

```rust
pub fn verify_particle(proof: &QueryProof, _root: &Particle, _particle: &Particle) -> bool {
    let value = eval_value_from_bytes(&proof.value_bytes);
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&proof.commitment, &proof.point, value, &proof.opening, &mut tx)
}
```

the leading underscores are the tell: the function checks only that
`proof.opening` is a valid Brakedown opening of `proof.commitment` at
`proof.point` to `proof.value_bytes`. it never checks that `proof.commitment`
is the one bound into `_root`, and it never checks that the entry the proof
opens actually belongs to `_particle`. a `QueryProof` generated for one
particle under one BBG state verifies successfully against any other root
and any other claimed particle passed to this function.

`rs/tests/serde_wire.rs:50` already demonstrates the gap without meaning to —
it asserts `verify_particle(&back, &[0u8; 32], &[0u8; 32])` against an
all-zero dummy root and particle and gets `true`. this PR adds a direct
regression, `verify_particle_accepts_wrong_root_and_wrong_particle` in
`rs/src/lib.rs`, that builds a real proof for `particle(3)` under a real
root, then shows it verifies against `particle(255)` and root `particle(255)`,
neither of which the proof has anything to do with.

`verify_query` in `rs/src/query.rs` has the same shape but says so honestly
in its doc comment: "does not re-check which dimension the proof came
from — the caller binds that context." `verify_particle` makes no such
disclaimer and its signature actively suggests the opposite: a caller
reading `fn verify_particle(proof, root, particle) -> bool` has every reason
to believe passing the wrong root or the wrong particle changes the answer.

## why closing this is not a small fix

`QueryProof` (`commitment`, `opening`, `value_bytes`, `point`) carries enough
to prove the opening is internally consistent, but not enough to bind it to
a claimed identity:

- binding to `_root` needs the dimension's commitment to be checked against
  `root_leaves()` (`rs/src/state.rs`): `BBG_root` is a hemera compression of
  the 11 dimension commitments plus `A`/`N`/stats. `QueryProof` carries no
  tag for which of the 11 dimensions it opened, and no inclusion path from
  that dimension's commitment into the root hash chain.
- binding to `_particle` needs proof that the opened cell's *key* is
  `_particle`. `dim_entries` is sorted by key and `point` encodes the flat
  index of that sort order, not the key itself; `value_bytes` carries the
  entry's value fields, not its key. nothing in `QueryProof` lets a verifier
  recompute "this index corresponds to this particle" without already
  holding the full dimension.

closing this needs either a structural extension to `QueryProof` (a
dimension tag plus a root-inclusion proof, and a key commitment alongside
the value) or a documented caller obligation matching `verify_query`'s, with
every call site audited to confirm the caller actually performs that
binding out of band. neither is a 40-minute slice; this PR is the audit and
the regression, not the fix.

## scope checked

`rg -n 'pub fn verify' rs/src` finds three verifiers: `proof::verify_particle`
(this gap), `query::verify_query` (honestly documented as unbound, same as
here), `query::verify_opening` (not reviewed in this pass). only
`verify_particle` is called anywhere in this checkout (`rs/src/lib.rs`,
`rs/tests/serde_wire.rs`) — both existing call sites are tests, so the gap is
not yet reachable from production code, but every `prove_particle` result a
light client receives over the wire would go through exactly this function.

## remains

- decide: extend `QueryProof` with a dimension tag and root-binding, or
  document `verify_particle`'s caller obligation like `verify_query` does
  and rename the misleading `_root`/`_particle` parameters to make the gap
  visible instead of hidden by convention
- review `query::verify_opening` for the same shape
- once light clients start calling `verify_particle` over the wire (row 16,
  row 34), this must be closed before that path is trusted
