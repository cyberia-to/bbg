// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Query proof generation and verification for BBG dimensions.

use lens::{brakedown::Brakedown, Commitment, Lens, MultilinearPoly, Opening, Transcript as LensTx};
use nebu::Goldilocks;

use crate::dim::{goldilocks_from_bytes32, goldilocks_from_u64};
use crate::state::{balance_key, BbgState};
use crate::types::{Particle, NeuronId};

/// A proof that a specific record exists in a BBG dimension.
pub struct QueryProof {
    pub commitment: Commitment,
    pub opening: Opening,
    pub value_bytes: Vec<u8>,
    /// The full evaluation point used during `open` (padded to poly.num_vars).
    pub point: Vec<Goldilocks>,
}

/// Commit the particles dimension and open at the particle-derived point.
pub fn prove_particle(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
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

    open_dim(&entries, particle, 0)
}

/// Verify a particle query proof.
pub fn verify_particle(proof: &QueryProof, _root: &Particle, _particle: &Particle) -> bool {
    let value = eval_value_from_bytes(&proof.value_bytes);
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&proof.commitment, &proof.point, value, &proof.opening, &mut tx)
}

/// Commit the neurons dimension and open at the NeuronId-derived point.
pub fn prove_neuron(state: &BbgState, id: &NeuronId) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
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

    open_dim(&entries, id, 0)
}

/// Commit the axons_out dimension and open at the particle-derived point.
pub fn prove_axons_out(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
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

    open_dim(&entries, particle, 0)
}

/// Commit the axons_in dimension and open at the particle-derived point.
pub fn prove_axons_in(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .axons_in
        .iter()
        .map(|(k, v)| {
            let mut vals = vec![goldilocks_from_u64(v.len() as u64)];
            for c in v {
                vals.extend_from_slice(&goldilocks_from_bytes32(c));
            }
            (*k, vals)
        })
        .collect();

    open_dim(&entries, particle, 0)
}

/// Commit the locations dimension and open at the particle-derived point.
pub fn prove_location(state: &BbgState, id: &Particle) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .locations
        .iter()
        .map(|(k, v)| {
            let vals = vec![
                goldilocks_from_u64(v.lat as u32 as u64),
                goldilocks_from_u64(v.lon as u32 as u64),
            ];
            (*k, vals)
        })
        .collect();

    open_dim(&entries, id, 0)
}

/// Commit the coins dimension and open at the particle-derived point.
pub fn prove_coin(state: &BbgState, denom: &Particle) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .coins
        .iter()
        .map(|(k, v)| (*k, vec![goldilocks_from_u64(v.total_supply)]))
        .collect();

    open_dim(&entries, denom, 0)
}

/// Commit the cards dimension and open at the particle-derived point.
pub fn prove_card(state: &BbgState, card_id: &Particle) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .cards
        .iter()
        .map(|(k, v)| {
            let mut vals: Vec<Goldilocks> = goldilocks_from_bytes32(&v.owner).to_vec();
            vals.extend_from_slice(&goldilocks_from_bytes32(&v.particle));
            (*k, vals)
        })
        .collect();

    open_dim(&entries, card_id, 0)
}

/// Commit the files dimension and open at the particle-derived point.
pub fn prove_file(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .files
        .iter()
        .map(|(k, v)| {
            let vals = vec![
                goldilocks_from_u64(v.available as u64),
                goldilocks_from_u64(v.chunk_count as u64),
            ];
            (*k, vals)
        })
        .collect();

    open_dim(&entries, particle, 0)
}

/// Commit the signals dimension and open at the step-derived point.
pub fn prove_signal(state: &BbgState, step: u64) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .signals
        .iter()
        .map(|(s, v)| {
            let mut key = [0u8; 32];
            key[..8].copy_from_slice(&s.to_le_bytes());
            let mut vals: Vec<Goldilocks> = goldilocks_from_bytes32(&v.neuron).to_vec();
            vals.push(goldilocks_from_u64(v.link_count as u64));
            vals.push(goldilocks_from_u64(v.block_height));
            vals.extend_from_slice(&goldilocks_from_bytes32(&v.proof_hash));
            (key, vals)
        })
        .collect();

    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&step.to_le_bytes());
    // Signals' primary scalar is link_count, at value offset 4 (neuron id = 4 elems first).
    open_dim(&entries, &key, 4)
}

