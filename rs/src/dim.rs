//! Versioned, injective dimension serialization shared by roots and reads —
//! the v1 (flat Brakedown) dimension leaf. Under `RootVersion::V2` the leaf
//! is a keyed-trie root over the same record fields (see [`crate::trie`]).
use crate::types::Particle;
use lens::{Commitment, Lens, MultilinearPoly, brakedown::Brakedown};
use nebu::Goldilocks;

/// The dimension rule set every root derives under (specs/state.md
/// § dimension versions): record schemas, the conviction-free link and zero
/// row rules, and the trie leaf derivation, which carries this number.
/// 3 = `NeuronRecord.nonce`, keyed-trie leaves, no zero records.
pub const DIMENSION_VERSION: u64 = 3;
/// Version word of the flat table layout (`[version, length, entries,
/// (key(8) ‖ fields)*]`) — the v1 dimension polynomial and the state
/// certificate tables zheng authenticates (`zheng::execution::state_evidence::
/// TABLE_VERSION`). The layout is record-agnostic, so a record schema change
/// moves the table contents, not this word.
pub const TABLE_VERSION: u64 = 2;
pub const HEADER_FIELDS: usize = 3;
pub const KEY_FIELDS: usize = 8;

/// Legacy field conversion (modulo p). Commit arbitrary u64 values with u64_limbs.
#[inline]
pub fn goldilocks_from_u64(v: u64) -> Goldilocks {
    Goldilocks::new(v)
}

/// Injective encoding of an arbitrary u64 as two little-endian u32 limbs.
pub fn u64_limbs(v: u64) -> [Goldilocks; 2] {
    [Goldilocks::new(v & 0xffff_ffff), Goldilocks::new(v >> 32)]
}

pub(crate) fn scalar_fields(values: &[u64]) -> Vec<Goldilocks> {
    values.iter().flat_map(|&v| u64_limbs(v)).collect()
}

/// Injective encoding of arbitrary keys and IDs, including non-field bytes.
pub fn bytes32_limbs(bytes: &[u8; 32]) -> [Goldilocks; 8] {
    std::array::from_fn(|i| {
        Goldilocks::new(u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()) as u64)
    })
}

/// Legacy four-field conversion (modulo p). Use bytes32_limbs for arbitrary bytes.
/// Canonical Hemera digests already fit these four fields.
pub fn goldilocks_from_bytes32(bytes: &[u8; 32]) -> [Goldilocks; 4] {
    std::array::from_fn(|i| {
        let value = u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        Goldilocks::new(value)
    })
}

pub(crate) fn dim_serialize(entries: &[(Particle, Vec<Goldilocks>)]) -> Vec<Goldilocks> {
    let count = HEADER_FIELDS
        + entries
            .iter()
            .map(|(_, values)| KEY_FIELDS + values.len())
            .sum::<usize>();
    let mut fields = Vec::with_capacity(count);
    fields.extend([
        Goldilocks::new(TABLE_VERSION),
        Goldilocks::new(count as u64),
        Goldilocks::new(entries.len() as u64),
    ]);
    for (key, values) in entries {
        fields.extend(bytes32_limbs(key));
        fields.extend_from_slice(values);
    }
    fields
}

pub(crate) fn commit_fields(fields: &[Goldilocks]) -> Commitment {
    let mut padded = fields.to_vec();
    padded.resize(padded.len().next_power_of_two(), Goldilocks::ZERO);
    Brakedown::commit(&MultilinearPoly::new(padded))
}

pub fn commit_dim(entries: &[(Particle, Vec<Goldilocks>)]) -> Commitment {
    commit_fields(&dim_serialize(entries))
}

/// Internal commitment digests are canonical by the Hemera hash contract.
pub(crate) fn digest_limbs(bytes: &[u8; 32]) -> [Goldilocks; 4] {
    for chunk in bytes.chunks_exact(8) {
        assert!(
            u64::from_le_bytes(chunk.try_into().unwrap()) < nebu::field::P,
            "noncanonical Hemera digest"
        );
    }
    goldilocks_from_bytes32(bytes)
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
