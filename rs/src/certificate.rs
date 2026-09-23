//! Exact public-table authentication for consumed BBG dimensions.
//! The certificate reveals complete tables. See specs/state-certificate.md.
use crate::dim::{DIMENSION_VERSION, HEADER_FIELDS, KEY_FIELDS, commit_fields, dim_serialize};
use crate::proof::dim_entries;
use crate::{BbgState, Dim};
use nebu::Goldilocks;

pub const CERTIFICATE_VERSION: u32 = 2;
pub const MAX_FIELDS: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
pub struct DimensionTable {
    pub namespace: u64,
    #[cfg_attr(feature = "serde", serde(deserialize_with = "bounded::fields"))]
    pub fields: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
pub struct StateCertificate {
    pub version: u32,
    pub lens_version: u32,
    /// Public dimensions 0..10, A, N, statistics, in root-compression order.
    pub leaves: [[u64; 4]; 14],
    #[cfg_attr(feature = "serde", serde(deserialize_with = "bounded::dimensions"))]
    pub dimensions: Vec<DimensionTable>,
}

impl StateCertificate {
    pub fn from_state(state: &BbgState, namespaces: &[u64]) -> Result<Self, String> {
        if namespaces.len() > 11 || namespaces.iter().any(|&n| n > 10) {
            return Err("certificate namespaces must be in 0..10".into());
        }
        let mut namespaces = namespaces.to_vec();
        namespaces.sort_unstable();
        if namespaces.windows(2).any(|ns| ns[0] == ns[1]) {
            return Err("duplicate certificate namespace".into());
        }
        let mut dimensions = Vec::new();
        let mut total = 0usize;
        for namespace in namespaces {
            let entries = dim_entries(state, Dim::from_u64(namespace).unwrap());
            let count = HEADER_FIELDS
                + entries
                    .iter()
                    .map(|(_, v)| KEY_FIELDS + v.len())
                    .sum::<usize>();
            total = total
                .checked_add(count)
                .ok_or("certificate size overflow")?;
            if total > MAX_FIELDS {
                return Err("certificate field limit exceeded".into());
            }
            dimensions.push(DimensionTable {
                namespace,
                fields: dim_serialize(&entries).iter().map(|v| v.as_u64()).collect(),
            });
        }
        let root_leaves = state.root_leaves();
        let mut leaves = [[0; 4]; 14];
        for (dest, values) in leaves.iter_mut().zip(root_leaves.dims.iter().chain([
            &root_leaves.a,
            &root_leaves.n,
            &root_leaves.stats,
        ])) {
            *dest = values.map(|v| v.as_u64());
        }
        let certificate = Self {
            version: CERTIFICATE_VERSION,
            lens_version: lens::brakedown::COMMITMENT_VERSION,
            leaves,
            dimensions,
        };
        let root = certificate.root()?;
        let cached = state.root();
        if root.iter().flat_map(|v| v.to_le_bytes()).ne(cached) {
            return Err("stale cached BBG root; refresh state after mutation".into());
        }
        Ok(certificate)
    }

    /// Authenticate every included table and recompute the complete state root.
    pub fn root(&self) -> Result<[u64; 4], String> {
        self.validate_shape()?;
        for table in &self.dimensions {
            let fields: Vec<_> = table.fields.iter().map(|&v| Goldilocks::new(v)).collect();
            let commitment = commit_fields(&fields);
            let expected: Vec<_> = self.leaves[table.namespace as usize]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            if commitment.as_bytes() != expected {
                return Err(format!(
                    "dimension {} does not match its root commitment",
                    table.namespace
                ));
            }
        }
        let field_leaves = self.leaves.map(|leaf| leaf.map(Goldilocks::new));
        let leaves = zheng::RootLeaves {
            dims: field_leaves[..11].try_into().unwrap(),
            a: field_leaves[11],
            n: field_leaves[12],
            stats: field_leaves[13],
        };
        Ok(zheng::root_from_leaves(&leaves).map(|v| v.as_u64()))
    }

