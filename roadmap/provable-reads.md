---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-06-08
---
# provable reads — a read algebra for authenticated state

a soundness bug in the `look` primitive (below) was the trigger, but the real subject is
larger: **what is the complete set of read methods bbg must expose so that every present
and future consumer can read committed state with a proof?** inf (the language of sets) is
the first heavy consumer; tru, cybergraph, soma and the query compiler ([[verifiable-query]])
follow. "single scalar vs full record" is the narrowest possible slice of that question.
this doc frames the whole read surface, derives the minimal primitive set that spans it, and
positions the immediate fix inside it.

## the demand side — every kind of read the system asks

authenticated state has to answer far more than "value at key". the access patterns:

| # | read question | example | primitive (layer) | scope | status |
|---|---|---|---|---|---|
| 1 | point value | "particle X's energy" | record read → project (L1) | public | **L0 landed**; L1 key-bind next |
| 2 | full record | "X's whole record" | record read (L1) | public | proposed (B) |
| 3 | existence | "is X a particle?" | membership / NMT (L1) | public | NMT exists |
| 4 | non-existence | "X has no axon to Y" (datalog negation) | non-membership / NMT absence (L1) | public | **needed** |
| 5 | completeness | "ALL axons of X, none withheld" | completeness / NMT (L1) | public | NMT exists |
| 6 | range / filter | "neurons with focus > F" | filter over batched open (L2) | public | D / [[verifiable-query]] |
| 7 | join | "particles linked to neuron N" | LogUp (L2) | public | LogUp exists |
| 8 | aggregate | "Σ energy", "count", "max φ*" | accumulator (L2) | public | [[verifiable-query]] |
| 9 | sort / top-k | "top-100 by φ*" | permutation (L2) | public | [[verifiable-query]] |
| 10 | transitive / recursive | "reachable from A ≤ k hops" | folded L2, diameter-bounded (L3) | public | bound contract exists |
| 11 | temporal | "X at height h", "Δ between h₁,h₂" | any read vs historical root (L3) | public | [[temporal-polynomial]] |
| 12 | private / scoped | "my cyberlinks", "my balance" | mutator-set membership + scope (L1′) | private | mutator set exists |
| 13 | statistics | "node_count, diameter_bound" | committed `GraphStats` in root | public | **done** ([[statistics]]) |

three things fall out immediately: (a) most reads are **set-shaped** (6–10), not point-shaped;
(b) **negation/non-existence** (4) is a first-class read inf needs and nothing serves yet;
(c) the per-entity `look` we have today answers only #1 — the narrowest cell.

## does a full record read subsume the single scalar? — yes

the direct question: do we need *both* a single-scalar read and a record read? **no.** a sound
record read returns the entity's cells (or the subset a query touches) under one batched
opening; the single scalar is just *record read, project one column*. with `Lens::batch_open`
(exists — `lens/core/src/lib.rs:47`, Brakedown impl) the record read costs **one** opening
regardless of how many columns it returns, so the scalar primitive buys nothing — not even a
cost saving. **keep one point primitive: the record read.** single-scalar `look` should be
retired as a distinct method (opcode 17 redefined to record-read semantics, or kept as the
projection sugar over it).

and the sharper point: **the per-entity read is not the foundation for set queries at all.**
filter/join/aggregate/sort (the bulk of inf) are proven at the *set* layer over batched
dimension openings — not as N point reads. so the forward-looking answer is not "scalar or
record" but "a point primitive (record read) **and** a set layer", with the set layer doing
most of the work.

## the supply side — the minimal primitive set (layered)

```
L0  opening            sound corner opening + batch_open            ← the atom everything builds on
L1  point reads        record read · membership · non-membership · completeness
L1′ private reads      mutator-set membership (scoped, optionally ZK)
L2  set / relational   filter · join · aggregate · sort/top-k       ← the query→CCS layer
L3  compositions       bounded recursion · temporal (read vs past root)
cross-cutting          full 256-bit addressing · committed cost (stats) · privacy scoping
```

