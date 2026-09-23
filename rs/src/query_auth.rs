//! Exact public query authentication. Full public tables are disclosed once.
use crate::dim::{HEADER_FIELDS, KEY_FIELDS};
use crate::{BbgState, Dim, QueryProof, certificate::StateCertificate};
use lens::{Lens, Transcript, brakedown::Brakedown};
use nebu::Goldilocks;

pub const QUERY_VERSION: u32 = 3;
pub const MAX_QUERY_FIELDS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
pub struct QueryContext {
    pub version: u32,
    pub certificate: StateCertificate,
    pub namespace: u64,
    pub index: u64,
}

impl QueryContext {
    pub(crate) fn from_state(state: &BbgState, dim: Dim, index: usize) -> Option<Self> {
        Some(Self {
            version: QUERY_VERSION,
            certificate: StateCertificate::from_state(state, &[dim as u64]).ok()?,
            namespace: dim as u64,
            index: index as u64,
        })
    }
}

fn root_fields(root: &[u8; 32]) -> Option<[u64; 4]> {
    let values =
        std::array::from_fn(|i| u64::from_le_bytes(root[i * 8..i * 8 + 8].try_into().unwrap()));
    values.iter().all(|&v| v < nebu::field::P).then_some(values)
}

fn authenticated(proof: &QueryProof, root: Option<&[u8; 32]>) -> Option<()> {
    let context = proof.context.as_ref()?;
    if context.version != QUERY_VERSION
        || context.namespace > 10
        || context.certificate.dimensions.len() != 1
    {
        return None;
    }
    let table = &context.certificate.dimensions[0];
    if table.namespace != context.namespace || table.fields.len() > MAX_QUERY_FIELDS {
        return None;
    }
    let actual_root = context.certificate.root().ok()?;
    if let Some(root) = root {
        if actual_root != root_fields(root)? {
            return None;
        }
    }
    let index = usize::try_from(context.index).ok()?;
    let value = *table.fields.get(index)?;
    if proof.value_bytes.as_slice() != value.to_le_bytes() {
        return None;
    }
    let vars = table.fields.len().next_power_of_two().ilog2() as usize;
    if proof.point.len() != vars
        || proof
            .point
            .iter()
            .enumerate()
            .any(|(i, v)| v.as_u64() != ((index >> i) & 1) as u64)
    {
        return None;
    }
    let leaf_bytes: Vec<_> = context.certificate.leaves[context.namespace as usize]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    if proof.commitment.as_bytes() != leaf_bytes {
        return None;
    }
    // The exact authenticated table above establishes the cell claim. Sampling
    // is only a consistency check on the retained legacy opening field.
    Brakedown::verify(
        &proof.commitment,
        &proof.point,
        Goldilocks::new(value),
        &proof.opening,
        &mut Transcript::new(b"bbg-dim-open"),
    )
    .then_some(())
}

/// Authenticate the self-described public context. Pin external requests with
/// `verify_query_at` or `verify_entity`; this function accepts no trusted root.
pub fn verify_query(proof: &QueryProof) -> bool {
    authenticated(proof, None).is_some()
}

/// Authenticate a caller-selected public namespace and unpadded cell index.
pub fn verify_query_at(proof: &QueryProof, root: &[u8; 32], namespace: u64, index: u64) -> bool {
    proof
        .context
        .as_ref()
        .is_some_and(|c| c.namespace == namespace && c.index == index)
        && authenticated(proof, Some(root)).is_some()
}

/// Authenticate the primary field cell of an exact entity key in a public dim.
pub fn verify_entity(proof: &QueryProof, root: &[u8; 32], dim: Dim, key: &[u8; 32]) -> bool {
    if authenticated(proof, Some(root)).is_none() {
        return false;
    }
    let context = proof.context.as_ref().unwrap();
    if context.namespace != dim as u64 {
        return false;
    }
    let table = &context.certificate.dimensions[0].fields;
    primary_index(table, dim, key).is_some_and(|index| index as u64 == context.index)
}

