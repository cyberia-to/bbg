//! Versioned, injective dimension serialization shared by roots and reads.
use crate::types::Particle;
use lens::{Commitment, Lens, MultilinearPoly, brakedown::Brakedown};
use nebu::Goldilocks;

pub const DIMENSION_VERSION: u64 = 2;
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
        Goldilocks::new(DIMENSION_VERSION),
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
