// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! End-to-end look proof against real BBG state.
//!
//! A nox program declares the BBG root in its object, reads a committed cell
//! via pattern 17, and the zheng proof binds the opened value, the cell index,
//! the dimension commitment, and the recomputed root — the full chain from
//! `state.root()` to a verified `TraceProof`. This is the property the look
//! argument exists to enforce: a prover cannot read state the root does not
//! commit to.

use bbg::query::ProofLookProvider;
use bbg::types::ParticleRecord;
use bbg::BbgState;
use nebu::Goldilocks;
use nox::{reduce, Outcome, Order, Reduction, VecTrace};
use zheng::{commit, verify, ProofParams, Statement};

fn g(v: u64) -> Goldilocks {
    Goldilocks::new(v)
}

/// A state with two particles — enough for a non-trivial particles dimension.
fn sample_state() -> BbgState {
    let mut state = BbgState::new();
    state.particles.insert(
        [1u8; 32],
        ParticleRecord { energy: 77, pi_star: 0, weight: 0, s_yes: 0, s_no: 0, meta_score: 0 },
    );
    state.particles.insert(
        [2u8; 32],
        ParticleRecord { energy: 88, pi_star: 0, weight: 0, s_yes: 0, s_no: 0, meta_score: 0 },
    );
    state
}

/// Build the look object `[[l0 | [l1 | [l2 | l3]]] | 0]` carrying the root limbs.
fn make_obj<const N: usize>(ar: &mut Reduction<N>, root: &[u8; 32]) -> Order {
    let limbs = bbg::dim::goldilocks_from_bytes32(root);
    let l: Vec<Order> = limbs.iter().map(|&x| ar.atom(x).unwrap()).collect();
    let inner = ar.pair(l[2], l[3]).unwrap();
    let mid = ar.pair(l[1], inner).unwrap();
    let root_pair = ar.pair(l[0], mid).unwrap();
    let rest = ar.atom(g(0)).unwrap();
    ar.pair(root_pair, rest).unwrap()
}

/// Build the look formula `[17 [[1 ns] [1 key]]]`.
fn make_look<const N: usize>(ar: &mut Reduction<N>, ns: u64, key: u64) -> Order {
    let t17 = ar.atom(g(17)).unwrap();
    let t1 = ar.atom(g(1)).unwrap();
    let vns = ar.atom(g(ns)).unwrap();
    let vkey = ar.atom(g(key)).unwrap();
    let nf = ar.pair(t1, vns).unwrap();
    let kf = ar.pair(t1, vkey).unwrap();
    let body = ar.pair(nf, kf).unwrap();
    ar.pair(t17, body).unwrap()
}

fn open_statement() -> Statement {
    Statement {
        program_hash: [0u8; 32],
        input_hash: [0u8; 32],
        output_hash: [0u8; 32],
        focus_bound: 0,
    }
}

#[test]
fn look_proof_verifies_against_state_root() {
    let state = sample_state();
    let root = state.root();

    // Particles dimension layout: [key(4) | energy, pi_star, weight, s_yes,
    // s_no, meta_score] per entry — cell 4 is the first entry's energy.
    let mut ar = Reduction::<4096>::new();
    let obj = make_obj(&mut ar, &root);
    let formula = make_look(&mut ar, 0, 4);

    let provider = ProofLookProvider::new(&state);
    let mut trace = VecTrace::default();
    let value = match reduce(&mut ar, obj, formula, 1000, &provider, &mut trace) {
        Outcome::Ok(res, _) => ar.atom_value(res).expect("atom result"),
        other => panic!("nox look failed: {other:?}"),
    };
    assert_eq!(value, g(77), "the look read the committed energy");

    let openings = provider.take_look_openings();
    assert_eq!(openings.len(), 1);

    let statement = open_statement();
    let proof = commit(&trace, &[], &[], &openings, &statement, &ProofParams::default())
        .expect("zheng commit with a real look opening");
    assert!(
        verify(&proof, &statement, &ProofParams::default()).is_ok(),
        "the look proof verifies against the state root"
    );
}

#[test]
fn look_against_stale_root_is_rejected() {
    let state = sample_state();
    let stale_root = state.root();

    // The state advances: the root the program declares is now stale.
    let mut state = state;
    state.particles.insert(
        [3u8; 32],
        ParticleRecord { energy: 99, pi_star: 0, weight: 0, s_yes: 0, s_no: 0, meta_score: 0 },
    );
    state.refresh_root();

    let mut ar = Reduction::<4096>::new();
    let obj = make_obj(&mut ar, &stale_root);
    let formula = make_look(&mut ar, 0, 4);

    let provider = ProofLookProvider::new(&state);
    let mut trace = VecTrace::default();
    let _ = reduce(&mut ar, obj, formula, 1000, &provider, &mut trace);
    let openings = provider.take_look_openings();
    assert_eq!(openings.len(), 1);

    // The openings carry the CURRENT leaves; the trace carries the STALE root.
    // The root-binding steps disagree — commit must fail, not produce a proof.
    let statement = open_statement();
    let result = commit(&trace, &[], &[], &openings, &statement, &ProofParams::default());
    assert!(
        result.is_err(),
        "a look against a root the leaves do not hash to must not prove"
    );
}
