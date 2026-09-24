// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! Regression for the signals-dimension key collision
//! (`audit/signal-record-order-collision.md`, property 9: nodes converge
//! from different starting states).
//!
//! `signals` used to be keyed by the bare per-neuron `step`, and every
//! neuron's first signal is step 0 — two neurons' first signals collided in
//! the map, and whichever arrived last silently overwrote the other. The
//! key is now `signal_key(neuron, step)` (state.rs), so both survive
//! regardless of arrival order.

use bbg::proof::prove_signal;
use bbg::{BbgState, SignalRecord};

fn record(neuron: [u8; 32], link_count: u32) -> SignalRecord {
    SignalRecord {
        neuron,
        step: 0, // overwritten by apply_signal_record's `step` argument
        network: [0u8; 32],
        link_count,
        block_height: 1,
        proof_hash: [0u8; 32],
    }
}

#[test]
fn two_neurons_first_signal_both_survive_regardless_of_arrival_order() {
    let neuron_a = [1u8; 32];
    let neuron_b = [2u8; 32];

    let mut node_1 = BbgState::new();
    node_1.apply_signal_record(0, record(neuron_a, 11));
    node_1.apply_signal_record(0, record(neuron_b, 22));

    let mut node_2 = BbgState::new();
    node_2.apply_signal_record(0, record(neuron_b, 22));
    node_2.apply_signal_record(0, record(neuron_a, 11));

    // Both step-0 signals survive on both nodes — no silent overwrite.
    assert_eq!(node_1.signals.len(), 2);
    assert_eq!(node_2.signals.len(), 2);

    // Convergence is order-independent: same root regardless of arrival order.
    assert_eq!(node_1.root(), node_2.root());

    // Each neuron's own record is independently provable at step 0.
    let proof_a = prove_signal(&node_1, &neuron_a, 0).expect("neuron_a's step-0 signal");
    let proof_b = prove_signal(&node_1, &neuron_b, 0).expect("neuron_b's step-0 signal");
    assert_ne!(proof_a.value_bytes, proof_b.value_bytes, "distinct records must open distinct values");
}

#[test]
fn signal_key_binds_neuron_and_step() {
    use bbg::state::signal_key;

    let a = signal_key(&[1u8; 32], 0);
    let b = signal_key(&[2u8; 32], 0);
    let c = signal_key(&[1u8; 32], 1);
    assert_ne!(a, b, "different neurons at the same step must not collide");
    assert_ne!(a, c, "the same neuron at different steps must not collide");
    assert_eq!(a, signal_key(&[1u8; 32], 0), "the key is a pure function of its inputs");
}