- **L0 — opening.** open a committed cell at its **hypercube corner** so the value *is* the
  stored evaluation (`evaluate(corner)=evals[idx]`), with `batch_open` for many cells in one
  proof. this is the single load-bearing primitive; the current bug is that `look` does not
  use it (see below). it is algebra-agnostic (any [[lens]] backend).
- **L1 — point reads.** *record read* (subsumes scalar) = batched corner openings of an
  entity's key-block + touched value cells, with the key-block constrained to equal the
  requested id (binds value↔entity; key uniqueness gives position). *membership /
  non-membership / completeness* = NMT path / absence / namespace proofs — a tree primitive,
  not a poly open; non-membership (4) and completeness (5) are what make negation and
  "nothing withheld" provable.
- **L1′ — private reads.** cyberlinks/spent/balance live in the mutator set, not the public
  NMTs. reads are membership in AOCL/SWBF, neuron-scoped, and should support not revealing
  *which* record was read (ZK query). distinct trust model — never route private records
  through the public path.
- **L2 — set / relational.** the [[verifiable-query]] compiler: open the dimension
  polynomial(s) once, prove filter as range constraints, join as LogUp, aggregate as a
  running accumulator, sort/top-k as a permutation argument. cost scales with the **query,
  not the data**; no per-entity `look`. this is where a sets language wants to live.
- **L3 — compositions.** recursion = L2 per round, folded into one accumulator, bounded by
  the committed `diameter_bound` (the existing inf↔bbg contract, [[statistics]]). temporal =
  any L0–L2 read evaluated against a historical root via the time dimension
  ([[temporal-polynomial]]).
- **cross-cutting.** full 256-bit entity addressing (today's 8-byte key truncates —
  `look_provider_full_particle_key_not_addressable`); static cost from committed stats (done);
  privacy scoping as a session capability over L1/L1′.

everything reduces to **L0 + a tree primitive (NMT/mutator-set) + CCS arguments**. there is no
need for a zoo of bespoke read opcodes — record read, the set compiler, and the membership
proofs cover the whole table above.

## the concrete defect, and the immediate fix

today (`rs/src/proof.rs`, `rs/src/query.rs`): `commit_dim` lays each dimension as
`[key0(4)|val0(M)|key1(4)|val1(M)|…]` — **every column is already committed**. but
`open_dim(entries,key)` opens at `point = key_bytes_as_field_elements` (the key *value*),
which is **not a hypercube corner**, so `evaluate(point)` is an **MLE fingerprint** mixing the
whole dimension, not a cell. `ProofLookProvider` returns that fingerprint; `BbgLookProvider`
returns the real cell (`look_scalar`) — the two **disagree**, and the fingerprint is unstable
(any insert changes it). nox's `BrakedownLookProvider` is already corner-correct (key = flat
index → `evals[idx]`); only the bbg entity-keyed path is wrong.

**fix (L0, bbg-side) — LANDED 2026-06-08** (`rs/src/proof.rs`, 51 bbg tests green):
`open_dim` now maps (dim, entity) → the entity's flat cell index (walks the sorted entries,
accumulating `4+|val|` per entry — handles variable-width axons), opens at that **corner**
(`corner_point`: `point[j]=(idx>>j)&1`, LSB-first to match `evaluate` and zheng's
`look_openings_from_provider`), and returns `evals[idx]` — the real cell, matching
`look_scalar`. `ProofLookProvider`/`collect_look_openings` now return the real value and
**agree with `BbgLookProvider`** (new tests `proof_provider_value_matches_fast_provider_*`
assert it — a check that would have failed before). absent keys now return `None`.

