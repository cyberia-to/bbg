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

use bbg::BbgState;
use bbg::query::ProofLookProvider;
use bbg::types::ParticleRecord;
use nebu::Goldilocks;
use nox::{Order, Outcome, Reduction, VecTrace, reduce};
use zheng::{ProofParams, Statement, commit};

fn g(v: u64) -> Goldilocks {
    Goldilocks::new(v)
}

/// A state with two particles — enough for a non-trivial particles dimension.
fn sample_state() -> BbgState {
    let mut state = BbgState::new();
    state.particles.insert(
        [1u8; 32],
        ParticleRecord {
            energy: 77,
            pi_star: 0,
            weight: 0,
            s_yes: 0,
            s_no: 0,
            meta_score: 0,
        },
    );
    state.particles.insert(
        [2u8; 32],
        ParticleRecord {
            energy: 88,
            pi_star: 0,
            weight: 0,
            s_yes: 0,
            s_no: 0,
            meta_score: 0,
        },
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

/// Statement carrying the PUBLIC state root — since zheng's look-root
/// milestone the root is a public input; the zero root is the
/// no-state-read sentinel and look rows against it are rejected.
fn open_statement(bbg_root: [u8; 32]) -> Statement {
    Statement {
        program_hash: [0u8; 32],
        input_hash: [0u8; 32],
        output_hash: [0u8; 32],
        focus_bound: 0,
        bbg_root,
    }
}

#[test]
fn legacy_recursive_opening_fails_closed() {
    let state = sample_state();
    let root = state.root();

    // Particles dimension layout: [header(3)|key(8) | energy_lo,energy_hi, pi_star, weight, s_yes,
    // s_no, meta_score] per entry — cell 11 is the first entry's energy.
    let mut ar = Reduction::<4096>::new();
    let obj = make_obj(&mut ar, &root);
    let formula = make_look(&mut ar, 0, 11);

    let provider = ProofLookProvider::new(&state);
    let mut trace = VecTrace::default();
    let value = match reduce(&mut ar, obj, formula, 1000, &provider, &mut trace) {
        Outcome::Ok(res, _) => ar.atom_value(res).expect("atom result"),
        other => panic!("nox look failed: {other:?}"),
    };
    assert_eq!(value, g(77), "the look read the committed energy");

    let openings = provider.take_look_openings();
    assert_eq!(openings.len(), 1);

    let statement = open_statement(root);
    let error = commit(
        &trace,
        &[],
        &[],
        &openings,
        &statement,
        &ProofParams::default(),
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("UnsupportedRecursiveOpening"));
}

#[test]
fn look_against_stale_root_is_rejected() {
    let state = sample_state();
    let stale_root = state.root();

    // The state advances: the root the program declares is now stale.
    let mut state = state;
    state.particles.insert(
        [3u8; 32],
        ParticleRecord {
            energy: 99,
            pi_star: 0,
            weight: 0,
            s_yes: 0,
            s_no: 0,
            meta_score: 0,
        },
    );
    state.refresh_root();

    let mut ar = Reduction::<4096>::new();
    let obj = make_obj(&mut ar, &stale_root);
    let formula = make_look(&mut ar, 0, 11);

    let provider = ProofLookProvider::new(&state);
    let mut trace = VecTrace::default();
    let _ = reduce(&mut ar, obj, formula, 1000, &provider, &mut trace);
    let openings = provider.take_look_openings();
    assert_eq!(openings.len(), 1);

    // The openings carry the CURRENT leaves; the trace carries the STALE root.
    // The root-binding steps disagree — commit must fail, not produce a proof.
    let statement = open_statement(state.root());
    let result = commit(
        &trace,
        &[],
        &[],
        &openings,
        &statement,
        &ProofParams::default(),
    );
    assert!(
        result.is_err(),
        "a look against a root the leaves do not hash to must not prove"
    );
}

#[test]
fn certified_state_reads_bind_actual_execution_and_reject_missing_proof_data() {
    use bbg::certificate::StateCertificate;
    use zheng::execution::{ExecutionNoun as N, state::prove_state_execution};
    let pair = |a, b| N::Pair(Box::new(a), Box::new(b));
    let program = pair(
        N::Atom(17),
        pair(pair(N::Atom(1), N::Atom(0)), pair(N::Atom(1), N::Atom(11))),
    );
    let state = sample_state();
    let cert = StateCertificate::from_state(&state, &[0]).unwrap();
    let root = cert.root().unwrap();
    cert.verify(root).unwrap();
    let (statement, proof) =
        prove_state_execution(&program, &[], 1000, root, true, [0; 32], &mut |ns, key| {
            cert.cell(ns, key)
        })
        .unwrap();
    assert_eq!(statement.execution.public_output, vec![77]);
    statement
        .verify(&proof, &mut |ns, key| cert.cell(ns, key))
        .unwrap();
    for change in 0..5 {
        let mut altered = statement.clone();
        match change {
            0 => altered.execution.public_output[0] += 1,
            1 => altered.reads[0].key += 1,
            2 => altered.reads[0].value += 1,
            3 => altered.state_root[0] = (altered.state_root[0] + 1) % nebu::field::P,
            _ => altered.reads.clear(),
        }
        assert!(
            altered
                .verify(&proof, &mut |ns, key| cert.cell(ns, key))
                .is_err(),
            "change={change}"
        );
    }
    let mut missing = proof.clone();
    let lens::Opening::TensorMerkle { columns, .. } = &mut missing.spartan.pcs_opening else {
        panic!("expected authenticated public table")
    };
    columns.clear();
    assert!(
        statement
            .verify(&missing, &mut |ns, key| cert.cell(ns, key))
            .is_err()
    );
    let mut altered = cert.clone();
    altered.dimensions[0].fields[11] += 1;
    assert!(altered.verify(root).is_err());
}
