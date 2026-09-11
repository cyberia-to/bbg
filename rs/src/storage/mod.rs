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
//! | memory  | std BTreeMap   | hot   | default, always available    |
//! | unimem  | honeycrisp     | hot   | Apple Silicon zero-copy      |
//! | ssd     | fjall          | warm  | feature = "backend-ssd"      |
//! | hdd     | redb           | cold  | feature = "backend-hdd"      |
//! | network | NetworkStore   | L3    | injected by cybergraph       |
//!
//! ShardStore and NetworkStore are independent traits. ShardStore covers
//! local tiers (memory → hdd). NetworkStore is a separate injection point
//! for content retrieval; BBG owns tier routing but not transport.

mod access;
mod buffer;
pub mod mem;
pub mod network;
pub mod tiered;
pub use access::{
    Durability, MAX_VALUE_ELEMENTS, ScanLimits, ShardEntry, StorageError, StorageResult,
};

#[cfg(feature = "backend-hdd")]
pub mod application;

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

/// Dimension identifiers — 10 BBG_poly dimensions + 2 private polynomials + 1 intent + 1 local-only.
pub mod dim {
    pub const PARTICLES: u8 = 0;
    pub const AXONS_OUT: u8 = 1;
    pub const AXONS_IN: u8 = 2;
    pub const NEURONS: u8 = 3;
    pub const LOCATIONS: u8 = 4;
    pub const COINS: u8 = 5;
    pub const CARDS: u8 = 6;
    pub const FILES: u8 = 7;
    pub const TIME: u8 = 8;
    pub const SIGNALS: u8 = 9;
    /// A(x) — private commitment polynomial (NOT a BBG_poly dimension)
    pub const COMMITMENTS: u8 = 10;
    /// N(x) — private nullifier polynomial (NOT a BBG_poly dimension)
    pub const NULLIFIERS: u8 = 11;
    /// Unsealed intent records — declared scope + identity proof, no STARK yet.
    /// Persisted independently from signals so abandonment is on the record.
    pub const INTENTS: u8 = 12;
    /// Local-only state (Transform, AnimationPhase, etc.).
    /// put() skips dirty push; commit() never includes these entries.
    /// Never contributes to BBG_root and never written to warm/cold tiers.
    pub const EPHEMERAL: u8 = 13;
}

/// Storage interface for a polynomial evaluation shard.
///
/// Authentication is provided by the Lens commitment layer — the store has
/// no opinion on correctness. `commit()` returns a change identity only;
/// the authoritative commitment is computed by `dim.rs`.
pub trait ShardStore: Send + Sync {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]>;
    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>) -> StorageResult<()>;
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)];
    fn commit(&mut self) -> StorageResult<[u8; 32]>;

    /// Owned read, including staged mutations, with an explicit allocation bound.
    fn read(
        &self,
        dimension: u8,
        key: &[u8; 32],
        max_elements: usize,
    ) -> StorageResult<Option<Vec<Goldilocks>>> {
        access::check_dimension(dimension)?;
        access::check_read_limit(max_elements)?;
        self.get(dimension, key)
            .map(|v| access::copy_value(v, max_elements))
            .transpose()
    }

    /// Ascending committed disk entries after `after`; cache-only stores may
    /// expose their current values. Disk implementations reject pending writes.
    fn scan(
        &self,
        _dimension: u8,
        _after: Option<[u8; 32]>,
        _limits: ScanLimits,
    ) -> StorageResult<Vec<ShardEntry>> {
        Err(StorageError::Unsupported("bounded scan"))
    }

    fn durability(&self) -> Durability {
        Durability::Memory
    }
    fn last_commit(&self) -> StorageResult<Option<[u8; 32]>> {
        Ok(None)
    }
    fn is_poisoned(&self) -> bool {
        false
    }
    fn has_pending(&self) -> bool {
        !self.dirty_entries().is_empty()
    }

    /// In-place mutation. Caller must call `mark_dirty` after writing.
    /// Returns `None` on disk backends (fjall, redb) or if key is absent.
    fn get_mut(&mut self, dimension: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]>;

    /// Marks an existing entry dirty for the next `commit()`.
    /// No-op for `dim::EPHEMERAL` and on disk backends.
    fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]) -> StorageResult<()>;

    /// Removes an entry. Returns the previous value, or `None` if absent.
    fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<Option<Vec<Goldilocks>>>;

    /// Iterates all entries stored for a dimension.
    /// On disk backends, only entries loaded into the write-through cache are visible.
    fn iter(&self, dimension: u8) -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_>;
}

/// Serialize a Goldilocks slice to LE bytes (8 bytes per element).
pub(crate) fn serialize_goldilocks(vals: &[Goldilocks]) -> Vec<u8> {
    vals.iter().flat_map(|g| g.as_u64().to_le_bytes()).collect()
}

/// Deserialize LE bytes back to Goldilocks elements.
#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
pub(crate) fn deserialize_goldilocks(
    bytes: &[u8],
    max_elements: usize,
) -> StorageResult<Vec<Goldilocks>> {
    access::check_read_limit(max_elements)?;
    if !bytes.len().is_multiple_of(8) {
        return Err(StorageError::Corrupt("field encoding length"));
    }
    if bytes.len() / 8 > max_elements {
        return Err(StorageError::Limit("value exceeds read budget"));
    }
    bytes
        .chunks_exact(8)
        .map(|c| {
            let mut b = [0u8; 8];
            b.copy_from_slice(c);
            let value = u64::from_le_bytes(b);
            if value >= nebu::field::P {
                return Err(StorageError::Corrupt("noncanonical field element"));
            }
            Ok(Goldilocks::new(value))
        })
        .collect()
}

#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
pub(crate) fn decode_marker(bytes: &[u8]) -> StorageResult<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| StorageError::Corrupt("commit marker length"))
}

#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
pub(crate) fn sync_parent(path: &std::path::Path) -> StorageResult<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::File::open(parent)
        .map_err(access::io)?
        .sync_all()
        .map_err(access::io)
}

pub(crate) fn scan_cache(
    cache: &std::collections::BTreeMap<(u8, [u8; 32]), Vec<Goldilocks>>,
    dimension: u8,
    after: Option<[u8; 32]>,
    limits: ScanLimits,
) -> StorageResult<Vec<ShardEntry>> {
    use std::ops::Bound::{Excluded, Included};
    access::check_dimension(dimension)?;
    limits.validate()?;
    let start = after.map_or(Included((dimension, [0; 32])), |key| {
        Excluded((dimension, key))
    });
    let end = Included((dimension, [255; 32]));
    access::collect_page(
        cache
            .range((start, end))
            .map(|((_, k), v)| Ok((*k, access::copy_value(v, limits.max_elements)?))),
        limits,
    )
}

/// Compute a shard sub-commitment from the dirty key list (hemera hash).
#[cfg(all(target_os = "macos", feature = "backend-unimem"))]
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
