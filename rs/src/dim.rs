// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Dimension commitment helpers for BBG authenticated state.
//!
//! A dimension is a sorted map of `(key: Particle, value: &[u8])` entries
//! committed via Brakedown over a MultilinearPoly.

use lens::{brakedown::Brakedown, Commitment, Lens, MultilinearPoly};
use nebu::Goldilocks;

use crate::types::Particle;

/// Serialize a `u64` value to one Goldilocks element.
#[inline]
pub fn goldilocks_from_u64(v: u64) -> Goldilocks {
    Goldilocks::new(v)
}

/// Serialize a 32-byte array to 4 Goldilocks elements (8 bytes each, LE).
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

/// Compute the BBG_poly commit: hemera hash of the concatenated 32-byte
/// commitments for all 10 dimensions (d0..d9).
///
/// Uses `Brakedown::commit_raw` so all hemera types stay in the 0.2 version.
pub fn bbg_poly_commit(dim_commits: &[Commitment; 10]) -> Particle {
    let mut buf = Vec::with_capacity(320); // 10 × 32
    for c in dim_commits.iter() {
        buf.extend_from_slice(c.as_bytes());
    }
    // Encode the buffer as field elements and commit_raw to stay in 0.2 hemera.
    let elems: Vec<Goldilocks> = buf.iter().map(|&b| Goldilocks::new(b as u64)).collect();
    let target = elems.len().next_power_of_two();
    let mut padded = elems;
    padded.resize(target, Goldilocks::ZERO);
    let commit = Brakedown::commit_raw(&padded);
    // Extract the 32 bytes from the commitment.
    let mut out = [0u8; 32];
    let cb = commit.as_bytes();
    let len = cb.len().min(32);
    out[..len].copy_from_slice(&cb[..len]);
    out
}

