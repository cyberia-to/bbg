---
tags: bbg, docs
crystal-type: entity
crystal-domain: cyber
---
# storage API

Reference for `bbg::storage::*`. The canonical contract is
[storage](../../specs/storage.md); implementation and failure-test evidence live
in the [durable shard storage audit](../../audit/durable-shard-storage.md).

`ShardStore` stores dimension/key/value shards. `TieredStore` composes a HOT cache
with optional WARM, COLD and network tiers. The `Bbg` facade still holds its graph
state in memory; connecting its state lifecycle to this interface remains an
integration task. [ApplicationStore](../../specs/application-storage.md) provides
a separate atomic API for local application history and receipts.

## ShardStore

```rust
pub type StorageResult<T> = Result<T, StorageError>;
pub type ShardEntry = ([u8; 32], Vec<Goldilocks>);

pub struct ScanLimits {
    pub max_entries: usize,
    pub max_elements: usize,
}

pub enum Durability { Memory, Disk }

pub trait ShardStore: Send + Sync {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]>;
    fn get_mut(&mut self, dimension: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]>;
    fn iter(&self, dimension: u8)
        -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_>;

    fn read(&self, dimension: u8, key: &[u8; 32], max_elements: usize)
        -> StorageResult<Option<Vec<Goldilocks>>>;
    fn scan(&self, dimension: u8, after: Option<[u8; 32]>, limits: ScanLimits)
        -> StorageResult<Vec<ShardEntry>>;

    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>)
        -> StorageResult<()>;
    fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]) -> StorageResult<()>;
    fn remove(&mut self, dimension: u8, key: &[u8; 32])
        -> StorageResult<Option<Vec<Goldilocks>>>;
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)];
    fn has_pending(&self) -> bool;

    fn commit(&mut self) -> StorageResult<[u8; 32]>;
    fn last_commit(&self) -> StorageResult<Option<[u8; 32]>>;
    fn durability(&self) -> Durability;
    fn is_poisoned(&self) -> bool;
}
```

### Reads and bounds

`get` and `iter` expose borrowed cache contents. A cache miss says nothing about
disk absence or read failure. Use `read` for an owned value: `Ok(None)` means
absent, and errors remain explicit. Disk `read` sees staged puts and deletions
before consulting committed data. Successful disk commit clears persistent
cache entries; reopening starts with an empty cache.

Disk caches are read-only through the borrowed interface: their `get_mut`
always returns `None`. On MemStore and TieredStore HOT, in-place mutation must
be followed by a successful `mark_dirty`; TieredStore stages that value in WARM.
For disk replacement, use `put`.

`scan` returns keys in ascending byte order, strictly after its optional cursor.
Pass `None` for the first page and the last returned key for the next page;
stop on an empty page. On disk it scans committed state and returns
`PendingWrites` while any persistent put or delete is pending. MemStore scans
its current in-memory state.

| Bound | Limit |
|---|---:|
| Elements in one value or owned read (`MAX_VALUE_ELEMENTS`) | 131,072 |
| Elements across one returned scan page | caller budget, at most 131,072 |
| Entries in one scan page | caller budget, 1–4,096 |
| Distinct pending persistent keys | 65,536 |
| Elements across pending persistent puts | 2,097,152 |

Repeated writes to one key coalesce. The element budget counts Goldilocks
elements. A row larger than the whole page budget returns `Limit`; a row that
fits a fresh page but exceeds the remaining budget starts the next page.
Decoding checks the requested bound before allocating the output vector.
The database engine may already have materialized the encoded record.

### Writes, commit and recovery

`put`, `mark_dirty`, `remove` and `commit` return `StorageResult`. `remove`
returns the previous owned value and stages a deletion; successful staging
does not acknowledge durability. `dirty_entries` exposes coalesced pending
puts. Use `has_pending` to include deletions.

Fjall and redb commit all persistent updates, deletions and the `last_commit`
marker in one backend transaction. Pending data is retained on failure and
cleared only after successful commit. The returned 32-byte change identity
binds dimensions, keys, operation kinds and values in canonical order under
the `bbg/shard-batch/v1` domain. Authenticated graph roots are computed by BBG's
commitment layer; request receipts belong to the application or native history
transaction above this API.

`CommitUnknown { change_id, message }` freezes the handle. Keep that identity,
stop dependent publication, drop the handle and reopen the same store.
Compare `last_commit()` with the unresolved identity before proceeding. The
marker resolves the latest unresolved batch under the exclusive-writer
contract; it does not retain a history of request receipts. Reopened marker
and data reads can themselves fail and must be propagated.

An empty disk commit returns the existing marker when present. New or legacy
stores without a marker return `None` from `last_commit`; their first nonempty
successful commit installs it. MemStore reports `Durability::Memory`, returns
a change identity and retains its data in RAM. Its commit supplies no disk
durability and its `last_commit` is `None`.

```rust
pub enum StorageError {
    InvalidDimension(u8),
    Limit(&'static str),
    Corrupt(&'static str),
    Io(String),
    Busy,
    PendingWrites,
    Unsupported(&'static str),
    CommitUnknown { change_id: [u8; 32], message: String },
}
```

Invalid input rejected before staging leaves the store usable. A rejected
dimension or oversized `put` does not poison a healthy TieredStore. Failures
after a tier has staged a change, or during tier commit, can freeze the wrapper
to preserve an explicit recovery boundary. Borrowed cache methods cannot
report errors; fallible reads are required when storage health matters.

### Dimensions

Constants are in `bbg::storage::dim`.