    pub fn verify(&self, expected_root: [u64; 4]) -> Result<(), String> {
        if expected_root.iter().any(|&v| v >= nebu::field::P) {
            return Err("noncanonical expected state root".into());
        }
        if self.root()? != expected_root {
            return Err("state certificate root mismatch".into());
        }
        Ok(())
    }

    /// Read an unpadded cell. Verify against the trusted root before use.
    pub fn cell(&self, namespace: u64, key: u64) -> Option<u64> {
        if self.version != CERTIFICATE_VERSION
            || self.lens_version != lens::brakedown::COMMITMENT_VERSION
            || namespace > 10
            || self.dimensions.len() > 11
        {
            return None;
        }
        let index = usize::try_from(key).ok()?;
        let table = self.dimensions.iter().find(|t| t.namespace == namespace)?;
        if table.fields.len() < HEADER_FIELDS
            || table.fields.len() > MAX_FIELDS
            || table.fields[0] != DIMENSION_VERSION
            || table.fields[1] != table.fields.len() as u64
        {
            return None;
        }
        let value = *table.fields.get(index)?;
        (value < nebu::field::P).then_some(value)
    }

    fn validate_shape(&self) -> Result<(), String> {
        if self.version != CERTIFICATE_VERSION
            || self.lens_version != lens::brakedown::COMMITMENT_VERSION
        {
            return Err("unsupported state certificate or commitment version".into());
        }
        if self.leaves.iter().flatten().any(|&v| v >= nebu::field::P) {
            return Err("noncanonical root leaf".into());
        }
        if self.dimensions.len() > 11 {
            return Err("too many certificate dimensions".into());
        }
        let mut previous = None;
        let mut total = 0usize;
        for table in &self.dimensions {
            if table.namespace > 10 || previous.is_some_and(|n| n >= table.namespace) {
                return Err(
                    "certificate namespaces must be unique, increasing and in 0..10".into(),
                );
            }
            previous = Some(table.namespace);
            total = total
                .checked_add(table.fields.len())
                .ok_or("certificate size overflow")?;
            if total > MAX_FIELDS {
                return Err("certificate field limit exceeded".into());
            }
            if table.fields.len() < HEADER_FIELDS
                || table.fields[0] != DIMENSION_VERSION
                || table.fields[1] != table.fields.len() as u64
                || table.fields[2] > ((table.fields.len() - HEADER_FIELDS) / KEY_FIELDS) as u64
                || (table.fields[2] == 0 && table.fields.len() != HEADER_FIELDS)
                || table.fields.iter().any(|&v| v >= nebu::field::P)
            {
                return Err("invalid dimension metadata or noncanonical field".into());
            }
        }
        Ok(())
    }
}

#[cfg(feature = "serde")]
mod bounded {
    use super::*;
    use serde::{
        Deserialize, Deserializer,
        de::{Error, SeqAccess, Visitor},
    };
    use std::{fmt, marker::PhantomData};
    fn vector<'de, T: Deserialize<'de>, D: Deserializer<'de>, const MAX: usize>(
        d: D,
    ) -> Result<Vec<T>, D::Error> {
        struct Bounded<T, const MAX: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const MAX: usize> Visitor<'de> for Bounded<T, MAX> {
            type Value = Vec<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "at most {MAX} entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq.size_hint().is_some_and(|n| n > MAX) {
                    return Err(A::Error::custom("certificate allocation limit"));
                }
                let mut values = Vec::new();
                while let Some(value) = seq.next_element()? {
                    if values.len() == MAX {
                        return Err(A::Error::custom("certificate allocation limit"));
                    }
                    values.push(value);
                }
                Ok(values)
            }
        }
        d.deserialize_seq(Bounded::<T, MAX>(PhantomData))
    }
    pub fn fields<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u64>, D::Error> {
        vector::<u64, D, MAX_FIELDS>(d)
    }
    pub fn dimensions<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<DimensionTable>, D::Error> {
        vector::<DimensionTable, D, 11>(d)
    }
}
