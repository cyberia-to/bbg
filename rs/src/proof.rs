// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Authenticated reads over committed BBG dimensions.
//!
//! Public namespaces 0..9 carry a complete-table StateCertificate in version-3
//! query context. A cell is addressed by its unpadded flat index; entity helpers
//! locate the primary value cell and `verify_entity` checks the exact key layout.
//! Full u64 values occupy two u32 cells; one cell proof does not claim a record.
//!
//! Private balance/A openings stay contextless and disclose no additional table.
//! They cannot establish a state-root/entity claim through the public query API.
//! See specs/query.md for bounds and the complete authentication contract.

use lens::{
    Commitment, Lens, MultilinearPoly, Opening, Transcript as LensTx, brakedown::Brakedown,
};
use nebu::Goldilocks;

use crate::dim::{
    HEADER_FIELDS, KEY_FIELDS, bytes32_limbs as goldilocks_from_bytes32, dim_serialize,
    scalar_fields,
};
use crate::query::Dim;
use crate::query_auth::{MAX_QUERY_FIELDS, QueryContext};
use crate::state::{BbgState, balance_key};
use crate::types::{NeuronId, Particle};

/// A proof that a committed cell of a BBG dimension has a given value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
pub struct QueryProof {
    pub commitment: Commitment,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::query_wire::opening")
    )]
    pub opening: Opening,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::query_wire::value")
    )]
    pub value_bytes: Vec<u8>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub context: Option<QueryContext>,
    /// The hypercube corner (LSB-first) of the opened cell.
    #[cfg_attr(feature = "serde", serde(with = "goldilocks_vec"))]
    pub point: Vec<Goldilocks>,
}

/// Canonical serde for `Vec<Goldilocks>`: each element is its canonical u64
/// in `[0, p)`. Deserialization rejects non-canonical values, so every point
/// has exactly one valid encoding.
#[cfg(feature = "serde")]
mod goldilocks_vec {
    use nebu::Goldilocks;
    use nebu::field::P;

    pub fn serialize<S: serde::Serializer>(
        v: &Vec<Goldilocks>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(v.iter().map(|g| g.as_u64()))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Goldilocks>, D::Error> {
        let raw: Vec<u64> = crate::query_wire::point(deserializer)?;
        raw.into_iter()
            .map(|u| {
                if u < P {
                    Ok(Goldilocks::new(u))
                } else {
                    Err(serde::de::Error::custom(format!(
                        "non-canonical Goldilocks element {u} (>= modulus)"
                    )))
                }
            })
            .collect()
    }
}

// ── dimension layout (single source of truth) ─────────────────────────────────

/// The `(key, value-fields)` entries of a dimension, in committed (sorted) order.
///
/// Single source of truth for each dimension's on-poly layout, so the
/// entity-keyed `prove_*` path and the index-addressed `open_cell` path cannot
/// diverge. Flattened, each entry occupies `[key(8 u32 limbs) | value-fields]`.
pub(crate) fn dim_entries(state: &BbgState, dim: Dim) -> Vec<(Particle, Vec<Goldilocks>)> {
    let fields = scalar_fields;
    match dim {
        Dim::Particles => state
            .particles
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    fields(&[v.energy, v.pi_star, v.weight, v.s_yes, v.s_no, v.meta_score]),
                )
            })
            .collect(),
        Dim::AxonsOut => state
            .axons_out
            .iter()
            .map(|(k, v)| {
                let mut vals = fields(&[v.len() as u64]);
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
                let mut vals = fields(&[v.len() as u64]);
                for c in v {
                    vals.extend_from_slice(&goldilocks_from_bytes32(c));
                }
                (*k, vals)
            })
            .collect(),
        Dim::Neurons => state
            .neurons
            .iter()
            .map(|(k, v)| (*k, fields(&[v.focus, v.karma, v.stake])))
            .collect(),
        Dim::Locations => state
            .locations
            .iter()
            .map(|(k, v)| (*k, fields(&[v.lat as u32 as u64, v.lon as u32 as u64])))
            .collect(),
        Dim::Coins => state
            .coins
            .iter()
            .map(|(k, v)| (*k, fields(&[v.total_supply])))
            .collect(),
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
            .map(|(k, v)| (*k, fields(&[v.available as u64, v.chunk_count as u64])))
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
                vals.extend(goldilocks_from_bytes32(&v.network));
                vals.extend(fields(&[v.link_count as u64, v.block_height]));
                vals.extend_from_slice(&goldilocks_from_bytes32(&v.proof_hash));
                (key, vals)
            })
            .collect(),
        Dim::Balances => state
            .balances
            .iter()
            .map(|(k, v)| (*k, fields(&[*v])))
            .collect(),
    }
}

