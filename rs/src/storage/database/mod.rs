//! One physical owner and transaction boundary for all durable BBG views.
mod backend;
mod transaction;
pub use transaction::Transaction;

use super::{StorageError, StorageResult, decode_marker};
use backend::Engine;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_TRANSACTION_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TRANSACTION_KEYS: usize = 150_000;
pub const MAX_BYTES_VALUE: usize = 16 * 1024 * 1024 + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Ssd,
    Hdd,
}

#[derive(Debug)]
pub struct Commit<T> {
    pub value: T,
    /// None for a read-only transaction, including an already recorded retry.
    pub change_id: Option<[u8; 32]>,
}

#[derive(Clone)]
pub struct Database {
    shared: Arc<Shared>,
}
struct Shared {
    state: Mutex<State>,
    poisoned: AtomicBool,
}
struct State {
    engine: Engine,
    failure: Option<StorageError>,
}

impl Database {
    pub fn open(path: impl AsRef<Path>, backend: Backend) -> StorageResult<Self> {
        if migration_guard(path.as_ref())?
            .try_exists()
            .map_err(super::access::io)?
        {
            return Err(StorageError::Corrupt("incomplete application migration"));
        }
        let db = Self::open_unchecked(path.as_ref(), backend)?;
        Self::validate_open(db, path.as_ref())
    }

    fn validate_open(db: Self, path: &Path) -> StorageResult<Self> {
        // An import can begin while this opener waits for the filesystem lock.
        if migration_guard(path)?
            .try_exists()
            .map_err(super::access::io)?
        {
            return Err(StorageError::Corrupt("incomplete application migration"));
        }
        let migration = db.read(Table::Migration, b"status", 16)?;
        if migration.as_deref().is_some_and(|s| s != b"complete") {
            return Err(StorageError::Corrupt("incomplete application migration"));
        }
        db.last_transaction()?;
        db.read(Table::Metadata, b"last_commit", 32)?
            .map(|bytes| decode_marker(&bytes))
            .transpose()?;
        Ok(db)
    }

    pub(crate) fn open_unchecked(path: &Path, backend: Backend) -> StorageResult<Self> {
        Ok(Self::from_engine(Engine::open(path, backend)?))
    }

    fn from_engine(engine: Engine) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    engine,
                    failure: None,
                }),
                poisoned: AtomicBool::new(false),
            }),
        }
    }

    #[cfg(all(test, feature = "backend-hdd"))]
    pub(crate) fn from_redb(db: ::redb::Database) -> Self {
        Self::from_engine(Engine::from_redb(db))
    }

    /// The closure uses its Transaction exclusively; reentering another view of
    /// this owner while the closure runs would deadlock.
    pub fn transaction<T, E: From<StorageError>>(
        &self,
        execute: impl FnOnce(&mut Transaction<'_>) -> Result<T, E>,
    ) -> Result<Commit<T>, E> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| E::from(StorageError::Corrupt("panicked database transaction")))?;
        if let Some(error) = &state.failure {
            return Err(error.clone().into());
        }
        let mut tx = Transaction::new(&state.engine);
        let value = execute(&mut tx)?;
        let mut changes = tx.into_changes();
        if changes.is_empty() {
            return Ok(Commit {
                value,
                change_id: None,
            });
        }
        let id = change_id(&changes);
        if changes.keys().any(|(t, _)| matches!(t, Table::Shard(_))) {
            changes.insert(
                (Table::Metadata, b"last_commit".to_vec()),
                Some(id.to_vec()),
            );
        }
        changes.insert(
            (Table::Metadata, b"last_transaction".to_vec()),
            Some(id.to_vec()),
        );
        if let Err(error) = state.engine.commit(&changes, id) {
            if matches!(error, StorageError::CommitUnknown { .. }) {
                state.failure = Some(error.clone());
                self.shared.poisoned.store(true, Ordering::Release);
            }
            return Err(error.into());
        }
        Ok(Commit {
            value,
            change_id: Some(id),
        })
    }

    pub fn last_transaction(&self) -> StorageResult<Option<[u8; 32]>> {
        self.read(Table::Metadata, b"last_transaction", 32)?
            .map(|bytes| decode_marker(&bytes))
            .transpose()
    }

    pub fn is_poisoned(&self) -> bool {
        self.shared.poisoned.load(Ordering::Acquire) || self.shared.state.is_poisoned()
    }

    pub fn shards(&self) -> super::DiskStore {
        super::DiskStore::new(self.clone())
    }

    pub(crate) fn ready(&self) -> StorageResult<()> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| StorageError::Corrupt("panicked database transaction"))?;
        state.failure.clone().map_or(Ok(()), Err)
    }

    pub(crate) fn read(
        &self,
        table: Table,
        key: &[u8],
        max: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        self.transaction::<_, StorageError>(|tx| tx.get(table, key, max))
            .map(|c| c.value)
    }

    pub(crate) fn scan(
        &self,
        table: Table,
        after: Option<&[u8]>,
        prefix: &[u8],
        limits: ByteLimits,
    ) -> StorageResult<Vec<RawEntry>> {
        self.transaction::<_, StorageError>(|tx| tx.scan(table, after, prefix, limits))
            .map(|c| c.value)
    }
}

