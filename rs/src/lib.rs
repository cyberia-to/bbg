// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! bbg — Big Badass Graph: authenticated state with polynomial commitments.
//!
//! BBG has one operation: insert(signal).
//! All semantic validation (A1–A3, focus sufficiency, box ownership,
//! conservation, VDF) is the responsibility of cybergraph.
//! BBG only enforces the structural double-spend invariant via N(x).

pub mod checkpoint;
pub mod dim;
pub mod proof;
pub mod prune;
pub mod query;
pub mod signal;
pub mod state;
pub mod storage;
pub mod types;

pub use checkpoint::Checkpoint;
pub use proof::{
    prove_axons_in, prove_axons_out, prove_balances, prove_card, prove_coin, prove_commitment,
    prove_file, prove_location, prove_neuron, prove_particle, prove_signal, prove_time,
    verify_particle, QueryProof,
};
pub use prune::{PruneConfig, PruneState};
pub use query::{
    bbg_query, collect_look_openings, verify_opening, verify_query,
    BbgLookProvider, Dim, ProofLookProvider,
};
pub use signal::{BoxMove, Cyberlink, InsertError, Signal};
pub use state::BbgState;
pub use types::{IntentRecord, NeuronId, Particle, SignalRecord};

/// The BBG facade: state + checkpoint + pruning policy as a single unit.
pub struct Bbg {
    pub state: BbgState,
    pub checkpoint: Checkpoint,
    pub prune_config: PruneConfig,
    pub prune_state: PruneState,
}

impl Bbg {
    pub fn new() -> Self {
        let state = BbgState::new();
        let checkpoint = Checkpoint::new(&state);
        Self { state, checkpoint, prune_config: PruneConfig::default(), prune_state: PruneState::default() }
    }

    pub fn with_prune_config(mut self, config: PruneConfig) -> Self {
        self.prune_config = config;
        self
    }

    /// Insert a pre-validated signal. Fails only on structural double-spend.
    /// Updates pruning state (last_touched) on success.
    pub fn insert(&mut self, signal: &Signal) -> Result<(), InsertError> {
        self.state.insert(signal)?;
        let epoch = self.state.height / state::EPOCH_BLOCKS;
        for link in &signal.links {
            let aid = state::axon_id(&link.from, &link.to);
            self.prune_state.touch(aid, epoch);
        }
        Ok(())
    }

    /// Finalize the current block: record a time snapshot, increment height,
    /// and run pruning at epoch boundaries.
    pub fn finalize_block(&mut self) {
        let h = self.state.height;
        let root = self.state.root;
        self.state.time.insert(h, root);
        self.state.root = self.state.compute_root();
        self.state.height += 1;
        if self.state.height % state::EPOCH_BLOCKS == 0 {
            let epoch = self.state.height / state::EPOCH_BLOCKS;
            prune::prune(&mut self.state, &mut self.prune_state, &self.prune_config, epoch);
        }
        self.checkpoint = self.checkpoint.advance(&self.state);
    }

    pub fn prove_particle(&self, particle: &Particle) -> Option<QueryProof> {
        prove_particle(&self.state, particle)
    }

    pub fn prove_neuron(&self, id: &NeuronId) -> Option<QueryProof> {
        prove_neuron(&self.state, id)
    }

    pub fn prove_axons_out(&self, particle: &Particle) -> Option<QueryProof> {
        prove_axons_out(&self.state, particle)
    }

    pub fn prove_axons_in(&self, particle: &Particle) -> Option<QueryProof> {
        prove_axons_in(&self.state, particle)
    }

    pub fn prove_location(&self, particle: &Particle) -> Option<QueryProof> {
        prove_location(&self.state, particle)
    }

    pub fn prove_coin(&self, denom: &Particle) -> Option<QueryProof> {
        prove_coin(&self.state, denom)
    }

    pub fn prove_card(&self, card_id: &Particle) -> Option<QueryProof> {
        prove_card(&self.state, card_id)
    }

    pub fn prove_file(&self, particle: &Particle) -> Option<QueryProof> {
        prove_file(&self.state, particle)
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

    pub fn prove_balances(&self, owner: &[u8; 32], token: &[u8; 32]) -> Option<QueryProof> {
        prove_balances(&self.state, owner, token)
    }

    /// Persist an unsealed intent. Validation (identity signature) is sync's job.
    /// Returns the intent key = H(ν ‖ h0 ‖ scope_hash).
    pub fn apply_intent(&mut self, intent: &IntentRecord) -> Particle {
        self.state.apply_intent(intent)
    }

    /// Persist a signal header without applying its cyberlinks.
    /// Used when sealing follows a separate intent → seal lifecycle.
    pub fn apply_signal_record(&mut self, step: u64, record: SignalRecord) {
        self.state.apply_signal_record(step, record);
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
    use signal::BoxMove;
    use types::NeuronRecord;

    fn neuron_id(seed: u8) -> NeuronId { [seed; 32] }
    fn particle(seed: u8) -> Particle { [seed; 32] }

    fn seed_neuron(bbg: &mut Bbg, id: NeuronId, focus: u64) {
        bbg.state.neurons.insert(id, NeuronRecord { focus, karma: 0, stake: 0 });
    }

    fn one_link(neuron: NeuronId, from: Particle, to: Particle) -> Signal {
        Signal {
            neuron,
            links: vec![Cyberlink { from, to, token: particle(0), amount: 1, valence: 1 }],
            box_moves: vec![],
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
        a.insert(&one_link(neuron_id(1), particle(2), particle(3))).unwrap();
        b.insert(&one_link(neuron_id(1), particle(2), particle(3))).unwrap();
        assert_eq!(a.state.compute_root(), b.state.compute_root());
    }

    #[test]
    fn cyberlink_changes_root() {
        let mut bbg = Bbg::new();
        seed_neuron(&mut bbg, neuron_id(1), 100);
        let root_before = bbg.state.root;
        bbg.insert(&one_link(neuron_id(1), particle(2), particle(3))).unwrap();
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
        let nullifier = particle(42);
        let mk_signal = || Signal {
            neuron: neuron_id(1),
            links: vec![],
            box_moves: vec![BoxMove { nullifier, commitment: None }],
            height: 0,
        };
        bbg.insert(&mk_signal()).unwrap();
        assert_eq!(bbg.insert(&mk_signal()), Err(InsertError::DoubleSpend));
    }

    #[test]
    fn prove_and_verify_particle_roundtrip() {
        let mut bbg = Bbg::new();
        seed_neuron(&mut bbg, neuron_id(1), 100);
        bbg.insert(&one_link(neuron_id(1), particle(2), particle(3))).unwrap();

        let proof = bbg.prove_particle(&particle(3)).expect("particle proof must exist");
        assert!(verify_particle(&proof, &bbg.state.root, &particle(3)));
    }

    #[test]
    fn prove_particle_returns_none_for_unknown_cid() {
        assert!(Bbg::new().prove_particle(&particle(255)).is_none());
    }
}
