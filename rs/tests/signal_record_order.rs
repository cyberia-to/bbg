//! Property #9 (launch.md) — nodes converge from different starting states /
//! arrival orders. `BbgState::apply_signal_record` commits a signal's header
//! to `signals: BTreeMap<u64, SignalRecord>`, keyed by `step` alone
//! (`src/state.rs`). `step` is a per-neuron chain counter (cybergraph's
//! `SignalChain`, one per `NeuronId`), not a global one, so every neuron's
//! first signal lands on `step = 0`. Two different neurons' first signals
//! collide on that key: whichever is applied last silently overwrites the
//! earlier one's record.
//!
//! This test shows two nodes that receive the same two signals in opposite
//! arrival order converge on cyberlink state (`particles`, keyed by content
//! hash — order-independent) but diverge on the `signals` dimension and
//! therefore on the committed root: the opposite of what property #9 requires.

use bbg::types::NeuronRecord;
use bbg::{BbgState, Cyberlink, NeuronId, Particle, Signal};

fn neuron(seed: u8) -> NeuronId {
    [seed; 32]
}
fn particle(seed: u8) -> Particle {
    [seed; 32]
}

/// Each neuron's own first signal — one cyberlink, no box moves.
fn first_signal(n: NeuronId, from: u8, to: u8) -> Signal {
    Signal {
        neuron: n,
        links: vec![Cyberlink {
            from: particle(from),
            to: particle(to),
            token: particle(0),
            amount: 1,
            valence: 1,
        }],
        box_moves: vec![],
        height: 0,
    }
}

fn record(neuron: NeuronId) -> bbg::SignalRecord {
    bbg::SignalRecord {
        neuron,
        network: [0u8; 32],
        link_count: 1,
        block_height: 0,
        proof_hash: [0u8; 32],
    }
}

/// Two neurons' first signals, applied in opposite arrival order on two
/// nodes: the cyberlink graph converges, the committed root does not.
#[test]
fn same_step_signals_from_different_neurons_collide_in_the_signals_dimension() {
    let n1 = neuron(1);
    let n2 = neuron(2);

    let mut node_ab = BbgState::new();
    node_ab.neurons.insert(n1, NeuronRecord { focus: 100_000, karma: 0, stake: 0 });
    node_ab.neurons.insert(n2, NeuronRecord { focus: 100_000, karma: 0, stake: 0 });
    node_ab.insert(&first_signal(n1, 10, 11)).unwrap();
    node_ab.apply_signal_record(0, record(n1));
    node_ab.insert(&first_signal(n2, 20, 21)).unwrap();
    node_ab.apply_signal_record(0, record(n2));

    let mut node_ba = BbgState::new();
    node_ba.neurons.insert(n1, NeuronRecord { focus: 100_000, karma: 0, stake: 0 });
    node_ba.neurons.insert(n2, NeuronRecord { focus: 100_000, karma: 0, stake: 0 });
    node_ba.insert(&first_signal(n2, 20, 21)).unwrap();
    node_ba.apply_signal_record(0, record(n2));
    node_ba.insert(&first_signal(n1, 10, 11)).unwrap();
    node_ba.apply_signal_record(0, record(n1));

    // Both cyberlinks land in both nodes regardless of order: the graph
    // itself converges, as property #9 requires.
    let axon_1 = bbg::state::axon_id(&particle(10), &particle(11));
    let axon_2 = bbg::state::axon_id(&particle(20), &particle(21));
    assert!(node_ab.particles.contains_key(&axon_1));
    assert!(node_ab.particles.contains_key(&axon_2));
    assert!(node_ba.particles.contains_key(&axon_1));
    assert!(node_ba.particles.contains_key(&axon_2));

    // But `signals[0]` holds whichever neuron's record was inserted last, so
    // the two nodes disagree on it —
    let rec_ab = node_ab.signals.get(&0).expect("record at step 0");
    let rec_ba = node_ba.signals.get(&0).expect("record at step 0");
    assert_ne!(
        rec_ab.neuron, rec_ba.neuron,
        "arrival order must not change the recorded signal header, but it does: \
         BbgState::signals is keyed by the per-neuron step counter alone, so two \
         neurons' first signals (both step 0) overwrite each other by arrival order"
    );

    // — and so the committed root, which folds `signals` in, diverges too.
    assert_ne!(
        node_ab.root(),
        node_ba.root(),
        "property #9 requires nodes to converge from different arrival orders; \
         they do not, because of the signals-dimension key collision above"
    );
}
