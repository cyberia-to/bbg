# Implement pattern 17 (look) inline CCS constraints and BBG polynomial parameters

## Background

Pattern 17 (look) reads an authenticated value from BBG state via a Brakedown opening:

```
BBG_verify(root = r5, key = r4, value = r6, proof = r7/r10/r11)
```

`zheng/src/ccs/patterns.rs` returns `trivial_ccs()` for `pattern_look_inline()`.

`zheng/specs/constraints.md` documents two inline constraints for look:

| Constraint | Equation | Semantics |
|------------|----------|-----------|
| C_17a | `r5_t - BBG_root_instance = 0` | r5 must equal the BBG root declared in the Statement |
| C_17b | `eval_point_t - f(r4_t) = 0` | evaluation point is derived from lookup key r4 |

The actual Brakedown opening (~825 constraints) is a folded sub-instance handled via the
same mechanism as pattern 0 (axis); see `lens/.claude/plans/pattern0-axis-folded-opening.md`.

---

## What BBG must specify and provide

### 1. Specify `f(r4)` — key-to-evaluation-point derivation

`bbg/specs/indexes.md` must document the exact mapping from a BBG lookup key (`r4`,
a Goldilocks field element) to a polynomial evaluation point. This is constraint C_17b.

Requirements:
- The derivation must be expressible as degree-1 field operations so it encodes as a
  CCS linear constraint. If it is simply `eval_point = r4` (identity), say so explicitly.
- If the derivation requires a hash (e.g., poseidon2(r4) → coordinates), it cannot be
  degree-1 and must instead be expressed as a sub-pattern referencing pattern 15 (hash).
  In that case, zheng will wire a hash sub-CCS before C_17b.
- Specify the codomain: is `eval_point` a single Goldilocks element, or a vector
  (multilinear evaluation point)?

**Action item for BBG:** add to `bbg/specs/indexes.md`:
```
## Key-to-evaluation-point derivation

eval_point(key) = <formula>
field: Goldilocks (p = 2^64 - 2^32 + 1)
degree: <1 if affine, or "hash" if poseidon2 required>
```

### 2. Specify the BBG state polynomial structure

The folded Brakedown sub-instance for look must know the polynomial being opened. BBG
must specify in `bbg/specs/indexes.md`:

- **Polynomial name and variables:** `BBG_poly(index, key, t)` — what are the concrete
  dimensions? (e.g., `index ∈ [0, N)`, `key ∈ [0, K)`, `t ∈ epochs`)
- **Degree:** total degree and per-variable degree of `BBG_poly`
- **Witness size:** how many field elements constitute a Brakedown opening proof for one
  BBG_poly evaluation? (determines the sub-CCS witness width)
- **Commitment scheme parameters:** which `BrakedownParams` (code rate, matrix shape)
  does BBG use for committing to `BBG_poly`?

zheng will pass these parameters to `lens::brakedown::verifier_ccs(&params)` to obtain
the folded sub-CCS.

---

## What zheng will implement once BBG specifies these

### `Statement` update (`src/types.rs`)

```rust
pub struct Statement {
    pub program_hash:  [u8; 32],
    pub input_hash:    [u8; 32],
    pub output_hash:   [u8; 32],
    pub focus_bound:   u64,
    pub bbg_root:      Option<[u8; 32]>,  // required when trace uses pattern 17
}
```

The verifier rejects a proof if the trace contains pattern-17 rows but `bbg_root` is
`None`, or if any `r5_t` in a look row does not equal the declared `bbg_root`.

### `pattern_look_inline()` (`src/ccs/patterns.rs`)

```rust
fn pattern_look_inline() -> CCSInstance {
    // C_17a: r5_t - bbg_root_wire = 0
    //   bbg_root_wire is a constant contribution from CONST_IDX encoding Statement.bbg_root
    // C_17b: eval_point_t - f(r4_t) = 0
    //   if f is identity: direct degree-1 constraint
    //   if f is poseidon2: wire in pattern_hash() sub-CCS
    // m = 1 (or 2 if f requires hash sub-constraint)
}
```

### Folded sub-instance (`src/lib.rs commit()`)

When the trace contains pattern-17 rows, `zheng::commit()`:

1. Extracts `(root, key, value, proof)` from those rows.
2. Verifies `root == statement.bbg_root.unwrap()`.
3. Calls `lens::brakedown::verifier_ccs(&bbg_params)` for the BBG polynomial params.
4. Folds the resulting `CCSInstance` into the main accumulator (shared helper with
   pattern-0 axis fold).

---

## Cross-repo coordination checklist

- [ ] `bbg/specs/indexes.md` — specify `f(r4)` formula and BBG polynomial structure
- [ ] `lens` — expose `verifier_ccs()` (see lens plan; shared requirement with pattern 0)
- [ ] `zheng/src/types.rs` — add `bbg_root: Option<[u8; 32]>` to `Statement`
- [ ] `zheng/src/ccs/patterns.rs` — implement `pattern_look_inline()`
- [ ] `zheng/src/lib.rs` — add look sub-instance folding

## Integration test

A nox program that:
1. Inserts a key-value pair into BBG state (obtaining a root)
2. Executes a look opcode to read the value back
3. The full execution trace is proved by zheng with `Statement { bbg_root: Some(root) }`
4. The verifier accepts

This test lives in `zheng/tests/integration/` and depends on both nox and bbg being
importable as test dependencies.

---

## Coordination notes

- C_17a (root binding) can be implemented in zheng immediately — it is a constant check
  that does not depend on the BBG polynomial spec.
- C_17b and the folded sub-instance are blocked on BBG specifying `f(r4)` and polynomial
  parameters.
- If `f(r4)` requires hash, pattern 15 (hash) must be non-trivial first; see
  `nox/.claude/plans/pattern-bit-decomp.md` for the hash trace work.
- The `lens::brakedown::verifier_ccs` function is a shared dependency with pattern 0;
  whichever is implemented first will unblock both.
