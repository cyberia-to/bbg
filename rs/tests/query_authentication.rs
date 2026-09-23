use bbg::proof::{open_cell, prove_balances, prove_commitment, prove_particle};
use bbg::types::*;
use bbg::{
    BbgState, Dim, QueryProof, bbg_query, verify_entity, verify_particle, verify_query,
    verify_query_at,
};
use nebu::Goldilocks as F;

fn state() -> BbgState {
    let mut s = BbgState::new();
    for key in [[0; 32], alias_key()] {
        let mut r = ParticleRecord::zero();
        r.energy = 77;
        s.particles.insert(key, r);
    }
    s.refresh_root();
    s
}
fn alias_key() -> [u8; 32] {
    let mut key = [0; 32];
    key[..8].copy_from_slice(&nebu::field::P.to_le_bytes());
    key
}
fn rejects(proof: &QueryProof, root: &[u8; 32]) {
    assert!(!verify_particle(proof, root, &[0; 32]));
}

#[test]
fn exact_particle_root_and_injective_key_are_pinned() {
    let s = state();
    let root = s.root();
    let proof = prove_particle(&s, &[0; 32]).unwrap();
    assert!(verify_particle(&proof, &root, &[0; 32]));
    assert!(verify_query(&proof));
    assert!(verify_query_at(&proof, &root, 0, 11));
    assert!(!verify_particle(&proof, &root, &alias_key()));
    let alias = prove_particle(&s, &alias_key()).unwrap();
    assert_eq!(alias.value_bytes, proof.value_bytes);
    assert!(verify_particle(&alias, &root, &alias_key()));
    assert!(!verify_particle(&alias, &root, &[0; 32]));
    let mut other_root = root;
    other_root[0] ^= 1;
    rejects(&proof, &other_root);
    assert!(!verify_query_at(&proof, &root, 1, 11));
    assert!(!verify_query_at(&proof, &root, 0, 12));
    let key_cell = open_cell(&s, Dim::Particles, 3).unwrap();
    assert!(verify_query_at(&key_cell, &root, 0, 3));
    rejects(&key_cell, &root);
}

#[test]
fn query_claim_context_opening_and_certificate_mutations_fail() {
    let s = state();
    let root = s.root();
    let proof = prove_particle(&s, &[0; 32]).unwrap();
    for mutation in 0..12 {
        let mut bad = proof.clone();
        match mutation {
            0 => bad.value_bytes[0] ^= 1,
            1 => bad.value_bytes.push(0),
            2 => bad.point[0] = F::new(2),
            3 => bad.point.clear(),
            4 => bad.context = None,
            5 => bad.context.as_mut().unwrap().namespace = 1,
            6 => bad.context.as_mut().unwrap().index = 12,
            7 => bad.context.as_mut().unwrap().version += 1,
            8 => bad.context.as_mut().unwrap().certificate.dimensions[0].fields[11] += 1,
            9 => bad.context.as_mut().unwrap().certificate.leaves[0][0] += 1,
            10 => {
                if let lens::Opening::TensorMerkle {
                    row_combination, ..
                } = &mut bad.opening
                {
                    row_combination[0] ^= 1
                } else {
                    panic!("current opening")
                }
            }
            _ => {
                if let lens::Opening::TensorMerkle { columns, .. } = &mut bad.opening {
                    columns.clear()
                } else {
                    panic!("current opening")
                }
            }
        }
        rejects(&bad, &root);
        assert!(!verify_query(&bad), "mutation {mutation}");
    }
    let mut stale = state();
    stale.particles.get_mut(&[0; 32]).unwrap().energy += 1;
    assert!(prove_particle(&stale, &[0; 32]).is_none());
}

