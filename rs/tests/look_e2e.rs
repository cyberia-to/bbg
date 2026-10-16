// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! End-to-end state read against real BBG state, zheng state profile v3.
//!
//! A nox program reads a committed cell via pattern 17. zheng authenticates
//! the certificate's evidence (complete tables, root leaves) under the
//! statement's own root and checks every CCS row with the read coordinates
//! pinned to the certified value. A prover cannot read state the root does
//! not commit to, and no caller-side check is needed for that.

use bbg::BbgState;
use bbg::certificate::StateCertificate;
use bbg::types::ParticleRecord;
use zheng::execution::state::{StateStatement, certify_state_execution};
use zheng::execution::{Certificate, ExecutionNoun as N};

/// A state with two particles — enough for a non-trivial particles dimension.
///
/// Root version 1: the look argument opens a Brakedown cell of the dimension
/// polynomial and binds that commitment to `leaves.dims[ns]`; under v2 the
/// leaf is a trie root. zheng opens trie paths next (specs/state.md § root
/// versions); until then proofs pin v1.
fn sample_state() -> BbgState {
    let mut state = BbgState::new();
    state.set_root_version(bbg::RootVersion::V1);
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

/// `[17 [[1 0] [1 11]]]`: read cell 11 of the particles dimension — the
/// first entry's energy (`[header(3)|key(8) | energy_lo, …]`).
fn read_first_energy() -> N {
    let pair = |a, b| N::Pair(Box::new(a), Box::new(b));
    pair(
        N::Atom(17),
        pair(pair(N::Atom(1), N::Atom(0)), pair(N::Atom(1), N::Atom(11))),
    )
}

/// The verifier's pipeline: require the statement to name the root it
/// trusts; zheng authenticates the evidence under that root and every read.
fn verify_against(
    trusted_root: [u64; 4],
    state: &StateCertificate,
    statement: &StateStatement,
    certificate: &Certificate,
) -> Result<(), String> {
    if statement.state_root != trusted_root {
        return Err("statement names another state root".into());
    }
    statement.verify_certificate(certificate, &state.evidence()?)
}

#[test]
fn zheng_and_bbg_agree_on_the_root() {
    let state = sample_state();
    let cert = StateCertificate::from_state(&state, &[0, 3]).unwrap();
    let evidence = cert.evidence().unwrap();
    assert_eq!(evidence.root().unwrap(), cert.root().unwrap());
    assert_eq!(
        evidence.root().unwrap().iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>(),
        state.root().to_vec()
    );
    let mut tampered = evidence.clone();
    tampered.tables[0].fields[11] += 1;
    assert!(tampered.authenticate(cert.root().unwrap()).is_err());
}

#[test]
fn certified_state_read_binds_value_key_root_and_witness() {
    let state = sample_state();
    let cert = StateCertificate::from_state(&state, &[0]).unwrap();
    let root = cert.root().unwrap();
    let (statement, certificate) =
        certify_state_execution(&read_first_energy(), &[], 1000, true, &cert.evidence().unwrap())
            .unwrap();
    assert_eq!(statement.state_root, root);
    assert_eq!(statement.execution.public_output, vec![77]);
    verify_against(root, &cert, &statement, &certificate).unwrap();

    for change in 0..4 {
        let mut altered = statement.clone();
        match change {
            0 => altered.execution.public_output[0] += 1,
            1 => altered.reads[0].key += 1,
            2 => altered.reads[0].value += 1,
            _ => altered.reads.clear(),
        }
        assert!(
            verify_against(root, &cert, &altered, &certificate).is_err(),
            "statement change {change}"
        );
    }
    // The root limbs are pinned but this program never computes on them: the
    // root is bound by authenticating every read under it (zheng
    // specs/execution.md, authenticated-state profile v3). A statement naming
    // another root fails that authentication, whichever root the verifier trusts.
    let mut other_root = statement.clone();
    other_root.state_root[0] = (other_root.state_root[0] + 1) % nebu::field::P;
    assert!(verify_against(root, &cert, &other_root, &certificate).is_err());
    assert!(verify_against(other_root.state_root, &cert, &other_root, &certificate).is_err());
    // Every wire of a bare read is pinned (constant, root, read coordinates,
    // output, cost): the certificate carries no free value, and any value it
    // does carry is rejected.
    assert!(certificate.free.is_empty());
    for extra in [1, 77, nebu::field::P - 1] {
        let mut padded = certificate.clone();
        padded.free.push(extra);
        assert!(verify_against(root, &cert, &statement, &padded).is_err(), "extra {extra}");
    }

    let mut altered = cert.clone();
    altered.dimensions[0].fields[11] += 1;
    assert!(altered.verify(root).is_err());
}

#[test]
fn read_against_a_stale_root_is_rejected() {
    let state = sample_state();
    let stale_cert = StateCertificate::from_state(&state, &[0]).unwrap();
    let stale_root = stale_cert.root().unwrap();
    let (statement, certificate) = certify_state_execution(
        &read_first_energy(),
        &[],
        1000,
        true,
        &stale_cert.evidence().unwrap(),
    )
    .unwrap();
    assert_eq!(statement.state_root, stale_root);

    // The state advances: a particle sorting before [1; 32] moves into the
    // first entry, so cell 11 now holds 99 and the root the statement names
    // is stale.
    let mut state = state;
    state.particles.insert(
        [0u8; 32],
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
    let current = StateCertificate::from_state(&state, &[0]).unwrap();
    let current_root = current.root().unwrap();
    assert_ne!(current_root, stale_root);

    assert_eq!(current.cell(0, 11), Some(99));
    assert!(current.verify(stale_root).is_err());
    assert!(verify_against(current_root, &current, &statement, &certificate).is_err());
    let mut rerooted = statement.clone();
    rerooted.state_root = current_root;
    assert!(verify_against(current_root, &current, &rerooted, &certificate).is_err());
}
