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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u64_round_trips_through_goldilocks() {
        for v in [0u64, 1, 42, u32::MAX as u64, u64::MAX / 2] {
            assert_eq!(goldilocks_from_u64(v), Goldilocks::new(v));
        }
    }

    #[test]
    fn bytes32_splits_into_four_little_endian_limbs() {
        let mut b = [0u8; 32];
        // Limb 0 = 1 (LE), limb 1 = 2, limb 2 = 0, limb 3 = u32::MAX as u64.
        b[0] = 1;
        b[8] = 2;
        b[24..32].copy_from_slice(&(u32::MAX as u64).to_le_bytes());
        let limbs = goldilocks_from_bytes32(&b);
        assert_eq!(limbs[0], Goldilocks::new(1));
        assert_eq!(limbs[1], Goldilocks::new(2));
        assert_eq!(limbs[2], Goldilocks::new(0));
        assert_eq!(limbs[3], Goldilocks::new(u32::MAX as u64));
    }

    #[test]
    fn bytes32_all_zero_is_all_zero_limbs() {
        let limbs = goldilocks_from_bytes32(&[0u8; 32]);
        assert_eq!(limbs, [Goldilocks::ZERO; 4]);
    }

    #[test]
    fn commit_dim_is_deterministic() {
        let entries = vec![
            ([1u8; 32], vec![Goldilocks::new(10)]),
            ([2u8; 32], vec![Goldilocks::new(20)]),
        ];
        assert_eq!(commit_dim(&entries), commit_dim(&entries));
    }

    #[test]
    fn commit_dim_distinguishes_key_order() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let forward = vec![(a, vec![Goldilocks::new(10)]), (b, vec![Goldilocks::new(20)])];
        let backward = vec![(b, vec![Goldilocks::new(20)]), (a, vec![Goldilocks::new(10)])];
        assert_ne!(commit_dim(&forward), commit_dim(&backward));
    }

    #[test]
    fn commit_dim_distinguishes_values() {
        let key = [1u8; 32];
        let low = vec![(key, vec![Goldilocks::new(10)])];
        let high = vec![(key, vec![Goldilocks::new(11)])];
        assert_ne!(commit_dim(&low), commit_dim(&high));
    }

    #[test]
    fn commit_dim_empty_is_a_stable_sentinel() {
        assert_eq!(commit_dim(&[]), commit_dim(&[]));
        let non_empty = vec![([1u8; 32], vec![Goldilocks::new(1)])];
        assert_ne!(commit_dim(&[]), commit_dim(&non_empty));
    }
}

