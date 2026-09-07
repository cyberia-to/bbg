---
tags: bbg, docs
crystal-type: entity
crystal-domain: cyber
---
# QueryProof wire format

Reference for the serialized shape of `bbg::QueryProof` — the proof object a
BBG read returns and a light client verifies. This is the schema seed for the
soft3 SDK's query wire protocol.

Serialization is behind the `serde` cargo feature in both `bbg` and
`cyber-lens` (`bbg = { features = ["serde"] }` enables the whole chain:
bbg → lens → hemera). The format is pinned by golden-fixture tests:
`bbg/rs/tests/serde_wire.rs` and `lens/core/tests/serde_roundtrip.rs`.
A format change breaks those tests; it cannot drift silently.

## shape

`QueryProof` serializes as a struct of four fields (JSON shown; any serde
format works):

```json
{
  "commitment": [ /* 32 bytes, u8 each — hemera Hash of the dimension poly */ ],
  "opening":    { /* one Opening variant, externally tagged — see below */ },
  "value_bytes": [ /* 8 bytes, little-endian u64: the opened Goldilocks value */ ],
  "point":      [ /* canonical u64 per element, in [0, p), p = 2^64 - 2^32 + 1 */ ]
}
```

- `commitment` — `lens::Commitment`, a newtype over `hemera::Hash`;
  serializes as a fixed 32-tuple of bytes.
- `opening` — `lens::Opening`, externally tagged enum (standard serde):
  `{"Tensor": {...}}`, `{"Folding": {...}}`, or `{"Witness": {...}}`.
  BBG opens cells via Brakedown, so bbg-produced proofs always carry
  `Tensor`.
- `value_bytes` — little-endian u64 encoding of the opened cell value.
- `point` — the hypercube corner (LSB-first) of the opened cell. Canonical
  encoding: deserialization rejects any element ≥ p, so each point has
  exactly one valid wire form.

## Opening variants

```json
{"Tensor": {
  "round_commitments": [ /* Commitment... */ ],
  "final_poly":        [ /* bytes */ ],
  "query_responses":   [ [ /* index */, [ /* bytes */ ] ] /* ... */ ]
}}

{"Folding": {
  "round_commitments": [ /* Commitment... */ ],
  "merkle_paths":      [ [ /* Hash (32-tuple of bytes)... */ ] ],
  "final_value":       [ /* bytes */ ]
}}

{"Witness": {
  "witness_commitment": /* Commitment */,
  "witness_opening":    { /* nested Opening */ },
  "certificate":        [ /* bytes */ ]
}}
```

Tensor = Brakedown / Ikat / Porphyry, Folding = Binius, Witness = Assayer.

## root binding

`QueryProof.commitment` is the dimension commitment — one of the 14 leaves of
BBG_root (`BbgState::root_leaves()`). A verifier that also holds the root
checks `root_from_leaves(leaves) == BBG_root` and that `leaves.dims[dim]`
matches `commitment`; the proof then binds the opened value all the way to the
state root. `BBG_root` itself is a `Particle` (`[u8; 32]`) — four Goldilocks
limbs, little-endian — and needs no special serde.