#[test]
fn every_public_dimension_entity_locator_uses_its_real_record_layout() {
    let mut s = state();
    let key = [3; 32];
    s.axons_out.insert(key, vec![[4; 32], [5; 32]]);
    s.axons_in.insert(key, vec![[6; 32]]);
    s.neurons.insert(
        key,
        NeuronRecord {
            focus: 77,
            karma: u64::MAX,
            stake: 1,
        },
    );
    s.locations.insert(key, LocationRecord { lat: -1, lon: 2 });
    s.coins.insert(
        key,
        CoinRecord {
            total_supply: u64::MAX,
        },
    );
    s.cards.insert(
        key,
        CardRecord {
            owner: [7; 32],
            particle: [8; 32],
        },
    );
    s.files.insert(
        key,
        FileRecord {
            available: true,
            chunk_count: 99,
        },
    );
    for n in [1, 256] {
        s.time.insert(n, [9; 32]);
        s.signals.insert(
            n,
            SignalRecord {
                neuron: [10; 32],
                network: [11; 32],
                link_count: 55,
                block_height: u64::MAX,
                proof_hash: [12; 32],
            },
        );
    }
    s.refresh_root();
    for ns in 1..=9 {
        let dim = Dim::from_u64(ns).unwrap();
        let mut entity = key;
        if ns >= 8 {
            entity = [0; 32];
            entity[..8].copy_from_slice(&256u64.to_le_bytes());
        }
        let proof = bbg_query(&s, dim, &entity).unwrap();
        assert!(
            verify_entity(&proof, &s.root(), dim, &entity),
            "namespace {ns}"
        );
        let mut wrong = entity;
        wrong[31] ^= 1;
        assert!(!verify_entity(&proof, &s.root(), dim, &wrong));
    }
    let mut invalid = [0; 32];
    invalid[31] = 1;
    assert!(bbg_query(&s, Dim::Time, &invalid).is_none());
    assert!(bbg_query(&s, Dim::Signals, &invalid).is_none());
}

#[test]
fn private_tables_are_not_disclosed_and_cannot_claim_public_context() {
    let mut s = state();
    let owner = [7; 32];
    let token = [8; 32];
    s.balances
        .insert(bbg::state::balance_key(&owner, &token), 99);
    s.commitments.insert([9; 32], F::new(42));
    s.refresh_root();
    for proof in [
        prove_balances(&s, &owner, &token).unwrap(),
        prove_commitment(&s, &[9; 32]).unwrap(),
    ] {
        assert!(proof.context.is_none());
        assert!(!verify_query(&proof));
        assert!(!verify_query_at(&proof, &s.root(), 10, 11));
        rejects(&proof, &s.root());
    }
}

#[test]
fn public_query_limit_is_explicit_and_large_certificates_remain_available() {
    let mut s = BbgState::new();
    for i in 0u64..205 {
        let mut key = [0; 32];
        key[..8].copy_from_slice(&i.to_le_bytes());
        s.particles.insert(key, ParticleRecord::zero());
    }
    s.refresh_root();
    assert!(open_cell(&s, Dim::Particles, 11).is_none());
    let certificate = bbg::certificate::StateCertificate::from_state(&s, &[0]).unwrap();
    assert_eq!(certificate.cell(0, 11), Some(0));
    certificate.root().unwrap();
}

#[test]
fn legacy_look_needs_matching_complete_context_and_rejects_each_substitution() {
    use bbg::{ProofLookProvider, verify_opening, verify_opening_with_context};
    use nox::LookProvider;
    let s = state();
    let root = s.root();
    let provider = ProofLookProvider::new(&s);
    assert_eq!(
        provider.look(F::ZERO, F::ZERO, F::new(11)),
        Some(F::new(77))
    );
    let lo = provider.take_look_openings().remove(0);
    let proof = open_cell(&s, Dim::Particles, 11).unwrap();
    assert!(!verify_opening(&lo));
    assert!(verify_opening_with_context(&lo, &proof, &root, 11));
    assert!(!verify_opening_with_context(&lo, &proof, &root, 12));
    for change in 0..6 {
        let mut bad = zheng::LookOpening {
            commitment: lo.commitment,
            point: lo.point.clone(),
            value: lo.value,
            opening: lo.opening.clone(),
            transcript_seed: lo.transcript_seed.clone(),
            leaves: lo.leaves.clone(),
            namespace: lo.namespace,
        };
        match change {
            0 => bad.namespace = F::ONE,
            1 => bad.value += F::ONE,
            2 => bad.point[0] = F::new(2),
            3 => bad.leaves.dims[0][0] += F::ONE,
            4 => bad.transcript_seed.push(0),
            _ => {
                if let lens::Opening::TensorMerkle { columns, .. } = &mut bad.opening {
                    columns.clear()
                } else {
                    panic!("current opening")
                }
            }
        }
        assert!(
            !verify_opening_with_context(&bad, &proof, &root, 11),
            "look mutation {change}"
        );
    }
}
