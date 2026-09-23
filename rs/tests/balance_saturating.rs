// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! `BbgState::insert`'s five per-link accumulators (particle weight, particle
//! energy, neuron focus, balance credit, balance debit) must all saturate the
//! same way — cybergraph validates semantics before calling insert, but the
//! structural check here is the last line of defense against a corrupted or
//! adversarial u64 amount overflowing an authenticated public balance.

use bbg::signal::{Cyberlink, Signal};
use bbg::state::balance_key;
use bbg::BbgState;

fn link(to: [u8; 32], token: [u8; 32], amount: u64) -> Cyberlink {
    Cyberlink { from: [9u8; 32], to, token, amount, valence: 0 }
}

fn push(state: &mut BbgState, links: Vec<Cyberlink>) {
    state
        .insert(&Signal { neuron: [7u8; 32], links, box_moves: vec![], height: 0 })
        .unwrap();
}

#[test]
fn balance_credit_saturates_instead_of_overflowing() {
    let to = [1u8; 32];
    let token = [2u8; 32];
    let mut state = BbgState::new();

    // first link credits u64::MAX, second credits 1 more — a real overflow
    // if the accumulator is a bare `+=` instead of saturating.
    push(&mut state, vec![link(to, token, u64::MAX)]);
    push(&mut state, vec![link(to, token, 1)]);

    let key = balance_key(&to, &token);
    assert_eq!(state.balances.get(&key), Some(&u64::MAX));
}

#[test]
fn balance_credit_saturates_within_one_signal() {
    // two links in the same signal crediting the same (to, token) pair,
    // summing past u64::MAX.
    let to = [3u8; 32];
    let token = [4u8; 32];
    let mut state = BbgState::new();

    push(&mut state, vec![link(to, token, u64::MAX), link(to, token, 5)]);

    let key = balance_key(&to, &token);
    assert_eq!(state.balances.get(&key), Some(&u64::MAX));
}
