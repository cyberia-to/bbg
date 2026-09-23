// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! Serde round-trip and wire-format stability for `QueryProof`.
//!
//! Current authenticated context round-trips, legacy unsupported opening variants
//! fail closed, and untrusted vector sizes/canonical field encodings are bounded.
#![cfg(feature = "serde")]

use bbg::proof::{open_cell, verify_particle};
use bbg::query::Dim;
use bbg::types::ParticleRecord;
use bbg::{BbgState, QueryProof};

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
    state.refresh_root();
    state
}

#[test]
fn queryproof_roundtrip_real_proof() {
    let state = sample_state();
    // cell 11 = first value field (energy) of the first particle entry.
    let proof = open_cell(&state, Dim::Particles, 11).unwrap();

    let json = serde_json::to_string(&proof).unwrap();
    let back: QueryProof = serde_json::from_str(&json).unwrap();

    assert_eq!(proof, back);
    assert_eq!(back.context.as_ref().unwrap().version, 3);
    // the deserialized proof still binds the caller-selected root and particle
    assert!(verify_particle(&back, &state.root(), &[1u8; 32]));
}

#[test]
fn queryproof_rejects_noncanonical_point() {
    let state = sample_state();
    let proof = open_cell(&state, Dim::Particles, 11).unwrap();
    let mut v = serde_json::to_value(&proof).unwrap();
    // p = 2^64 - 2^32 + 1; inject a non-canonical element
    v["point"][0] = serde_json::json!(0xFFFF_FFFF_0000_0001u64);
    let err = serde_json::from_value::<QueryProof>(v).unwrap_err();
    assert!(err.to_string().contains("non-canonical"));
}

/// Old sampled-opening variants cannot silently acquire a root claim through
/// the new decoder. Preserve the old wire fixture as an explicit rejection test.
#[test]
fn legacy_opening_variants_are_rejected() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/query_proof_golden.json")).unwrap();
    for variant in ["tensor", "folding", "witness"] {
        assert!(serde_json::from_value::<QueryProof>(value[variant].clone()).is_err());
    }
}

#[test]
fn query_wire_bounds_and_contextless_refusal() {
    let state = sample_state();
    let proof = open_cell(&state, Dim::Particles, 11).unwrap();
    let original = serde_json::to_value(&proof).unwrap();
    for (field, value) in [
        ("point", serde_json::json!(vec![0; 21])),
        ("value_bytes", serde_json::json!(vec![0; 9])),
        ("value_bytes", serde_json::json!(vec![0; 7])),
    ] {
        let mut bad = original.clone();
        bad[field] = value;
        assert!(serde_json::from_value::<QueryProof>(bad).is_err());
    }
    let mut noncanonical = original.clone();
    noncanonical["value_bytes"] = serde_json::json!(nebu::field::P.to_le_bytes());
    assert!(serde_json::from_value::<QueryProof>(noncanonical).is_err());
    let mut bad = original.clone();
    bad["opening"]["TensorMerkle"]["columns"][0]["column"] = serde_json::json!(vec![0; 8193]);
    assert!(serde_json::from_value::<QueryProof>(bad).is_err());
    let mut legacy = original;
    legacy.as_object_mut().unwrap().remove("context");
    let decoded: QueryProof = serde_json::from_value(legacy).unwrap();
    assert!(!verify_particle(&decoded, &state.root(), &[1; 32]));
}
