// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Signal and cyberlink types — the single input to BBG state.

use crate::types::{Cid, NeuronId};

/// A cyberlink: the five-field atomic unit of knowledge.
/// ℓ = (p, q, τ, a, v)
pub struct Cyberlink {
    /// source particle
    pub from: Cid,
    /// target particle
    pub to: Cid,
    /// token denomination
    pub token: Cid,
    /// conviction amount (focus consumed)
    pub amount: u64,
    /// epistemic valence: -1, 0, or +1
    pub valence: i8,
}

/// A nullifier movement: spend a conviction UTXO and optionally create a new one.
pub struct UtxoMove {
    /// nullifier of the spent UTXO
    pub nullifier: Cid,
    /// new commitment point (if creating an output)
    pub commitment: Option<(Cid, u64)>,
}

/// A signal: the atomic broadcast unit from a neuron.
/// s = (ν, ℓ⃗, Δφ*, σ, t)
///
/// cybergraph validates σ and all semantic constraints before calling bbg.insert.
/// BBG only checks the structural double-spend invariant.
pub struct Signal {
    /// signing neuron
    pub neuron: NeuronId,
    /// one or more cyberlinks
    pub links: Vec<Cyberlink>,
    /// UTXO movements (conviction spends and outputs)
    pub utxo_moves: Vec<UtxoMove>,
    /// block height
    pub height: u64,
}

/// Error returned by bbg.insert — structural check only.
#[derive(Debug, PartialEq)]
pub enum InsertError {
    /// N(nullifier) = 0: this UTXO was already spent.
    DoubleSpend,
}