fn primary_index(fields: &[u64], dim: Dim, key: &[u8; 32]) -> Option<usize> {
    let entries = usize::try_from(*fields.get(2)?).ok()?;
    let mut offset = HEADER_FIELDS;
    let mut found = None;
    let mut previous = None;
    for _ in 0..entries {
        let limbs = fields.get(offset..offset.checked_add(KEY_FIELDS)?)?;
        let mut bytes = [0u8; 32];
        for (part, &limb) in bytes.chunks_exact_mut(4).zip(limbs) {
            part.copy_from_slice(&u32::try_from(limb).ok()?.to_le_bytes());
        }
        // Numeric dimensions use BTreeMap<u64>; their eight-byte keys are LE.
        let sort_key = if matches!(dim, Dim::Time | Dim::Signals) {
            if bytes[8..].iter().any(|&v| v != 0) {
                return None;
            }
            let mut sort_key = [0; 32];
            sort_key[..8]
                .copy_from_slice(&u64::from_le_bytes(bytes[..8].try_into().ok()?).to_be_bytes());
            sort_key
        } else {
            bytes
        };
        if previous.is_some_and(|p| p >= sort_key) {
            return None;
        }
        previous = Some(sort_key);
        let start = offset + KEY_FIELDS;
        let width = match dim {
            Dim::Particles => 12,
            Dim::Neurons => 6,
            Dim::Locations => 4,
            Dim::Coins => 2,
            Dim::Cards => 16,
            Dim::Files => 4,
            Dim::Time => 8,
            Dim::Signals => 28,
            Dim::AxonsOut | Dim::AxonsIn => {
                let low = u32::try_from(*fields.get(start)?).ok()? as u64;
                let high = u32::try_from(*fields.get(start + 1)?).ok()? as u64;
                usize::try_from(low | high << 32)
                    .ok()?
                    .checked_mul(8)?
                    .checked_add(2)?
            }
            Dim::Balances => 2,
        };
        offset = start.checked_add(width)?;
        if fields
            .get(start..offset)?
            .iter()
            .any(|&v| v > u32::MAX as u64)
        {
            return None;
        }
        if &bytes == key {
            found = Some(start + if dim == Dim::Signals { 16 } else { 0 });
        }
    }
    if offset != fields.len() {
        return None;
    }
    found
}

/// Authenticate both limbs of an opt-in public balance under a trusted root.
pub fn verify_public_balance(
    proof: &QueryProof,
    root: &[u8; 32],
    owner: &[u8; 32],
    token: &[u8; 32],
) -> Option<u64> {
    let key = crate::state::balance_key(owner, token);
    if !verify_entity(proof, root, Dim::Balances, &key) {
        return None;
    }
    let context = proof.context.as_ref()?;
    let table = &context.certificate.dimensions[0].fields;
    let index = usize::try_from(context.index).ok()?;
    let low = u32::try_from(*table.get(index)?).ok()? as u64;
    let high = u32::try_from(*table.get(index + 1)?).ok()? as u64;
    Some(low | high << 32)
}

/// A legacy LookOpening alone cannot establish a root/cell claim. Supply the
/// corresponding complete public query and trusted root through this API.
pub fn verify_opening_with_context(
    lo: &zheng::LookOpening,
    proof: &QueryProof,
    root: &[u8; 32],
    index: u64,
) -> bool {
    if !verify_query_at(proof, root, lo.namespace.as_u64(), index) {
        return false;
    }
    let c = proof.context.as_ref().unwrap();
    let leaves: Vec<_> = lo
        .leaves
        .dims
        .iter()
        .chain([&lo.leaves.a, &lo.leaves.n, &lo.leaves.stats])
        .map(|v| v.map(|x| x.as_u64()))
        .collect();
    leaves.as_slice() == c.certificate.leaves
        && lo.commitment == proof.commitment
        && lo.point == proof.point
        && lo.value.as_u64().to_le_bytes().as_slice() == proof.value_bytes
        && lo.opening == proof.opening
        && lo.transcript_seed == b"bbg-dim-open"
}
