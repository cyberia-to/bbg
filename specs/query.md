---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# query

BBG queries address cells in versioned dimension tables. Namespace 0..9 selects
particles, outgoing axons, incoming axons, neurons, locations, coins, cards,
files, time or signals. Balances is dimension 10 and has an entity-keyed API;
current nox `look` accepts only 0..9.

## Cell layout

The canonical layout is defined in [state-certificate](state-certificate.md).
Version 2 begins each unpadded table with three metadata fields, then entries
with eight u32 key limbs and injectively encoded record values. Arbitrary u64
values occupy two u32 cells. A cell key is a flat index, not an entity hash.
Padding cells are unavailable. Root commitments, native reads and proof reads
all use the same serializer, including the Signals network field.

`cell_value(state,dimension,index)` reads the exact field. `open_cell` commits
that same padded table and opens its multilinear polynomial at the Boolean
corner corresponding to the index. `prove_*` helpers locate an entity's primary
value cell. Reading full u64 scalars or records requires all constituent cells.

## Public execution authentication

`StateCertificate::from_state` discloses the complete consumed tables and all
root leaves. Verification recomputes each table's versioned commitment and the
state root. It is linear in the disclosed table sizes and makes no query-hiding
claim. After `certificate.verify(expected_root)`, individual `cell` reads cost
at most ten namespace comparisons and one indexed lookup.

Zheng's state execution statement derives its CCS relation from the actual
program, then binds each active namespace, key, value and four state-root limbs.
Its lookup callback must use a certificate already verified against that exact
root. Verification also authenticates the complete public execution witness and
checks the relation. The certificate and execution proof together establish the
requested public computation under the caller-supplied state root.

## Legacy sampled openings

Legacy contextless `QueryProof` values are low-level dimension openings only.
They are explicitly refused by root/entity verification. Public queries now
carry authenticated context as specified below.

The old recursive `zheng::commit` path rejects these unsupported openings. Empty
or multi-point Brakedown batches also fail closed. No fixed proof-size or
constant verification-time claim applies to these implementations. Relational
query compilation, hidden reads and compact recursive state authentication
require separate protocols and are outside this current public-table contract.

## Authenticated query format, version 3

Public `open_cell` and entity `prove_*` queries carry an optional version-3
context: a `StateCertificate` for exactly the consumed namespace 0..9 and the
unpadded cell index. This discloses the full public dimension, once. Verifiers
recompute its commitment and root, read the exact canonical cell and require
the opening's point and value to match that index. The sampled opening is
checked for consistency only; complete-table authentication establishes the
claim. `verify_particle` additionally locates the exact injective eight-limb
particle key and requires its primary energy cell. Caller-pinned cell and entity
verification APIs bind the requested root, namespace and index/key.

`verify_query` verifies the self-described context only; callers comparing a
particular state or query use `verify_query_at` or `verify_entity`. Legacy queries
without context cannot establish root/entity claims. Existing `prove_balances`
and A queries remain contextless low-level openings; they never implicitly
disclose a full balance table. A legacy `LookOpening` also lacks complete-table context and
is refused by `verify_opening`; `verify_opening_with_context` requires the
corresponding public query, a trusted root and cell index.

Standalone public query generation is bounded to 4096 unpadded fields; larger
public tables use the state-certificate execution API (limit 2^20 fields).
The query decoder accepts only current TensorMerkle openings, canonical points
of at most 20 coordinates, exactly eight value bytes, bounded paths/columns
and at most 16 MiB of opening payload. Unsupported old opening variants fail
closed on decoding; contextless current openings may decode but fail root claims.

## Explicit public balance disclosure

`prove_public_balance(state,owner,token)` explicitly discloses the complete
opt-in plaintext balance dimension 10 (state.md), within the 4096-field bound.
It never reads or discloses private A/N contents. Existing `prove_balances`,
`open_cell` and nox look behavior is unchanged; callers choose the disclosure API.
`verify_public_balance(proof,root,owner,token)` pins the caller's trusted root,
the exact H(owner||token) key, namespace 10 and the primary cell. It reconstructs
the u64 amount from both authenticated u32 cells of the complete table. A single
sampled low limb is insufficient. Wrong keys, roots, modified high limbs and
contextless proofs fail. Absent entries require a separate absence API.
