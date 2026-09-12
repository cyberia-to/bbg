//! Closed native record spaces sharing the physical BBG transaction.
use super::{ByteLimits, Database, RawEntry, Table, Transaction};
use crate::storage::StorageResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecordDomain {
    NativeState,
    NativeHistory,
    NativeRequests,
    NativeMetadata,
    NativeBalances,
    NativeBlocks,
    NativeExport,
}

impl RecordDomain {
    #[cfg(feature = "backend-ssd")]
    pub(super) const ALL: [Self; 7] = [
        Self::NativeState,
        Self::NativeHistory,
        Self::NativeRequests,
        Self::NativeMetadata,
        Self::NativeBalances,
        Self::NativeBlocks,
        Self::NativeExport,
    ];
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::NativeState => "native_state_v1",
            Self::NativeHistory => "native_history_v1",
            Self::NativeRequests => "native_requests_v1",
            Self::NativeMetadata => "native_metadata_v1",
            Self::NativeBalances => "native_balances_v1",
            Self::NativeBlocks => "native_blocks_v1",
            Self::NativeExport => "native_export_v1",
        }
    }
}

#[derive(Clone, Copy)]
pub struct RecordLimits {
    pub max_entries: usize,
    pub max_bytes: usize,
}

impl Database {
    pub fn read_record(
        &self,
        domain: RecordDomain,
        key: &[u8],
        max: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        self.read(Table::Native(domain), key, max)
    }

    pub fn scan_records(
        &self,
        domain: RecordDomain,
        after: Option<&[u8]>,
        prefix: &[u8],
        limits: RecordLimits,
    ) -> StorageResult<Vec<RawEntry>> {
        self.scan(
            Table::Native(domain),
            after,
            prefix,
            ByteLimits {
                max_entries: limits.max_entries,
                max_bytes: limits.max_bytes,
            },
        )
    }
}

impl Transaction<'_> {
    pub fn read_record(
        &self,
        domain: RecordDomain,
        key: &[u8],
        max: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        self.get(Table::Native(domain), key, max)
    }

    pub fn put_record(
        &mut self,
        domain: RecordDomain,
        key: &[u8],
        value: &[u8],
    ) -> StorageResult<()> {
        self.put(Table::Native(domain), key, value)
    }

    pub fn remove_record(&mut self, domain: RecordDomain, key: &[u8]) -> StorageResult<()> {
        self.remove(Table::Native(domain), key)
    }

    pub fn scan_records(
        &self,
        domain: RecordDomain,
        after: Option<&[u8]>,
        prefix: &[u8],
        limits: RecordLimits,
    ) -> StorageResult<Vec<RawEntry>> {
        self.scan(
            Table::Native(domain),
            after,
            prefix,
            ByteLimits {
                max_entries: limits.max_entries,
                max_bytes: limits.max_bytes,
            },
        )
    }
}
