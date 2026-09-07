// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! Serde round-trip and wire-format stability for `QueryProof`.
//!
//! Two properties:
//! 1. round-trip — a real Brakedown-backed proof survives value → JSON →
//!    value, and the recovered proof still verifies;
//! 2. stability — the JSON shape is pinned by a committed golden fixture,
//!    so a wire-format change breaks a test instead of drifting silently.
//!
//! Regenerate the fixture (after an INTENTIONAL format change only) with
//! `BBG_BLESS=1 cargo test --features serde --test serde_wire`.
#![cfg(feature = "serde")]

use bbg::proof::{open_cell, verify_particle};
use bbg::query::Dim;
use bbg::types::ParticleRecord;
use bbg::{BbgState, QueryProof};
use lens::{Commitment, Opening};
use nebu::Goldilocks;

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

#[test]
fn queryproof_roundtrip_real_proof() {
    let state = sample_state();
    // cell 4 = first value field (energy) of the first particle entry.
    let proof = open_cell(&state, Dim::Particles, 4).unwrap();

    let json = serde_json::to_string(&proof).unwrap();
    let back: QueryProof = serde_json::from_str(&json).unwrap();

    assert_eq!(proof, back);
    // the deserialized proof still verifies against the commitment
    assert!(verify_particle(&back, &[0u8; 32], &[0u8; 32]));
}

#[test]
fn queryproof_rejects_noncanonical_point() {
    let state = sample_state();
    let proof = open_cell(&state, Dim::Particles, 4).unwrap();
    let mut v = serde_json::to_value(&proof).unwrap();
    // p = 2^64 - 2^32 + 1; inject a non-canonical element
    v["point"][0] = serde_json::json!(0xFFFF_FFFF_0000_0001u64);
    let err = serde_json::from_value::<QueryProof>(v).unwrap_err();
    assert!(err.to_string().contains("non-canonical"));
}

/// A synthetic QueryProof with fixed contents — pins the serde shape of every
/// field and every Opening variant without depending on Brakedown internals.
fn synthetic_proof(opening: Opening) -> QueryProof {
    QueryProof {
        commitment: Commitment(hemera::Hash::from([7u8; 32])),
        opening,
        value_bytes: vec![77, 0, 0, 0, 0, 0, 0, 0],
        point: vec![Goldilocks::ZERO, Goldilocks::ONE, Goldilocks::new(5)],
    }
}

#[test]
fn golden_fixture_is_stable() {
    let tensor = Opening::Tensor {
        round_commitments: vec![Commitment(hemera::Hash::from([1u8; 32]))],
        final_poly: vec![2, 3],
        query_responses: vec![(0, vec![4]), (5, vec![6, 7])],
    };
    let folding = Opening::Folding {
        round_commitments: vec![Commitment(hemera::Hash::from([8u8; 32]))],
        merkle_paths: vec![vec![hemera::Hash::from([9u8; 32])]],
        final_value: vec![10],
    };
    let witness = Opening::Witness {
        witness_commitment: Commitment(hemera::Hash::from([11u8; 32])),
        witness_opening: Box::new(tensor.clone()),
        certificate: vec![12],
    };
    let value = serde_json::json!({
        "tensor": synthetic_proof(tensor),
        "folding": synthetic_proof(folding),
        "witness": synthetic_proof(witness),
    });

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/query_proof_golden.json");
    if std::env::var_os("BBG_BLESS").is_some() {
        std::fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    }
    let golden: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(value, golden, "QueryProof wire format drifted from the committed fixture");

    // the fixture still deserializes into live types
    for variant in ["tensor", "folding", "witness"] {
        let _: QueryProof = serde_json::from_value(golden[variant].clone()).unwrap();
    }
}
