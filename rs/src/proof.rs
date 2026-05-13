// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Query proof generation and verification for BBG dimensions.

use lens::{brakedown::Brakedown, Commitment, Lens, MultilinearPoly, Opening, Transcript as LensTx};
use nebu::Goldilocks;

use crate::dim::{goldilocks_from_bytes32, goldilocks_from_u64};
use crate::state::BbgState;
use crate::types::{Cid, NeuronId};

/// A proof that a specific record exists in a BBG dimension.
pub struct QueryProof {
    pub commitment: Commitment,
    pub opening: Opening,
    pub value_bytes: Vec<u8>,
    /// The full evaluation point used during `open` (padded to poly.num_vars).
    pub point: Vec<Goldilocks>,
}

/// Build the evaluation point for a CID: 4 Goldilocks elements (8 bytes each).
fn cid_to_point(cid: &Cid) -> Vec<Goldilocks> {
    goldilocks_from_bytes32(cid).to_vec()
}

/// Commit the particles dimension and open at the CID-derived point.
pub fn prove_particle(state: &BbgState, cid: &Cid) -> Option<QueryProof> {
    let entries: Vec<(Cid, Vec<Goldilocks>)> = state
        .particles
        .iter()
        .map(|(k, v)| {
            let vals = vec![
                goldilocks_from_u64(v.energy),
                goldilocks_from_u64(v.pi_star),
                goldilocks_from_u64(v.weight),
                goldilocks_from_u64(v.s_yes),
                goldilocks_from_u64(v.s_no),
                goldilocks_from_u64(v.meta_score),
            ];
            (*k, vals)
        })
        .collect();

    open_dim(&entries, cid)
}

/// Verify a particle query proof.
pub fn verify_particle(proof: &QueryProof, _root: &Cid, _cid: &Cid) -> bool {
    let value = eval_value_from_bytes(&proof.value_bytes);
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&proof.commitment, &proof.point, value, &proof.opening, &mut tx)
}

/// Commit the neurons dimension and open at the NeuronId-derived point.
pub fn prove_neuron(state: &BbgState, id: &NeuronId) -> Option<QueryProof> {
    let entries: Vec<(Cid, Vec<Goldilocks>)> = state
        .neurons
        .iter()
        .map(|(k, v)| {
            let vals = vec![
                goldilocks_from_u64(v.focus),
                goldilocks_from_u64(v.karma),
                goldilocks_from_u64(v.stake),
            ];
            (*k, vals)
        })
        .collect();

    open_dim(&entries, id)
}

/// Verify a neuron query proof.
pub fn verify_neuron(proof: &QueryProof, _root: &Cid, _id: &NeuronId) -> bool {
    let value = eval_value_from_bytes(&proof.value_bytes);
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&proof.commitment, &proof.point, value, &proof.opening, &mut tx)
}

/// Commit the axons_out dimension and open at the CID-derived point.
pub fn prove_axons_out(state: &BbgState, cid: &Cid) -> Option<QueryProof> {
    let entries: Vec<(Cid, Vec<Goldilocks>)> = state
        .axons_out
        .iter()
        .map(|(k, v)| {
            let mut vals = vec![goldilocks_from_u64(v.len() as u64)];
            for c in v {
                vals.extend_from_slice(&goldilocks_from_bytes32(c));
            }
            (*k, vals)
        })
        .collect();

    open_dim(&entries, cid)
}

/// Verify an axons_out query proof.
pub fn verify_axons_out(proof: &QueryProof, _root: &Cid, _cid: &Cid) -> bool {
    let value = eval_value_from_bytes(&proof.value_bytes);
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&proof.commitment, &proof.point, value, &proof.opening, &mut tx)
}

// ── internals ────────────────────────────────────────────────────────────────

/// Build, commit, and open a dimension at the CID-derived evaluation point.
fn open_dim(entries: &[(Cid, Vec<Goldilocks>)], key: &Cid) -> Option<QueryProof> {
    if entries.is_empty() {
        return None;
    }

    // Serialize the dimension into a flat field-element vector.
    let mut elems: Vec<Goldilocks> = Vec::new();
    for (k, vals) in entries {
        let key_elems = goldilocks_from_bytes32(k);
        elems.extend_from_slice(&key_elems);
        elems.extend_from_slice(vals);
    }

    let target = elems.len().next_power_of_two();
    elems.resize(target, Goldilocks::ZERO);

    let poly = MultilinearPoly::new(elems);
    let commitment = Brakedown::commit(&poly);

    // Evaluation point: 4 field elements from the key (= num_vars up to 4,
    // padded with ZERO if the polynomial has more variables).
    let key_point = cid_to_point(key);
    if poly.num_vars < key_point.len() {
        return None;
    }
    let mut point = key_point;
    // Pad point with ZERO to match num_vars.
    while point.len() < poly.num_vars {
        point.push(Goldilocks::ZERO);
    }

    let value = poly.evaluate(&point);
    let value_bytes = value.as_u64().to_le_bytes().to_vec();

    let mut tx = LensTx::new(b"bbg-dim-open");
    let opening = Brakedown::open(&poly, &point, &mut tx);

    Some(QueryProof { commitment, opening, value_bytes, point })
}

/// Decode the first 8 bytes of `value_bytes` as a little-endian u64 Goldilocks element.
fn eval_value_from_bytes(bytes: &[u8]) -> Goldilocks {
    if bytes.len() < 8 {
        return Goldilocks::ZERO;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[..8]);
    Goldilocks::new(u64::from_le_bytes(buf))
}
