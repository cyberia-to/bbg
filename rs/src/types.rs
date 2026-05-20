// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Core record types for BBG authenticated state.

/// Particle: 32-byte hemera hash.
pub type Particle = [u8; 32];

/// Neuron identifier: 32-byte hemera hash.
pub type NeuronId = [u8; 32];

/// A particle in the cybergraph.
///
/// axon-particles carry weight/s_yes/s_no/meta_score.
/// content-particles carry energy/pi_star.
/// both share the same record type — unused fields are zero.
pub struct ParticleRecord {
    /// aggregate energy from incoming axons (content-particles)
    pub energy: u64,
    /// focus ranking from tri-kernel (updated by cybergraph, not bbg)
    pub pi_star: u64,
    /// aggregate conviction (axon-particles only)
    pub weight: u64,
    /// ICBS YES reserve (axon-particles only)
    pub s_yes: u64,
    /// ICBS NO reserve (axon-particles only)
    pub s_no: u64,
    /// aggregate valence prediction (axon-particles only)
    pub meta_score: u64,
}

impl ParticleRecord {
    pub fn zero() -> Self {
        Self { energy: 0, pi_star: 0, weight: 0, s_yes: 0, s_no: 0, meta_score: 0 }
    }
}

/// A neuron (agent) in the cybergraph.
pub struct NeuronRecord {
    /// available attention budget
    pub focus: u64,
    /// accumulated BTS score (updated by cybergraph)
    pub karma: u64,
    /// total committed conviction
    pub stake: u64,
}

/// A geolocation record.
pub struct LocationRecord {
    pub lat: i32,
    pub lon: i32,
}

/// A coin (token) record.
pub struct CoinRecord {
    pub total_supply: u64,
}

/// A card in the cards dimension.
///
/// Every cyberlink is a card (A6). This records the current beneficial owner
/// of the conviction box attached to an axon-particle.
pub struct CardRecord {
    /// current owner neuron
    pub owner: NeuronId,
    /// axon-particle this card is bound to
    pub particle: Particle,
}

/// A file availability record.
pub struct FileRecord {
    pub available: bool,
    pub chunk_count: u32,
}

/// A signal finalization record committed to the signals dimension.
pub struct SignalRecord {
    pub neuron: NeuronId,
    pub link_count: u32,
    pub block_height: u64,
    pub proof_hash: Particle,
}
