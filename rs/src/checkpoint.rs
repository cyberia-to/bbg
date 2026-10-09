// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Checkpointing: snapshot BBG root + optional proof slot.

use crate::state::BbgState;
use crate::types::Particle;

/// An opaque, versioned proof attached to a checkpoint.
///
/// No profile is defined yet. The slot used to hold zheng's legacy folding
/// accumulator, whose fold the verifier never checked; it proved nothing
/// about the checkpoint and is gone. A sound transition proof (zheng
/// certificate v3 today, an accumulated profile later) lands here under a
/// profile byte, decoded and verified by its owner — bbg only carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointProof {
    /// Proof profile; defined by the proof's owner, never interpreted here.
    pub profile: u8,
    /// Canonical proof bytes of that profile.
    pub bytes: Vec<u8>,
}

/// A BBG checkpoint: root hash + optional proof + block height.
pub struct Checkpoint {
    pub root: Particle,
    /// Proof slot (see [`CheckpointProof`]). The field keeps its old name so
    /// callers that write `acc: None` build unchanged; native persistence
    /// refuses a checkpoint that carries one.
    pub acc: Option<CheckpointProof>,
    pub height: u64,
}

impl Checkpoint {
    /// Create a checkpoint from the current state.
    pub fn new(state: &BbgState) -> Self {
        Self {
            root: state.root(),
            acc: None,
            height: state.height,
        }
    }

    /// Advance the checkpoint to the current state. The proof belonged to
    /// the old root, so it is dropped: carrying it forward would attach a
    /// proof of one state to another.
    pub fn advance(&self, state: &BbgState) -> Self {
        Self {
            root: state.root(),
            acc: None,
            height: state.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_drops_a_proof_of_the_previous_root() {
        let state = BbgState::new();
        let mut checkpoint = Checkpoint::new(&state);
        checkpoint.acc = Some(CheckpointProof {
            profile: 1,
            bytes: vec![7; 4],
        });
        let advanced = checkpoint.advance(&state);
        assert_eq!(
            (advanced.root, advanced.height),
            (state.root(), state.height)
        );
        assert!(advanced.acc.is_none());
    }
}
