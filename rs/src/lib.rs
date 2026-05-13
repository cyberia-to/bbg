// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! bbg — Big Badass Graph: authenticated state with polynomial commitments.
//!
//! BBG has one operation: insert(signal).
//! All semantic validation (A1–A3, focus sufficiency, UTXO ownership,
//! conservation, VDF) is the responsibility of cybergraph.
//! BBG only enforces the structural double-spend invariant via N(x).

pub mod checkpoint;
pub mod dim;
pub mod proof;
pub mod signal;
pub mod state;
pub mod storage;
pub mod types;

pub use checkpoint::Checkpoint;
pub use proof::{
    prove_axons_in, prove_axons_out, prove_card, prove_coin, prove_commitment, prove_file,
    prove_location, prove_neuron, prove_particle, prove_signal, prove_time, verify_particle,
    QueryProof,
};
pub use signal::{Cyberlink, InsertError, Signal, UtxoMove};
pub use state::BbgState;
pub use types::{Cid, NeuronId};

/// The BBG facade: state + checkpoint as a single unit.
pub struct Bbg {
    pub state: BbgState,
    pub checkpoint: Checkpoint,
}

impl Bbg {
    pub fn new() -> Self {
        let state = BbgState::new();
        let checkpoint = Checkpoint::new(&state);
        Self { state, checkpoint }
    }

    /// Insert a pre-validated signal. Fails only on structural double-spend.
    pub fn insert(&mut self, signal: &Signal) -> Result<(), InsertError> {
        self.state.insert(signal)
    }

    /// Finalize the current block: record a time snapshot, increment height,
    /// and run decay+pruning at epoch boundaries.
    pub fn finalize_block(&mut self) {
        let h = self.state.height;
        let root = self.state.root;
        self.state.time.insert(h, root);
        self.state.root = self.state.compute_root();
        self.state.height += 1;
        if self.state.height % state::EPOCH_BLOCKS == 0 {
            self.state.apply_decay_and_prune();
        }
        self.checkpoint = self.checkpoint.advance(&self.state);
    }

    pub fn prove_particle(&self, cid: &Cid) -> Option<QueryProof> {
        prove_particle(&self.state, cid)
    }

    pub fn prove_neuron(&self, id: &NeuronId) -> Option<QueryProof> {
        prove_neuron(&self.state, id)
    }

    pub fn prove_axons_out(&self, cid: &Cid) -> Option<QueryProof> {
        prove_axons_out(&self.state, cid)
    }

    pub fn prove_axons_in(&self, cid: &Cid) -> Option<QueryProof> {
        prove_axons_in(&self.state, cid)
    }

    pub fn prove_location(&self, cid: &Cid) -> Option<QueryProof> {
        prove_location(&self.state, cid)
    }

    pub fn prove_coin(&self, denom: &Cid) -> Option<QueryProof> {
        prove_coin(&self.state, denom)
    }

    pub fn prove_card(&self, card_id: &Cid) -> Option<QueryProof> {
        prove_card(&self.state, card_id)
    }

    pub fn prove_file(&self, cid: &Cid) -> Option<QueryProof> {
        prove_file(&self.state, cid)
    }

    pub fn prove_signal(&self, step: u64) -> Option<QueryProof> {
        prove_signal(&self.state, step)
    }

    pub fn prove_time(&self, height: u64) -> Option<QueryProof> {
        prove_time(&self.state, height)
    }

    pub fn prove_commitment(&self, point: &[u8; 32]) -> Option<QueryProof> {
        prove_commitment(&self.state, point)
    }
}

impl Default for Bbg {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::NeuronRecord;

    fn neuron_id(seed: u8) -> NeuronId { [seed; 32] }
    fn cid(seed: u8) -> Cid { [seed; 32] }

    fn seed_neuron(bbg: &mut Bbg, id: NeuronId, focus: u64) {
        bbg.state.neurons.insert(id, NeuronRecord { focus, karma: 0, stake: 0 });
    }

    fn one_link(neuron: NeuronId, from: Cid, to: Cid) -> Signal {
        Signal {
            neuron,
            links: vec![Cyberlink { from, to, token: cid(0), amount: 1, valence: 1 }],
            utxo_moves: vec![],
            height: 0,
        }
    }

    #[test]
    fn empty_root_is_deterministic() {
        assert_eq!(BbgState::new().root, BbgState::new().root);
    }

    #[test]
    fn compute_root_is_deterministic() {
        let mut a = Bbg::new();
        let mut b = Bbg::new();
        seed_neuron(&mut a, neuron_id(1), 100);
        seed_neuron(&mut b, neuron_id(1), 100);
        a.insert(&one_link(neuron_id(1), cid(2), cid(3))).unwrap();
        b.insert(&one_link(neuron_id(1), cid(2), cid(3))).unwrap();
        assert_eq!(a.state.compute_root(), b.state.compute_root());
    }

    #[test]
    fn cyberlink_changes_root() {
        let mut bbg = Bbg::new();
        seed_neuron(&mut bbg, neuron_id(1), 100);
        let root_before = bbg.state.root;
        bbg.insert(&one_link(neuron_id(1), cid(2), cid(3))).unwrap();
        assert_ne!(bbg.state.root, root_before);
    }

    #[test]
    fn finalize_block_increments_height() {
        let mut bbg = Bbg::new();
        assert_eq!(bbg.state.height, 0);
        bbg.finalize_block();
        assert_eq!(bbg.state.height, 1);
        bbg.finalize_block();
        assert_eq!(bbg.state.height, 2);
    }

    #[test]
    fn double_spend_is_rejected() {
        let mut bbg = Bbg::new();
        let nullifier = cid(42);
        let mk_signal = || Signal {
            neuron: neuron_id(1),
            links: vec![],
            utxo_moves: vec![UtxoMove { nullifier, commitment: None }],
            height: 0,
        };
        bbg.insert(&mk_signal()).unwrap();
        assert_eq!(bbg.insert(&mk_signal()), Err(InsertError::DoubleSpend));
    }

    #[test]
    fn prove_and_verify_particle_roundtrip() {
        let mut bbg = Bbg::new();
        seed_neuron(&mut bbg, neuron_id(1), 100);
        bbg.insert(&one_link(neuron_id(1), cid(2), cid(3))).unwrap();

        let proof = bbg.prove_particle(&cid(3)).expect("particle proof must exist");
        assert!(verify_particle(&proof, &bbg.state.root, &cid(3)));
    }

    #[test]
    fn prove_particle_returns_none_for_unknown_cid() {
        assert!(Bbg::new().prove_particle(&cid(255)).is_none());
    }
}