// ── the read primitive: index-addressed cell open ─────────────────────────────

/// Open dimension `dim` at the hypercube corner of cell `idx` — the opened value
/// IS `evals[idx]`, bound to the dimension commitment. `None` if `idx` is out of
/// range. This is the one read primitive; everything else composes above it.
pub fn open_cell(state: &BbgState, dim: Dim, idx: usize) -> Option<QueryProof> {
    let entries = dim_entries(state, dim);
    let count = HEADER_FIELDS
        + entries
            .iter()
            .map(|(_, v)| KEY_FIELDS + v.len())
            .sum::<usize>();
    if (dim as u64) <= 9 && count > MAX_QUERY_FIELDS {
        return None;
    }
    let mut proof = open_cell_from_entries(&entries, idx)?;
    if (dim as u64) <= 9 {
        proof.context = Some(QueryContext::from_state(state, dim, idx)?);
    }
    Some(proof)
}

/// The value of cell `idx` in `dim`, no proof. Matches `open_cell`'s value.
pub fn cell_value(state: &BbgState, dim: Dim, idx: usize) -> Option<Goldilocks> {
    dim_serialize(&dim_entries(state, dim)).get(idx).copied()
}

// ── entity-keyed conveniences (light clients): locate an entity, open its cell ─

/// Open the primary cell of `particle` in the particles dimension.
pub fn prove_particle(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_entity(state, Dim::Particles, particle, 0)
}

/// Verify this particle's primary cell under the caller's trusted state root.
/// Contextless legacy/private openings cannot establish this claim.
pub fn verify_particle(proof: &QueryProof, root: &Particle, particle: &Particle) -> bool {
    crate::query_auth::verify_entity(proof, root, Dim::Particles, particle)
}

pub fn prove_neuron(state: &BbgState, id: &NeuronId) -> Option<QueryProof> {
    open_entity(state, Dim::Neurons, id, 0)
}

pub fn prove_axons_out(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_entity(state, Dim::AxonsOut, particle, 0)
}

pub fn prove_axons_in(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_entity(state, Dim::AxonsIn, particle, 0)
}

pub fn prove_location(state: &BbgState, id: &Particle) -> Option<QueryProof> {
    open_entity(state, Dim::Locations, id, 0)
}

pub fn prove_coin(state: &BbgState, denom: &Particle) -> Option<QueryProof> {
    open_entity(state, Dim::Coins, denom, 0)
}

pub fn prove_card(state: &BbgState, card_id: &Particle) -> Option<QueryProof> {
    open_entity(state, Dim::Cards, card_id, 0)
}

pub fn prove_file(state: &BbgState, particle: &Particle) -> Option<QueryProof> {
    open_entity(state, Dim::Files, particle, 0)
}

pub fn prove_signal(state: &BbgState, step: u64) -> Option<QueryProof> {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&step.to_le_bytes());
    // link_count follows two eight-limb IDs: neuron and network.
    open_entity(state, Dim::Signals, &key, 16)
}

pub fn prove_time(state: &BbgState, height: u64) -> Option<QueryProof> {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&height.to_le_bytes());
    open_entity(state, Dim::Time, &key, 0)
}