**residual after this fix (honest scope):** the value is now a real committed cell and its
opening binds to the dimension commitment — but the *verifier circuit* does not yet bind the
opened index to the requested key (a malicious prover could open a different entity's cell).
closing that is L1 record read: also open the key-block at the same entry and constrain it ==
the requested id, inside zheng's CCS. that is the next step, not this one — do not claim full
point-read soundness until it lands.

## the point-read addressing choices, re-situated (L1)

once L0 is sound, how does the VM *name* a point read? these are L1 record-read variants
(A/C are degenerate single-scalar forms; B is the real record read; D is not a point read at
all — it is the L2 set layer, included for contrast):

| criterion | current look | A: per-column ns | B: record read | C: structured key | D: set layer (L2) |
|---|---|---|---|---|---|
| value is the real cell | ❌ fingerprint | ✅ after L0 | ✅ after L0 | ✅ after L0 | ✅ intrinsic |
| granularity | 1 scalar | 1 column | record (subset) | 1 column | whole result set |
| openings per result | 1 (meaningless) | M per entity | 1 batched per entity | M per entity | few per relation |
| cost scales with | — | cols×entities | entities | cols×entities | **query, not data** |
| `ns` stays 0..9 (no nox change) | ✅ | ❌ needs ns≤159 | ✅ | ✅ | ✅ (no look) |
| 256-bit addressing | ❌ | ⚠️ must fix | ⚠️ must fix | ❌ worsens | ✅ by position |
| repos changed | — | nox+bbg | nox+zheng+bbg | bbg only | zheng(compiler)+bbg |
| fit for a *sets* language | ❌ | ⚠️ | ⚠️ point-read | ❌ | ✅ native |
| subsumes single scalar | — | ✅ | ✅ | ✅ | ✅ (1-row query) |

| option | pros | cons |
|---|---|---|
| **current look** | exists; opcode wired | fingerprint not cell; fast≠proof; unstable; **unsound** |
| **A — per-column ns** | reuses look step | changes nox (ns>9 rejected); M openings; column baked into protocol `ns` |
| **B — record read** | sound; 1 batched opening/entity; ns 0..9; columns addressed in inf; subsumes A/C | biggest *primitive* change (nox pattern + zheng step + bbg API); still a point read |
| **C — structured key** | bbg-only; fastest | crams entity+column into one field → worse key width; M openings; a stopgap |
| **D — set layer** | matches inf semantics; cost scales with query; no key-width problem; no nox change | needs the query→CCS compiler; largest *engine* effort; point-by-id still wants B |

## recommendation / roadmap

a single primitive does not win; a **layered set** does:

1. **L0 now (bbg):** the corner-opening fix above — real values + provider consistency. landing
   this commit. (full point-read soundness = L1 key-binding, next.)
2. **L1 record read = Option B** for genuine point queries; **drop A and C** except as
   deliberate stopgaps (A taxes nox forever; C worsens key width). add **non-membership +
   completeness** (table rows 4–5) — negation and "nothing withheld" are not optional for inf.
3. **L2 set layer = Option D** ([[verifiable-query]] compiler) for filter/join/aggregate/sort —
   the home for most inf queries; `look` becomes the 1-row special case.
4. **L3** recursion + temporal as compositions of L2 over folded rounds / historical roots.
5. **cross-cutting:** full 256-bit addressing; privacy scoping for L1′; cost already committed.

key facts that make this tractable: the data is already fully committed (L0 is an
addressing+opening fix, not a layout change); `batch_open` exists; `LogUp`/NMT/temporal
machinery exist; the current look has no production consumers, so opcode 17 is free to
redefine.

## open questions

1. position/key binding (L1): NMT membership vs in-circuit key-block equality + uniqueness —
   which is cheaper folded? (ties into [[algebraic-nmt]].)
2. zheng CCS step to *verify a batched opening* inside the proof — single-point verifier step
   exists; batched needs extending (the `batch_open` prover side already exists).
3. non-membership encoding (L1, row 4): NMT absence proof shape for datalog negation.
4. private reads (L1′): ZK query — prove a scoped read without revealing which record.
5. full 256-bit addressing without blowing up look/record trace columns.

see [[verifiable-query]] (the L2 compiler), [[statistics]] (committed cost contract),
[[temporal-polynomial]] (L3 temporal), [[algebraic-nmt]] (membership primitive),
[[storage-proofs]].
