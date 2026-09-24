//! Fallible, bounded storage access and commit outcomes.

use nebu::Goldilocks;

pub const MAX_VALUE_ELEMENTS: usize = 131_072;
pub const MAX_PENDING_ELEMENTS: usize = 2_097_152;
pub const MAX_PENDING_KEYS: usize = 65_536;
pub const MAX_SCAN_ENTRIES: usize = 4096;
pub type ShardEntry = ([u8; 32], Vec<Goldilocks>);
pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    InvalidDimension(u8),
    Limit(&'static str),
    Corrupt(&'static str),
    Io(String),
    Busy,
    PendingWrites,
    Unsupported(&'static str),
    CommitUnknown {
        change_id: [u8; 32],
        message: String,
    },
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimension(d) => write!(f, "invalid storage dimension {d}"),
            Self::Limit(s) => write!(f, "storage limit: {s}"),
            Self::Corrupt(s) => write!(f, "corrupt storage: {s}"),
            Self::Io(s) => write!(f, "storage I/O: {s}"),
            Self::Busy => write!(f, "storage already has an exclusive writer"),
            Self::PendingWrites => write!(f, "commit pending writes before scanning disk"),
            Self::Unsupported(s) => write!(f, "unsupported storage operation: {s}"),
            Self::CommitUnknown { message, .. } => {
                write!(f, "unknown commit outcome; reopen and resolve: {message}")
            }
        }
    }
}
impl std::error::Error for StorageError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    Memory,
    Disk,
}

#[derive(Debug, Clone, Copy)]
pub struct ScanLimits {
    pub max_entries: usize,
    pub max_elements: usize,
}

impl ScanLimits {
    pub(crate) fn validate(self) -> StorageResult<()> {
        if self.max_entries == 0 || self.max_entries > MAX_SCAN_ENTRIES {
            return Err(StorageError::Limit("scan entry count"));
        }
        check_read_limit(self.max_elements)
    }
}

pub(crate) fn check_dimension(dimension: u8) -> StorageResult<()> {
    if dimension > super::dim::EPHEMERAL {
        return Err(StorageError::InvalidDimension(dimension));
    }
    Ok(())
}

pub(crate) fn check_read_limit(max_elements: usize) -> StorageResult<()> {
    if max_elements > MAX_VALUE_ELEMENTS {
        return Err(StorageError::Limit("read element count"));
    }
    Ok(())
}

pub(crate) fn copy_value(
    value: &[Goldilocks],
    max_elements: usize,
) -> StorageResult<Vec<Goldilocks>> {
    check_read_limit(max_elements)?;
    if value.len() > max_elements {
        return Err(StorageError::Limit("value exceeds read budget"));
    }
    Ok(value.to_vec())
}

#[cfg(any(feature = "backend-ssd", feature = "backend-hdd"))]
pub(crate) fn io(error: impl std::fmt::Display) -> StorageError {
    StorageError::Io(error.to_string())
}

/// Limit iteration before decoding or retaining an unbounded page.
pub(crate) fn collect_page(
    entries: impl Iterator<Item = StorageResult<ShardEntry>>,
    limits: ScanLimits,
) -> StorageResult<Vec<ShardEntry>> {
    limits.validate()?;
    let mut page = Vec::new();
    let mut elements = 0;
    for entry in entries.take(limits.max_entries) {
        let (key, value) = entry?;
        if value.len() > limits.max_elements {
            return Err(StorageError::Limit("scan value exceeds page budget"));
        }
        if value.len() > limits.max_elements - elements {
            break;
        }
        elements += value.len();
        page.push((key, value));
    }
    Ok(page)
}