pub(crate) fn migration_guard(path: &Path) -> StorageResult<std::path::PathBuf> {
    // One physical destination must have one guard, including trailing slash,
    // dot/parent components and symlink aliases of an existing directory.
    let normalized = match std::fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or(StorageError::Unsupported("database path has no name"))?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            std::fs::canonicalize(parent)
                .map_err(super::access::io)?
                .join(name)
        }
        Err(error) => return Err(super::access::io(error)),
    };
    let mut guard = normalized.into_os_string();
    guard.push(".bbg-importing");
    Ok(guard.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Table {
    Shard(u8),
    Metadata,
    Content,
    Heads,
    History,
    Requests,
    Claims,
    Migration,
}
impl Table {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Shard(d) => SHARD_NAMES[d as usize],
            Self::Metadata => "bbg_storage_v1",
            Self::Content => "application_content",
            Self::Heads => "application_heads",
            Self::History => "application_history",
            Self::Requests => "application_requests",
            Self::Claims => "application_unique_claims",
            Self::Migration => "bbg_import_v1",
        }
    }
}
pub(crate) const SHARD_NAMES: [&str; 14] = [
    "particles",
    "axons_out",
    "axons_in",
    "neurons",
    "locations",
    "coins",
    "cards",
    "files",
    "time",
    "signals",
    "commitments",
    "nullifiers",
    "intents",
    "ephemeral",
];
#[cfg(all(feature = "backend-ssd", feature = "backend-hdd"))]
pub(crate) const APPLICATION_TABLES: [Table; 5] = [
    Table::Content,
    Table::Heads,
    Table::History,
    Table::Requests,
    Table::Claims,
];
pub(crate) type RawEntry = (Vec<u8>, Vec<u8>);
pub(crate) type Changes = BTreeMap<(Table, Vec<u8>), Option<Vec<u8>>>;
#[derive(Clone, Copy)]
pub(crate) struct ByteLimits {
    pub max_entries: usize,
    pub max_bytes: usize,
}
impl ByteLimits {
    pub(crate) fn validate(self) -> StorageResult<()> {
        if self.max_entries == 0
            || self.max_entries > 4096
            || self.max_bytes > MAX_TRANSACTION_BYTES
        {
            return Err(StorageError::Limit("byte scan budget"));
        }
        Ok(())
    }
}

pub(crate) fn change_id(changes: &Changes) -> [u8; 32] {
    let mut hash = hemera::Hasher::new();
    hash.update(b"bbg/database-batch/v1\0");
    let mut ordered: Vec<_> = changes.iter().collect();
    ordered.sort_unstable_by(|((a, ka), _), ((b, kb), _)| (a.name(), ka).cmp(&(b.name(), kb)));
    for ((table, key), value) in ordered {
        let name = table.name().as_bytes();
        hash.update(&(name.len() as u64).to_le_bytes());
        hash.update(name);
        hash.update(&(key.len() as u64).to_le_bytes());
        hash.update(key);
        hash.update(&[u8::from(value.is_some())]);
        if let Some(value) = value {
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value);
        }
    }
    *hash.finalize().as_bytes()
}

#[cfg(all(test, feature = "backend-ssd"))]
mod tests {
    use super::*;
    #[test]
    fn opener_rechecks_import_guard_after_acquiring_database_owner() {
        let root =
            std::env::temp_dir().join(format!("bbg-import-open-race-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("bbg");
        // The first guard check happened before this engine was acquired.
        let db = Database::open_unchecked(&path, Backend::Ssd).unwrap();
        std::fs::write(migration_guard(&path).unwrap(), b"").unwrap();
        assert!(matches!(
            Database::validate_open(db, &path),
            Err(StorageError::Corrupt("incomplete application migration"))
        ));
        std::fs::remove_file(migration_guard(&path).unwrap()).unwrap();
        let db = Database::open(&path, Backend::Ssd).unwrap();
        assert!(db.last_transaction().unwrap().is_none());
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
