// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Storage backends for BBG polynomial state.
//!
//! Five backends selected by deployment scale and hardware:
//!
//! | backend | crate          | tier  | notes                        |
//! |---------|----------------|-------|------------------------------|
//! | memory  | std HashMap    | hot   | default, always available    |
//! | unimem  | honeycrisp     | hot   | Apple Silicon zero-copy      |
//! | ssd     | fjall          | warm  | feature = "backend-ssd"      |
//! | hdd     | redb           | cold  | feature = "backend-hdd"      |
//! | network | NetworkStore   | L3    | injected by cybergraph       |
//!
//! ShardStore and NetworkStore are independent traits. ShardStore covers
//! local tiers (memory → hdd). NetworkStore is a separate injection point
//! for content retrieval; BBG owns tier routing but not transport.

pub mod mem;
pub mod network;
pub mod tiered;

#[cfg(feature = "backend-ssd")]
pub mod fjall;

#[cfg(feature = "backend-hdd")]
pub mod redb;

#[cfg(all(target_os = "macos", feature = "backend-unimem"))]
pub mod unimem;

pub use mem::MemStore;
pub use network::NetworkStore;
pub use tiered::TieredStore;

#[cfg(feature = "backend-ssd")]
pub use fjall::FjallStore;

#[cfg(feature = "backend-hdd")]
pub use redb::RedbStore;

#[cfg(all(target_os = "macos", feature = "backend-unimem"))]
pub use unimem::UnimemStore;

use nebu::Goldilocks;

/// Dimension identifiers — maps to the 10 BBG_poly dimensions + 2 private polynomials.
pub mod dim {
    pub const PARTICLES:   u8 = 0;
    pub const AXONS_OUT:   u8 = 1;
    pub const AXONS_IN:    u8 = 2;
    pub const NEURONS:     u8 = 3;
    pub const LOCATIONS:   u8 = 4;
    pub const COINS:       u8 = 5;
    pub const CARDS:       u8 = 6;
    pub const FILES:       u8 = 7;
    pub const TIME:        u8 = 8;
    pub const SIGNALS:     u8 = 9;
    /// A(x) — private commitment polynomial (NOT a BBG_poly dimension)
    pub const COMMITMENTS: u8 = 10;
    /// N(x) — private nullifier polynomial (NOT a BBG_poly dimension)
    pub const NULLIFIERS:  u8 = 11;
}

/// Storage interface for a polynomial evaluation shard.
///
/// Authentication is provided by the Lens commitment layer — the store has
/// no opinion on correctness. `commit()` returns a shard sub-root for change
/// tracking only; the authoritative commitment is computed by `dim.rs`.
pub trait ShardStore: Send + Sync {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]>;
    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>);
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)];
    fn commit(&mut self) -> [u8; 32];
}

/// Serialize a Goldilocks slice to LE bytes (8 bytes per element).
#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
pub(crate) fn serialize_goldilocks(vals: &[Goldilocks]) -> Vec<u8> {
    vals.iter().flat_map(|g| g.as_u64().to_le_bytes()).collect()
}

/// Deserialize LE bytes back to Goldilocks elements.
#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
pub(crate) fn deserialize_goldilocks(bytes: &[u8]) -> Vec<Goldilocks> {
    bytes
        .chunks_exact(8)
        .map(|c| {
            let mut b = [0u8; 8];
            b.copy_from_slice(c);
            Goldilocks::new(u64::from_le_bytes(b))
        })
        .collect()
}

/// Compute a shard sub-commitment from the dirty key list (hemera hash).
pub(crate) fn hash_dirty(dirty: &[(u8, [u8; 32], Vec<Goldilocks>)]) -> [u8; 32] {
    use hemera::hash as hemera_hash;
    let mut buf: Vec<u8> = Vec::with_capacity(dirty.len() * 33);
    for (d, k, _) in dirty {
        buf.push(*d);
        buf.extend_from_slice(k);
    }
    let h = hemera_hash(&buf);
    let b = h.as_bytes();
    let mut out = [0u8; 32];
    out[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
    out
}
