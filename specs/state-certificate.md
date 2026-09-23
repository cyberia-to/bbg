# Public state certificate, version 2

The state certificate authenticates complete consumed public dimension tables
against the same 14-leaf root used by BbgState and Zheng. It discloses the
consumed tables and verifies by recomputing their commitments; it provides no
query hiding and does not rely on sampled polynomial acceptance.

## Canonical dimension encoding

Dimension format version 2 changes roots and cell offsets. Recompute old roots
and proofs; no legacy fallback exists. For each dimension, the polynomial's
unpadded field table is `[2, field_count, entry_count, entries...]`. Each entry
contains its 32-byte key as eight little-endian u32 field limbs, followed by the
record fields. Every arbitrary u64 record scalar is two u32 limbs, low first;
every arbitrary 32-byte value is eight u32 limbs. True field values in A remain
single canonical field elements. Keys follow the dimension map's deterministic
ordering. Signals include neuron, network, link_count, block_height, proof_hash.
Root commitments and query proofs use the same entry serializer.

The table pads to exactly its next power of two for Brakedown version 2.
The header binds its unpadded length and entry count. Empty dimensions encode
`[2,3,0]`; padding cells are not addressable. Particle energy is at cell 11 for
the first record: three header fields plus eight key limbs. The following cell
contains its high limb. Native callers reconstruct arbitrary u64 values from
both limbs when needed.

Hemera digest bytes are already four canonical Goldilocks limbs and retain that
representation in the root leaves. Arbitrary keys/IDs use the injective eight
u32 representation; digest decoding must not silently reduce noncanonical words.

## Certificate

`StateCertificate { version, lens_version, leaves, dimensions }` contains the
14x4 canonical u64 root leaves (11 public dimensions, A, N, statistics), and one
`DimensionTable { namespace, fields }` for each consumed namespace. Namespaces
are strictly increasing and restricted to 0..10. Namespace 10 is the opt-in
plaintext balance map from state.md; requesting it explicitly discloses that
complete map. A and N remain private independent commitments and cannot be
requested as dimension tables. The nox look instruction still admits only 0..9.
Fields are complete unpadded
canonical tables with the version/length/count header. Their aggregate size is
at most 2^20 fields; validation checks bounds before rebuilding commitments.

`from_state(state,namespaces)` sorts requested namespaces, rejects duplicates,
checks bounds and rejects a stale cached state root. `root()` validates metadata,
all dimensions, exact recomputed commitments against their root leaves, and
returns Zheng's root compression over all 14 leaves. `verify(expected_root)`
additionally requires equality to the caller's canonical root. `cell(ns,key)`
returns an in-range canonical field value from a structurally valid certificate;
callers must verify the certificate against their expected root before trusting
that value. Missing namespaces and padded/out-of-range indices return None.

A certificate may omit unconsumed dimensions but never replace a consumed table
with a sampled opening. The execution proof must bind each actual namespace,
key, value and all four expected-root limbs. Root-only validation establishes
membership under a caller-supplied root, not historical validity of that root.