/// Commit the time dimension and open at the height-derived point.
pub fn prove_time(state: &BbgState, height: u64) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .time
        .iter()
        .map(|(h, particle)| {
            let mut key = [0u8; 32];
            key[..8].copy_from_slice(&h.to_le_bytes());
            let vals: Vec<Goldilocks> = goldilocks_from_bytes32(particle).to_vec();
            (key, vals)
        })
        .collect();

    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&height.to_le_bytes());
    open_dim(&entries, &key, 0)
}

/// Commit the balances dimension and open at H(owner || token).
pub fn prove_balances(state: &BbgState, owner: &[u8; 32], token: &[u8; 32]) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .balances
        .iter()
        .map(|(k, v)| (*k, vec![goldilocks_from_u64(*v)]))
        .collect();
    let key = balance_key(owner, token);
    open_dim(&entries, &key, 0)
}

/// Commit the A(x) polynomial (private commitments) and open at the given point.
pub fn prove_commitment(state: &BbgState, point: &[u8; 32]) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .commitments
        .iter()
        .map(|(k, v)| (*k, vec![*v]))
        .collect();

    open_dim(&entries, point, 0)
}

// ── internals ────────────────────────────────────────────────────────────────

/// The LSB-first hypercube corner for a flat evaluation index.
///
/// `MultilinearPoly::evaluate` uses `bit j of idx → point[j]` (lens types.rs),
/// so evaluating at this corner returns `evals[idx]` exactly. Matches zheng's
/// `look_openings_from_provider` convention.
fn corner_point(idx: usize, num_vars: usize) -> Vec<Goldilocks> {
    (0..num_vars)
        .map(|j| if (idx >> j) & 1 == 1 { Goldilocks::ONE } else { Goldilocks::ZERO })
        .collect()
}

/// Build, commit, and open a dimension at the HYPERCUBE CORNER of an entity's
/// value cell — so the opened value IS the committed cell, not an off-corner MLE
/// fingerprint.
///
/// `value_col` is the offset of the wanted scalar within the entity's value
/// block (0 for the primary field of most dimensions; 4 for `Signals`, whose
/// first value field is the 4-element neuron id). Returns `None` if `key` is
/// absent — a real cell can only be opened for a record that exists.
///
/// NOTE (soundness scope): this proves the opened value is a real committed cell
/// of the dimension. It does NOT yet bind the opened index to `key` in a verifier
/// circuit (a prover could open another entity's cell). Closing that is the L1
/// record read (open the key-block at the same entry and constrain it == key
/// inside zheng) — see bbg/roadmap/provable-reads.md.
fn open_dim(
    entries: &[(Particle, Vec<Goldilocks>)],
    key: &Particle,
    value_col: usize,
) -> Option<QueryProof> {
    if entries.is_empty() {
        return None;
    }

    // Locate the entity's flat value-cell index. Entries are sorted; value
    // widths vary per entry (e.g. axon adjacency lists), so accumulate offsets.
    let mut offset = 0usize;
    let mut flat_idx: Option<usize> = None;
    for (k, vals) in entries {
        if k == key {
            flat_idx = Some(offset + 4 + value_col);
            break;
        }
        offset += 4 + vals.len();
    }
    let flat_idx = flat_idx?;

    // Serialize the dimension into a flat field vector (same layout as commit_dim).
    let mut elems: Vec<Goldilocks> = Vec::new();
    for (k, vals) in entries {
        elems.extend_from_slice(&goldilocks_from_bytes32(k));
        elems.extend_from_slice(vals);
    }

    // Pad to at least 2^KEY_VARS, then to the next power of two.
    const KEY_VARS: usize = 4;
    let target = elems.len().next_power_of_two().max(1 << KEY_VARS);
    elems.resize(target, Goldilocks::ZERO);

    let poly = MultilinearPoly::new(elems);
    let commitment = Brakedown::commit(&poly);

    // Open at the corner of the value cell: evaluate(corner) == evals[flat_idx].
    let point = corner_point(flat_idx, poly.num_vars);
    let value = poly.evals[flat_idx];
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
