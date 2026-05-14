// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Checkpointing: snapshot BBG root + optional zheng accumulator.

use crate::state::BbgState;
use crate::types::Particle;

/// A BBG checkpoint: root hash + optional proof accumulator + block height.
pub struct Checkpoint {
    pub root: Particle,
    pub acc: Option<zheng::Accumulator>,
    pub height: u64,
}

impl Checkpoint {
    /// Create a checkpoint from the current state.
    pub fn new(state: &BbgState) -> Self {
        Self { root: state.root, acc: None, height: state.height }
    }

    /// Advance the checkpoint to the current state, preserving the accumulator.
    pub fn advance(&self, state: &BbgState) -> Self {
        Self {
            root: state.root,
            acc: self.acc.clone(),
            height: state.height,
        }
    }
}
