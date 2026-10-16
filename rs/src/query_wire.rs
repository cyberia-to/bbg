//! Bounded decoding of public query payloads; old opening variants are refused.
use lens::{ColumnQuery, Opening};
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
            write!(f, "at most {MAX} query entries")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX) {
                return Err(A::Error::custom("query allocation limit"));
            }
            let mut out = Vec::new();
            while let Some(value) = seq.next_element()? {
                if out.len() == MAX {
                    return Err(A::Error::custom("query allocation limit"));
                }
                out.push(value);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(Bounded::<T, MAX>(PhantomData))
}

pub fn point<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u64>, D::Error> {
    vector::<u64, D, 20>(d)
}
pub fn value<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let value = vector::<u8, D, 8>(d)?;
    if value.len() != 8 {
        return Err(D::Error::custom(
            "query value must contain exactly eight bytes",
        ));
    }
    if u64::from_le_bytes(value.as_slice().try_into().unwrap()) >= nebu::field::P {
        return Err(D::Error::custom("non-canonical query value"));
    }
    Ok(value)
}
fn bytes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    vector::<u8, D, 8192>(d)
}
fn path<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(hemera::Hash, hemera::Side)>, D::Error> {
    vector::<(hemera::Hash, hemera::Side), D, 12>(d)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Column {
    index: usize,
    #[serde(deserialize_with = "bytes")]
    column: Vec<u8>,
    #[serde(deserialize_with = "path")]
    path: Vec<(hemera::Hash, hemera::Side)>,
}
fn columns<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Column>, D::Error> {
    struct Columns;
    impl<'de> Visitor<'de> for Columns {
        type Value = Vec<Column>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("query columns up to 16 MiB")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            let mut bytes = 0usize;
            if seq.size_hint().is_some_and(|n| n > 307200) {
                return Err(A::Error::custom("query column limit"));
            }
            while let Some(value) = seq.next_element::<Column>()? {
                bytes += value.column.len() + value.path.len() * 33 + 64;
                if bytes > 16 * 1024 * 1024 || out.len() == 307200 {
                    return Err(A::Error::custom("query opening payload limit"));
                }
                out.push(value);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(Columns)
}
#[derive(Deserialize)]
enum WireOpening {
    TensorMerkle {
        #[serde(deserialize_with = "bytes")]
        row_combination: Vec<u8>,
        #[serde(deserialize_with = "columns")]
        columns: Vec<Column>,
    },
}
pub fn opening<'de, D: Deserializer<'de>>(d: D) -> Result<Opening, D::Error> {
    let WireOpening::TensorMerkle {
        row_combination,
        columns,
    } = WireOpening::deserialize(d)?;
    Ok(Opening::TensorMerkle {
        row_combination,
        columns: columns
            .into_iter()
            .map(|c| ColumnQuery {
                index: c.index,
                column: c.column,
                path: c.path,
            })
            .collect(),
    })
}
