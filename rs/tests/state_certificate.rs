use bbg::certificate::{MAX_FIELDS, StateCertificate};
use bbg::{
    BbgState, Dim,
    types::{ParticleRecord, SignalRecord},
};

fn state() -> BbgState {
    let mut state = BbgState::new();
    let mut record = ParticleRecord::zero();
    record.energy = 77;
    state.particles.insert([1; 32], record);
    state.signals.insert(
        7,
        SignalRecord {
            neuron: [2; 32],
            network: [3; 32],
            link_count: 5,
            block_height: u64::MAX,
            proof_hash: [4; 32],
        },
    );
    state.refresh_root();
    state
}

#[test]
fn certificates_authenticate_actual_cells_and_the_same_root_as_query_proofs() {
    let state = state();
    let cert = StateCertificate::from_state(&state, &[9, 0]).unwrap();
    let root = cert.root().unwrap();
    assert_eq!(root.map(|v| v.to_le_bytes()).concat(), state.root());
    cert.verify(root).unwrap();
    assert_eq!(cert.cell(0, 11), Some(77));
    assert_eq!(cert.cell(1, 11), None);
    for table in &cert.dimensions {
        let dim = Dim::from_u64(table.namespace).unwrap();
        for index in [0, 11, table.fields.len() - 1] {
            let proof = bbg::proof::open_cell(&state, dim, index).unwrap();
            let expected = cert.leaves[table.namespace as usize]
                .map(|v| v.to_le_bytes())
                .concat();
            assert_eq!(proof.commitment.as_bytes(), expected);
            assert_eq!(
                u64::from_le_bytes(proof.value_bytes.try_into().unwrap()),
                table.fields[index]
            );
            assert_eq!(
                bbg::proof::cell_value(&state, dim, index).unwrap().as_u64(),
                table.fields[index]
            );
        }
        assert!(bbg::proof::open_cell(&state, dim, table.fields.len()).is_none());
        assert!(
            cert.cell(table.namespace, table.fields.len() as u64)
                .is_none()
        );
    }
}

#[test]
fn every_committed_component_and_structural_metadata_is_checked() {
    let state = state();
    let cert = StateCertificate::from_state(&state, &[0, 9]).unwrap();
    let root = cert.root().unwrap();
    for mutation in 0..10 {
        let mut bad = cert.clone();
        match mutation {
            0 => bad.dimensions[0].fields[11] += 1,
            1 => bad.leaves[0][0] = (bad.leaves[0][0] + 1) % nebu::field::P,
            2 => bad.leaves[13][0] = (bad.leaves[13][0] + 1) % nebu::field::P,
            3 => bad.dimensions.push(bad.dimensions[0].clone()),
            4 => bad.dimensions[0].namespace = 10,
            5 => bad.dimensions[0].fields[1] += 1,
            6 => bad.dimensions[0].fields[0] = 1,
            7 => bad.dimensions[0].fields[11] = nebu::field::P,
            8 => bad.lens_version = 1,
            _ => bad.dimensions[0].fields.resize(MAX_FIELDS + 1, 0),
        }
        assert!(bad.verify(root).is_err(), "mutation={mutation}");
    }
    assert!(cert.verify([nebu::field::P; 4]).is_err());
    assert!(StateCertificate::from_state(&state, &[0, 0]).is_err());
    assert!(StateCertificate::from_state(&state, &[10]).is_ok());
    assert!(StateCertificate::from_state(&state, &[11]).is_err());
}

#[test]
fn stale_cached_roots_and_changed_signal_networks_are_rejected() {
    let mut state = state();
    let old = StateCertificate::from_state(&state, &[9])
        .unwrap()
        .root()
        .unwrap();
    state.signals.get_mut(&7).unwrap().network[0] += 1;
    assert!(
        StateCertificate::from_state(&state, &[9])
            .unwrap_err()
            .contains("stale cached")
    );
    state.refresh_root();
    let cert = StateCertificate::from_state(&state, &[9]).unwrap();
    assert_ne!(cert.root().unwrap(), old);
    assert!(cert.verify(old).is_err());
}

#[test]
fn arbitrary_key_bytes_and_u64_values_have_no_modulus_alias() {
    let mut a = BbgState::new();
    let mut b = BbgState::new();
    let first = [0; 32];
    let mut second = first;
    second[..8].copy_from_slice(&nebu::field::P.to_le_bytes());
    a.particles.insert(first, ParticleRecord::zero());
    b.particles.insert(second, ParticleRecord::zero());
    assert_ne!(a.compute_root(), b.compute_root());
    b.particles.clear();
    let mut record = ParticleRecord::zero();
    record.energy = nebu::field::P;
    b.particles.insert(first, record);
    assert_ne!(a.compute_root(), b.compute_root());
    let cert = StateCertificate::from_state(&b, &[0]).unwrap();
    assert_eq!(cert.cell(0, 11), Some(nebu::field::P & 0xffff_ffff));
    assert_eq!(cert.cell(0, 12), Some(nebu::field::P >> 32));
}

#[cfg(feature = "serde")]
#[test]
fn certificate_serde_requires_version_metadata_and_roundtrips() {
    let cert = StateCertificate::from_state(&state(), &[0, 9]).unwrap();
    let json = serde_json::to_string(&cert).unwrap();
    let decoded: StateCertificate = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, cert);
    decoded.verify(cert.root().unwrap()).unwrap();
    let mut missing = serde_json::to_value(&cert).unwrap();
    missing.as_object_mut().unwrap().remove("lens_version");
    assert!(serde_json::from_value::<StateCertificate>(missing).is_err());
}
