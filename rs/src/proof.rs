// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Reads over committed BBG dimensions.
//!
//! bbg is unified polynomial state: each dimension is a sorted polynomial
//! committed via Brakedown (see `dim.rs`). The one read primitive is an
//! **index-addressed cell open** — `open_cell(dim, idx)` opens the dimension
//! polynomial at the hypercube corner of cell `idx`, so the opened value IS
//! `evals[idx]`, bound to the dimension commitment (and thus the BBG root). This
//! matches what nox's `look` pattern and zheng's `look_openings_from_provider`
//! already assume (key = flat index → corner).
//!
//! Everything entity / record / relational composes ABOVE this primitive (in
//! inf), not here: inclusion = open the key cells + check `== K`; non-inclusion =
//! open the two adjacent sorted keys + a range check; a record = several cell
//! opens. bbg only commits sorted dimension polynomials and opens cells.
//! `prove_*` are thin entity-keyed conveniences (locate, then `open_cell`) for
//! light clients. See bbg/roadmap/provable-reads.md.

use lens::{brakedown::Brakedown, Commitment, Lens, MultilinearPoly, Opening, Transcript as LensTx};
use nebu::Goldilocks;

use crate::dim::{goldilocks_from_bytes32, goldilocks_from_u64};
use crate::query::Dim;
use crate::state::{balance_key, BbgState};
use crate::types::{NeuronId, Particle};

/// A proof that a committed cell of a BBG dimension has a given value.
pub struct QueryProof {
    pub commitment: Commitment,
    pub opening: Opening,
    pub value_bytes: Vec<u8>,
    /// The hypercube corner (LSB-first) of the opened cell.
    pub point: Vec<Goldilocks>,
}

// ── dimension layout (single source of truth) ─────────────────────────────────

/// The `(key, value-fields)` entries of a dimension, in committed (sorted) order.
///
/// Single source of truth for each dimension's on-poly layout, so the
/// entity-keyed `prove_*` path and the index-addressed `open_cell` path cannot
/// diverge. Flattened, each entry occupies `[key(4 elems) | value-fields]`.
fn dim_entries(state: &BbgState, dim: Dim) -> Vec<(Particle, Vec<Goldilocks>)> {
    let gu = goldilocks_from_u64;
    match dim {
        Dim::Particles => state
            .particles
            .iter()
            .map(|(k, v)| {
                (*k, vec![gu(v.energy), gu(v.pi_star), gu(v.weight), gu(v.s_yes), gu(v.s_no), gu(v.meta_score)])
            })
            .collect(),
        Dim::AxonsOut => state
            .axons_out
            .iter()
            .map(|(k, v)| {
                let mut vals = vec![gu(v.len() as u64)];
                for c in v {
                    vals.extend_from_slice(&goldilocks_from_bytes32(c));
                }
                (*k, vals)
            })
            .collect(),
        Dim::AxonsIn => state
            .axons_in
            .iter()
            .map(|(k, v)| {
                let mut vals = vec![gu(v.len() as u64)];
                for c in v {
                    vals.extend_from_slice(&goldilocks_from_bytes32(c));
                }
                (*k, vals)
            })
            .collect(),
        Dim::Neurons => state
            .neurons
            .iter()
            .map(|(k, v)| (*k, vec![gu(v.focus), gu(v.karma), gu(v.stake)]))
            .collect(),
        Dim::Locations => state
            .locations
            .iter()
            .map(|(k, v)| (*k, vec![gu(v.lat as u32 as u64), gu(v.lon as u32 as u64)]))
            .collect(),
        Dim::Coins => state.coins.iter().map(|(k, v)| (*k, vec![gu(v.total_supply)])).collect(),
        Dim::Cards => state
            .cards
            .iter()
            .map(|(k, v)| {
                let mut vals = goldilocks_from_bytes32(&v.owner).to_vec();
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.particle));
                (*k, vals)
            })
            .collect(),
        Dim::Files => state
            .files
            .iter()
            .map(|(k, v)| (*k, vec![gu(v.available as u64), gu(v.chunk_count as u64)]))
            .collect(),
        Dim::Time => state
            .time
            .iter()
            .map(|(h, p)| {
                let mut key = [0u8; 32];
                key[..8].copy_from_slice(&h.to_le_bytes());
                (key, goldilocks_from_bytes32(p).to_vec())
            })
            .collect(),
        Dim::Signals => state
            .signals
            .iter()
            .map(|(s, v)| {
                let mut key = [0u8; 32];
                key[..8].copy_from_slice(&s.to_le_bytes());
                let mut vals = goldilocks_from_bytes32(&v.neuron).to_vec();
                vals.push(gu(v.link_count as u64));
                vals.push(gu(v.block_height));
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.proof_hash));
                (key, vals)
            })
            .collect(),
        Dim::Balances => state.balances.iter().map(|(k, v)| (*k, vec![gu(*v)])).collect(),
    }
}

// ── the read primitive: index-addressed cell open ─────────────────────────────

