//! Reader compatibility only; owning adapters enforce the promoted semantics.
use super::{Database, Table, Transaction};
use crate::storage::{StorageError, StorageResult};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReaderGeneration {
    Original,
    NeuronV1,
    AuthenticatedV1,
}
fn decode(status: Option<&[u8]>) -> StorageResult<ReaderGeneration> {
    match status {
        None | Some(b"complete") => Ok(ReaderGeneration::Original),
        Some(b"neuron-v1") => Ok(ReaderGeneration::NeuronV1),
        Some(b"auth-v1") => Ok(ReaderGeneration::AuthenticatedV1),
        _ => Err(StorageError::Corrupt(
            "unsupported reader generation or incomplete migration",
        )),
    }
}
pub(crate) fn promote(
    tx: &mut Transaction<'_>,
    required: ReaderGeneration,
) -> StorageResult<ReaderGeneration> {
    let current = decode(tx.get(Table::Migration, b"status", 16)?.as_deref())?;
    if current >= required {
        return Ok(current);
    }
    let marker: &[u8] = match required {
        ReaderGeneration::Original => return Ok(current),
        ReaderGeneration::NeuronV1 => b"neuron-v1",
        ReaderGeneration::AuthenticatedV1 => b"auth-v1",
    };
    tx.put(Table::Migration, b"status", marker)?;
    Ok(required)
}
impl Database {
    pub fn reader_generation(&self) -> StorageResult<ReaderGeneration> {
        decode(self.read(Table::Migration, b"status", 16)?.as_deref())
    }
    pub fn require_reader_generation(
        &self,
        required: ReaderGeneration,
    ) -> StorageResult<ReaderGeneration> {
        self.transaction::<_, StorageError>(|tx| promote(tx, required))
            .map(|commit| commit.value)
    }
}
#[cfg(all(test, feature = "backend-ssd"))]
mod tests {
    use super::*;
    #[test]
    fn generation_promotion_is_durable_monotonic_and_rejects_incomplete_states() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let temp = Directory(std::env::temp_dir().join(
            format!("bbg-generation-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()),
        ));
        std::fs::create_dir(&temp.0).unwrap();
        let path = temp.0.join("bbg");
        {
            let db = Database::open(&path, super::super::Backend::Ssd).unwrap();
            assert_eq!(db.reader_generation().unwrap(), ReaderGeneration::Original);
            db.require_reader_generation(ReaderGeneration::AuthenticatedV1)
                .unwrap();
            let before = db.last_transaction().unwrap();
            assert_eq!(
                db.require_reader_generation(ReaderGeneration::NeuronV1)
                    .unwrap(),
                ReaderGeneration::AuthenticatedV1
            );
            assert_eq!(db.last_transaction().unwrap(), before);
        }
        let db = Database::open(&path, super::super::Backend::Ssd).unwrap();
        assert_eq!(
            db.reader_generation().unwrap(),
            ReaderGeneration::AuthenticatedV1
        );
        for status in [b"copying".as_slice(), b"export-v1", b"future-v9"] {
            db.transaction::<_, StorageError>(|tx| tx.put(Table::Migration, b"status", status))
                .unwrap();
            assert!(
                db.require_reader_generation(ReaderGeneration::AuthenticatedV1)
                    .is_err()
            );
            assert_eq!(
                db.read(Table::Migration, b"status", 16).unwrap().as_deref(),
                Some(status)
            );
        }
    }
}
