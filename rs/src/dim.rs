// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Dimension commitment helpers for BBG authenticated state — root version v1.
//!
//! A dimension is a sorted map of `(key: Particle, value: &[u8])` entries
//! committed via Brakedown over a MultilinearPoly. Under `RootVersion::V2`
//! the dimension leaf is a keyed-trie root instead (see [`crate::trie`]).

use lens::{brakedown::Brakedown, Commitment, Lens, MultilinearPoly};
use nebu::Goldilocks;

use crate::types::Particle;

/// Serialize a `u64` value to one Goldilocks element.
#[inline]
pub fn goldilocks_from_u64(v: u64) -> Goldilocks {
    Goldilocks::new(v)
}

/// A `u64` as two 32-bit limbs, low first — injective in the field.
///
/// One Goldilocks limb per `u64` is not: `v` and `v − p` collide for
/// `v ≥ p`. v2 leaves (and the chain's dimension version ≥ 2) use this.
pub fn u64_limbs(v: u64) -> [Goldilocks; 2] {
    [Goldilocks::new(v & 0xffff_ffff), Goldilocks::new(v >> 32)]
}

/// A 32-byte array as eight 32-bit limbs (little-endian) — injective.
pub fn bytes32_limbs(b: &[u8; 32]) -> [Goldilocks; 8] {
    std::array::from_fn(|i| {
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&b[i * 4..i * 4 + 4]);
        Goldilocks::new(u32::from_le_bytes(buf) as u64)
    })
}

/// Serialize a 32-byte array to 4 Goldilocks elements (8 bytes each, LE).
/// The limbs are raw `u64`s, so this is the root/commitment limb packing, not
/// an injective field encoding (see [`bytes32_limbs`]).
pub fn goldilocks_from_bytes32(b: &[u8; 32]) -> [Goldilocks; 4] {
    let mut out = [Goldilocks::ZERO; 4];
    for (i, chunk) in b.chunks_exact(8).enumerate() {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(chunk);
        out[i] = Goldilocks::new(u64::from_le_bytes(buf));
    }
    out
}

/// Commit a sorted list of `(key, raw_field_elements)` pairs.
///
/// - Each key is serialized as 4 Goldilocks elements.
/// - Each value element is one Goldilocks per u64.
/// - The concatenated list is padded to the next power of 2.
/// - Empty dimension → `Brakedown::commit_raw(b"bbg-empty-dim" encoded)`.
pub fn commit_dim(entries: &[(Particle, Vec<Goldilocks>)]) -> Commitment {
    if entries.is_empty() {
        // Use commit_raw on a canonical empty sentinel so the type is the
        // same `Commitment` that lens/0.2 exports.
        let sentinel: Vec<Goldilocks> = b"bbg-empty-dim"
            .iter()
            .map(|&b| Goldilocks::new(b as u64))
            .collect();
        let target = sentinel.len().next_power_of_two();
        let mut padded = sentinel;
        padded.resize(target, Goldilocks::ZERO);
        return Brakedown::commit_raw(&padded);
    }

    let mut elems: Vec<Goldilocks> = Vec::new();
    for (key, vals) in entries {
        let key_elems = goldilocks_from_bytes32(key);
        elems.extend_from_slice(&key_elems);
        elems.extend_from_slice(vals);
    }

    // Pad to next power of 2.
    let target = elems.len().next_power_of_two();
    elems.resize(target, Goldilocks::ZERO);

    let poly = MultilinearPoly::new(elems);
    Brakedown::commit(&poly)
}

