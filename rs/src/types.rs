// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Core record types for BBG authenticated state.

/// Particle: 32-byte hemera hash.
pub type Particle = [u8; 32];

/// Neuron identifier: 32-byte hemera hash.
pub use neuron_id::NeuronId;

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
    /// transactions this neuron has authored on the root chain
    /// (soft3/specs/chain.md §2.5, §3.2): the replay counter, under the root
    /// since dimension version 3
    pub nonce: u64,
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

/// The side of an axon a position pushes on (soft3 chain-economy §4.2):
/// `Plus` adds weight (v ∈ {0, +1}), `Minus` subtracts it (v = −1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Plus,
    Minus,
}

/// The valence v ∈ {−1, 0, +1} of a position, stored as the byte `v + 1`
/// (`specs/state.md` rule 8). Any other byte is not a valence.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Valence {
    /// v = −1: active stake against the edge
    Against = 0,
    /// v = 0: passive stake; adds weight, never earns
    Neutral = 1,
    /// v = +1: active stake for the edge
    For = 2,
}

impl Valence {
    /// From v ∈ {−1, 0, +1}.
    pub const fn from_i8(v: i8) -> Option<Self> {
        match v {
            -1 => Some(Self::Against),
            0 => Some(Self::Neutral),
            1 => Some(Self::For),
            _ => None,
        }
    }

    /// v ∈ {−1, 0, +1}.
    pub const fn to_i8(self) -> i8 {
        self as i8 - 1
    }

    /// From the stored byte `v + 1` ∈ {0, 1, 2}.
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::Against),
            1 => Some(Self::Neutral),
            2 => Some(Self::For),
            _ => None,
        }
    }

    /// The stored byte `v + 1`.
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// `Minus` for v = −1, `Plus` otherwise.
    pub const fn side(self) -> Side {
        match self {
            Self::Against => Side::Minus,
            Self::Neutral | Self::For => Side::Plus,
        }
    }

    /// v ≠ 0: the position takes epistemic risk and may earn.
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Neutral)
    }
}

/// A position in the cards dimension (dimension version 4, `specs/state.md`
/// rule 8): one author's locked stake on one axon, at
/// [`crate::position::card_id`]`(author, p, q)`. The stake is not a field: it
/// is the balance at [`crate::position::stake_key`]`(card)`.
///
/// Field order is the leaf order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardRecord {
    /// payout owner: rewards go to `balances[H(owner ‖ τ_native)]`
    pub owner: [u8; 32],
    /// the axon-particle `axon_id(p, q)`
    pub particle: Particle,
    /// the neuron that locked the stake; only it may unlock
    pub author: NeuronId,
    /// fixed while the stake is positive
    pub valence: Valence,
    /// height of the last stake increase
    pub since: u64,
}

/// A file availability record.
pub struct FileRecord {
    pub available: bool,
    pub chunk_count: u32,
}

/// A signal finalization record committed to the signals dimension.
pub struct SignalRecord {
    pub neuron: NeuronId,
    /// destination network this signal was delivered to (a card id)
    pub network: Particle,
    pub link_count: u32,
    pub block_height: u64,
    pub proof_hash: Particle,
    /// the neuron's own signal-chain step (0 for its first signal). The
    /// record lives at `state::signal_key(neuron, step)`: steps are
    /// per-neuron counters, so the step alone does not name a signal.
    pub step: u64,
}

/// An intent record committed to the intents dimension.
///
/// An intent is an unsealed declaration: the neuron commits to a scope
/// and signs it, but the STARK proof is deferred until sealing. If the
/// intent is never sealed, the record persists at its inception height —
/// abandonment is on the record.
pub struct IntentRecord {
    /// declaring neuron
    pub neuron: NeuronId,
    /// inception height
    pub h0: u64,
    /// hash of the structured scope (target, predicate, deadline, constraints)
    pub scope_hash: Particle,
    /// neuron's signature over (ν ‖ h0 ‖ scope_hash)
    pub signature: [u8; 64],
}
