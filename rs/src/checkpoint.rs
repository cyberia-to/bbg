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
        Self { root: state.root(), acc: None, height: state.height }
    }

    /// Advance the checkpoint to the current state, preserving the accumulator.
    pub fn advance(&self, state: &BbgState) -> Self {
        Self {
            root: state.root(),
            acc: self.acc.clone(),
            height: state.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NeuronRecord;

    fn particle(seed: u8) -> Particle {
        [seed; 32]
    }

    #[test]
    fn new_captures_root_and_height_with_no_accumulator() {
        let mut state = BbgState::new();
        state.height = 3;
        state.neurons.insert(particle(1), NeuronRecord { focus: 1, karma: 0, stake: 0 });

        let cp = Checkpoint::new(&state);

        assert_eq!(cp.root, state.root());
        assert_eq!(cp.height, 3);
        assert!(cp.acc.is_none());
    }

    #[test]
    fn advance_reflects_the_new_state() {
        let mut state = BbgState::new();
        state.neurons.insert(particle(1), NeuronRecord { focus: 1, karma: 0, stake: 0 });
        let cp = Checkpoint::new(&state);
        let root_before = cp.root;

        state.neurons.insert(particle(2), NeuronRecord { focus: 2, karma: 0, stake: 0 });
        state.height = 1;
        state.refresh_root();
        let advanced = cp.advance(&state);

        assert_ne!(advanced.root, root_before);
        assert_eq!(advanced.root, state.root());
        assert_eq!(advanced.height, 1);
    }

    #[test]
    fn advance_preserves_a_none_accumulator() {
        let state = BbgState::new();
        let cp = Checkpoint::new(&state);

        let advanced = cp.advance(&state);

        assert!(advanced.acc.is_none());
    }
}