pub fn prove_balances(state: &BbgState, owner: &[u8; 32], token: &[u8; 32]) -> Option<QueryProof> {
    let key = balance_key(owner, token);
    open_entity(state, Dim::Balances, &key, 0)
}

/// Explicitly disclose the complete opt-in plaintext balance table. Private
/// A/N contents are never included. Pin the result with verify_public_balance.
pub fn prove_public_balance(
    state: &BbgState,
    owner: &[u8; 32],
    token: &[u8; 32],
) -> Option<QueryProof> {
    let fields = HEADER_FIELDS.checked_add(state.balances.len().checked_mul(KEY_FIELDS + 2)?)?;
    if fields > MAX_QUERY_FIELDS {
        return None;
    }
    let key = balance_key(owner, token);
    let position = state.balances.keys().position(|k| k == &key)?;
    let index = HEADER_FIELDS + position * (KEY_FIELDS + 2) + KEY_FIELDS;
    let mut proof = prove_balances(state, owner, token)?;
    proof.context = Some(QueryContext::from_state(state, Dim::Balances, index)?);
    Some(proof)
}

/// Open the A(x) polynomial (private commitments) at the given point.
pub fn prove_commitment(state: &BbgState, point: &[u8; 32]) -> Option<QueryProof> {
    let entries: Vec<(Particle, Vec<Goldilocks>)> = state
        .commitments
        .iter()
        .map(|(k, v)| (*k, vec![*v]))
        .collect();
    open_dim(&entries, point, 0)
}

fn open_entity(state: &BbgState, dim: Dim, key: &Particle, value_col: usize) -> Option<QueryProof> {
    let entries = dim_entries(state, dim);
    let mut offset = HEADER_FIELDS;
    for (entry_key, values) in entries {
        if entry_key == *key {
            return (value_col < values.len())
                .then(|| open_cell(state, dim, offset + KEY_FIELDS + value_col))
                .flatten();
        }
        offset += KEY_FIELDS + values.len();
    }
    None
}

// ── internals ────────────────────────────────────────────────────────────────

/// The LSB-first hypercube corner for a flat evaluation index.
///
/// `MultilinearPoly::evaluate` uses `bit j of idx → point[j]` (lens types.rs),
/// so evaluating at this corner returns `evals[idx]` exactly. Matches zheng's
/// `look_openings_from_provider` convention.
fn corner_point(idx: usize, num_vars: usize) -> Vec<Goldilocks> {
    (0..num_vars)
        .map(|j| {
            if (idx >> j) & 1 == 1 {
                Goldilocks::ONE
            } else {
                Goldilocks::ZERO
            }
        })
        .collect()
}

/// Build/commit the dimension poly and open at the corner of cell `idx`.
fn open_cell_from_entries(
    entries: &[(Particle, Vec<Goldilocks>)],
    idx: usize,
) -> Option<QueryProof> {
    let mut elems = dim_serialize(entries);

    if idx >= elems.len() {
        return None;
    }
    let target = elems.len().next_power_of_two();
    elems.resize(target, Goldilocks::ZERO);

    let poly = MultilinearPoly::new(elems);
    let commitment = Brakedown::commit(&poly);

    let point = corner_point(idx, poly.num_vars);
    let value = poly.evals[idx];
    let value_bytes = value.as_u64().to_le_bytes().to_vec();

    let mut tx = LensTx::new(b"bbg-dim-open");
    let opening = Brakedown::open(&poly, &point, &mut tx);

    Some(QueryProof {
        commitment,
        opening,
        value_bytes,
        point,
        context: None,
    })
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
    let mut offset = HEADER_FIELDS;
    let mut flat_idx = None;
    for (k, vals) in entries {
        if k == key {
            if value_col >= vals.len() {
                return None;
            }
            flat_idx = Some(offset + KEY_FIELDS + value_col);
            break;
        }
        offset += KEY_FIELDS + vals.len();
    }
    open_cell_from_entries(entries, flat_idx?)
}
