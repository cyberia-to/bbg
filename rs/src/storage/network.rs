// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Network storage trait for L3 content retrieval.
//!
//! BBG holds an optional `Box<dyn NetworkStore>` injected by cybergraph at
//! init. On a local ShardStore miss, BBG delegates to NetworkStore::fetch.
//! Transport is not owned by BBG — the impl lives in cybergraph/radio.

use crate::proof::QueryProof;
use crate::types::Particle;

/// Content retrieval from the network tier (L3).
///
/// Injected by cybergraph; BBG holds an optional reference and delegates
/// on local miss. Two operations:
///
/// - `fetch`: retrieve raw particle content bytes
/// - `das_sample`: open a Lens proof for a byte range (DAS challenge/response)
pub trait NetworkStore: Send + Sync {
    /// Fetch raw particle content by particle. Returns None if unreachable.
    fn fetch(&self, particle: &Particle) -> Option<Vec<u8>>;

    /// DAS sample: open a Lens proof for `length` bytes at `offset` within particle.
    fn das_sample(&self, particle: &Particle, offset: u64, length: u64) -> Option<QueryProof>;
}