/// Open dimension `dim` at the hypercube corner of cell `idx` — the opened value
/// IS `evals[idx]`, bound to the dimension commitment. `None` if `idx` is out of
/// range. This is the one read primitive; everything else composes above it.
pub fn open_cell(state: &BbgState, dim: Dim, idx: usize) -> Option<QueryProof> {
    open_cell_from_entries(&dim_entries(state, dim), idx)
}

/// The value of cell `idx` in `dim`, no proof. Matches `open_cell`'s value.
pub fn cell_value(state: &BbgState, dim: Dim, idx: usize) -> Option<Goldilocks> {
    dim_serialize(&dim_entries(state, dim)).get(idx).copied()
}

// ── entity-keyed conveniences (light clients): locate an entity, open its cell ─

/// Open the primary cell of `particle` in the particles dimension.
pub fn prove_particle(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::Particles), particle, 0)
}

/// Verify a query proof: the opening proves `value` at `point` under `commitment`.
pub fn verify_particle(proof: &QueryProof, _root: &Particle, _particle: &Particle) -> bool {
    let value = eval_value_from_bytes(&proof.value_bytes);
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&proof.commitment, &proof.point, value, &proof.opening, &mut tx)
}

pub fn prove_neuron(state: &BbgState, id: &NeuronId) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::Neurons), id, 0)
}

pub fn prove_axons_out(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::AxonsOut), particle, 0)
}

pub fn prove_axons_in(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::AxonsIn), particle, 0)
}

pub fn prove_location(state: &BbgState, id: &Particle) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::Locations), id, 0)
}

pub fn prove_coin(state: &BbgState, denom: &Particle) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::Coins), denom, 0)
}

pub fn prove_card(state: &BbgState, card_id: &Particle) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::Cards), card_id, 0)
}

pub fn prove_file(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_dim(&dim_entries(state, Dim::Files), particle, 0)
}

pub fn prove_signal(state: &BbgState, step: u64) -> Option<QueryProof> {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&step.to_le_bytes());
    // Signals' primary scalar is link_count, at value offset 4 (neuron id = 4 elems first).
    open_dim(&dim_entries(state, Dim::Signals), &key, 4)
}

pub fn prove_time(state: &BbgState, height: u64) -> Option<QueryProof> {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&height.to_le_bytes());
    open_dim(&dim_entries(state, Dim::Time), &key, 0)
}

pub fn prove_balances(state: &BbgState, owner: &[u8; 32], token: &[u8; 32]) -> Option<QueryProof> {
    let key = balance_key(owner, token);
    open_dim(&dim_entries(state, Dim::Balances), &key, 0)
}

/// Open the A(x) polynomial (private commitments) at the given point.
pub fn prove_commitment(state: &BbgState, point: &[u8; 32]) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> =
        state.commitments.iter().map(|(k, v)| (*k, vec![*v])).collect();
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

/// Flatten entries into the committed `[key(4)|val|…]` field vector (unpadded).
/// Cell index `i` of this vector is what `look(ns, i)` reads.
fn dim_serialize(entries: &[(Particle, Vec<Goldilocks>)]) -> Vec<Goldilocks> {
    let mut elems: Vec<Goldilocks> = Vec::new();
    for (k, vals) in entries {
        elems.extend_from_slice(&goldilocks_from_bytes32(k));
        elems.extend_from_slice(vals);
    }
    elems
}

/// Build/commit the dimension poly and open at the corner of cell `idx`.
fn open_cell_from_entries(
    entries: &[(Particle, Vec<Goldilocks>)],
    idx: usize,
) -> Option<QueryProof> {
    if entries.is_empty() {
        return None;
    }
    let mut elems = dim_serialize(entries);

    // Pad to at least 2^KEY_VARS, then to the next power of two.
    const KEY_VARS: usize = 4;
    let target = elems.len().next_power_of_two().max(1 << KEY_VARS);
    elems.resize(target, Goldilocks::ZERO);
    if idx >= elems.len() {
        return None;
    }

    let poly = MultilinearPoly::new(elems);
    let commitment = Brakedown::commit(&poly);

    let point = corner_point(idx, poly.num_vars);
    let value = poly.evals[idx];
    let value_bytes = value.as_u64().to_le_bytes().to_vec();

    let mut tx = LensTx::new(b"bbg-dim-open");
    let opening = Brakedown::open(&poly, &point, &mut tx);

    Some(QueryProof { commitment, opening, value_bytes, point })
}

/// Entity-keyed open: locate `key`'s entry (sorted, variable width), open value
/// cell `value_col` at its corner. Thin convenience over `open_cell_from_entries`.
fn open_dim(
    entries: &[(Particle, Vec<Goldilocks>)],
    key: &Particle,
    value_col: usize,
) -> Option<QueryProof> {
    if entries.is_empty() {
        return None;
    }
    let mut offset = 0usize;
    let mut flat_idx = None;
    for (k, vals) in entries {
        if k == key {
            flat_idx = Some(offset + 4 + value_col);
            break;
        }
        offset += 4 + vals.len();
    }
    open_cell_from_entries(entries, flat_idx?)
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
