# audit: verify_opening does not bind its proof to a root or dimension

date: 2026-09-23 · revision: 450b579ac1335f2a3efb12ce56885d4f5205f3df (origin/master)

## finding

`rs/src/query.rs`'s `verify_opening` takes the full `zheng::LookOpening` —
including `leaves` (the 14-leaf BBG root preimage) and `namespace` (which of
the 11 dimensions the opening claims to be for) — but reads neither:

```rust
pub fn verify_opening(lo: &LookOpening) -> bool {
    use lens::{brakedown::Brakedown, Lens, Transcript as LensTx};
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&lo.commitment, &lo.point, lo.value, &lo.opening, &mut tx)
}
```

it checks only that `lo.opening` is a valid Brakedown opening of
`lo.commitment` at `lo.point` to `lo.value` — the proof is internally
self-consistent. it never recomputes `zheng::root_from_leaves(&lo.leaves)`
against any expected root, and it never checks that
`lo.leaves.dims[lo.namespace as usize]` actually corresponds to
`lo.commitment`. a `LookOpening` built for one dimension of one state
verifies successfully even after `leaves` and `namespace` are replaced with
values that have nothing to do with the proof — see the new regression,
`verify_opening_ignores_leaves_and_namespace` in `rs/src/query.rs`, which
takes a real opening, swaps in an all-zero `leaves` and an out-of-range
`namespace` (999), and still gets `true`.

this is the same shape as row 75/bbg#14's `verify_particle` gap, but here
`LookOpening` already carries the fields a fix would need: `leaves` and
`namespace` are exactly what `bbg::BbgState::root_leaves()` produces on the
prove side (`rs/src/state.rs`), and the struct's own doc comment says the
nox circuit "binds `leaves.dims[namespace]` to `commitment`, closing the
commitment↔root soundness gap" — but that binding happens only inside a
zheng in-circuit proof. `verify_opening` is a plain Rust function called
outside any circuit, and it does not perform the equivalent check itself.

## what a fix would need

`BbgState::root_leaves()` (`rs/src/state.rs:147`) shows the exact mapping:
each dimension's `lens::Commitment` becomes a leaf by taking the first 32
bytes of `commitment.as_bytes()` and running them through
`crate::dim::goldilocks_from_bytes32`. A structural fix to `verify_opening`
would, after the existing `Brakedown::verify` call:

1. recompute `limbs = goldilocks_from_bytes32(&lo.commitment.as_bytes()[..32])`
   and assert `lo.leaves.dims[lo.namespace as usize] == limbs`, binding the
   verified commitment to the dimension slot it claims;
2. accept an expected root parameter and assert
   `zheng::root_from_leaves(&lo.leaves) == expected_root`, binding the
   leaves to the state the caller actually trusts.

unlike `verify_particle`'s `QueryProof` (which genuinely lacks a dimension
tag and a root-inclusion path), both ingredients already exist on
`LookOpening` — this looks like a small, well-scoped fix. it is not
attempted here: getting the exact truncation/limb-packing convention wrong
would produce a check that looks like it works (tests could still pass
against self-consistent fixtures) while remaining unsound, and that risk is
worse than leaving the gap documented. this PR is the audit and the
regression, matching row 75/bbg#14's own precedent, not the fix.

## scope checked

`rg -n 'pub fn verify' rs/src` finds three verifiers: `proof::verify_particle`
(row 75, same shape), `query::verify_query` (honestly documented as unbound
in its own doc comment), `query::verify_opening` (this finding — row 75's
own Remains named it "not yet reviewed for the same shape"). `verify_opening`
is `pub use`-exported from `rs/src/lib.rs`, so it is part of bbg's public
API; every call site in this checkout is a test (`rs/src/query.rs`), so the
gap is not yet reachable from production code, but `LookOpening`s a light
client receives (rows 16, 34) would be checked through exactly this
function.

## remains

- implement the two-check fix above, with a second reviewer confirming the
  limb-packing convention matches `root_leaves()` exactly
- decide whether `verify_opening` should take an expected root as a
  parameter (matching `verify_particle`'s shape) or return the recomputed
  root for the caller to compare, since there is no state to read one from
  at verification time
- once light clients start calling `verify_opening` over the wire (row 16,
  row 34), this must be closed before that path is trusted