| Constant | Value |
|---|---:|
| `PARTICLES` | 0 |
| `AXONS_OUT` | 1 |
| `AXONS_IN` | 2 |
| `NEURONS` | 3 |
| `LOCATIONS` | 4 |
| `COINS` | 5 |
| `CARDS` | 6 |
| `FILES` | 7 |
| `TIME` | 8 |
| `SIGNALS` | 9 |
| `COMMITMENTS` | 10 |
| `NULLIFIERS` | 11 |
| `INTENTS` | 12 |
| `EPHEMERAL` | 13 |

EPHEMERAL values stay in the current process's memory, including on disk
backends. They are excluded from persistent batches and disappear on reopen.

## TieredStore

```rust
impl TieredStore {
    pub fn new(hot: Box<dyn ShardStore>) -> Self;
    pub fn with_warm(self, warm: Box<dyn ShardStore>) -> StorageResult<Self>;
    pub fn with_cold(self, cold: Box<dyn ShardStore>) -> Self;
    pub fn with_network(self, net: Box<dyn NetworkStore>) -> Self;
    pub fn promote(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<bool>;
    pub fn evict(&mut self, dimension: u8, key: &[u8; 32]) -> StorageResult<()>;
    pub fn archive(&mut self) -> StorageResult<Option<[u8; 32]>>;
    pub fn fetch_content(&self, particle: &Particle) -> Option<Vec<u8>>;
}
```

Construction with `backend-ssd` enabled and an existing `data` directory:

```rust
use bbg::storage::{FjallStore, MemStore, TieredStore};

let store = TieredStore::new(Box::new(MemStore::new()))
    .with_warm(Box::new(FjallStore::open("data/warm")?))?;
```

Disk WARM requires HOT to report `Durability::Memory`. Attach it before
populating persistent HOT entries or staging persistent changes.
`with_warm` rejects prepopulated or dirty persistent HOT and rejects replacing
an already attached WARM. Import existing values explicitly through the
combined store after attachment. `TieredStore::default()` is memory-only.

| Operation | Routing and outcome |
|---|---|
| Persistent `put` / `remove` | Stage WARM first, then HOT; propagate errors |
| Owned `read` | Use cached HOT while WARM is unpoisoned; otherwise query WARM. WARM absence is authoritative and never falls through to an old COLD value |
| `scan` | Scan authoritative WARM when attached; cache-only HOT promotions do not change that committed view |
| `commit` | Commit WARM before HOT; return WARM's identity when attached |
| `promote` | Read through the fallible interface and populate HOT; return whether found |
| `evict` | Remove HOT only if WARM has no pending changes and already contains the same value; preserve HOT otherwise |
| `archive` | Commit entries already staged in COLD; return `None` when COLD is absent |

If disk WARM commits but HOT publication fails, commit returns `CommitUnknown`
with the durable WARM identity and freezes the combined handle. If WARM commit
fails, HOT remains pending and the wrapper blocks further publication.
EPHEMERAL routes only to HOT and is never evicted by `evict`.

Without WARM, point reads can fall through HOT to COLD. Mixed HOT/COLD scans
and persistent deletion without authoritative WARM return `Unsupported`.
Archive population and restartable archive progress still require an adapter;
`archive()` supplies neither a copy operation nor a transaction across tiers.

## Backends

| Backend | Availability | Storage |
|---|---|---|
| `MemStore::new()` | always | ordered in-memory cache and bounded, coalesced pending changes |
| `FjallStore::open(path)` | `backend-ssd` | Fjall 2.11.2, dimension partitions, atomic `SyncAll` batch |
| `RedbStore::open(path)` | `backend-hdd` | redb 2.x, dimension tables, one `Immediate` write transaction |

Both disk constructors return `StorageResult<Self>`. Fjall accepts
`impl Into<PathBuf>`; redb accepts `impl AsRef<Path>`. Their parent directory must
already exist. Fjall creates its store directory and holds an exclusive
`bbg.lock` file lock; redb holds the database's exclusive writer lock. A second
writer receives `Busy`. Both sync the parent directory on opening.

Both disk stores provide
`load(dimension, key) -> StorageResult<Option<Vec<Goldilocks>>>` for committed
disk data with the maximum value bound. It bypasses pending changes. Prefer
`read` for an explicit caller budget and staged-write visibility.

BBG vendors the Fjall 2.11.2 batch error-path repair documented in the
[Fjall audit](../../audit/fjall-batch-write.md). The batch API stops before
memtable publication and poisons the keyspace on a journal write failure.

`UnimemStore` source remains guarded by macOS and `backend-unimem`; that feature
and its optional dependency are not wired into BBG's manifest. The tested
storage profile uses MemStore HOT with Fjall or redb WARM.

## Encoding

Disk keys are exactly 32 bytes. Each field element is encoded as an 8-byte
little-endian canonical Goldilocks integer. Decode rejects invalid byte lengths,
limbs greater than or equal to the field modulus, oversized values and malformed
scan keys. The private `bbg_storage_v1` metadata partition/table contains the
32-byte `last_commit` marker. Existing shard names and value encoding are
retained; databases without this metadata can be opened.

## NetworkStore

```rust
pub trait NetworkStore: Send + Sync {
    fn fetch(&self, particle: &Particle) -> Option<Vec<u8>>;
    fn das_sample(&self, particle: &Particle, offset: u64, length: u64)
        -> Option<QueryProof>;
}
```

Cybergraph injects content retrieval; BBG does not own transport.
`fetch_content` explicitly delegates to `fetch`. Network retrieval is separate
from local shard transactions and their disk commit acknowledgement.
