---
tags: bbg, docs
crystal-type: entity
crystal-domain: cyber
---
# storage API

Reference for `bbg::storage`. All types are in `bbg::storage::*`.

## overview

BBG storage is split across four tiers that run simultaneously. `TieredStore`
composes them into a single `ShardStore` interface. Feature flags control which
backend types are compiled in; tier occupancy is configured at runtime.

```
TieredStore
  │
  ├── hot:     Box<dyn ShardStore>          L1 — always present
  ├── warm:    Option<Box<dyn ShardStore>>  L2 — optional
  ├── cold:    Option<Box<dyn ShardStore>>  L4 — optional
  └── network: Option<Box<dyn NetworkStore>> L3 — injected by cybergraph
```

---

## ShardStore

```rust
pub trait ShardStore: Send + Sync {
    fn get(&self, dimension: u8, key: &[u8; 32]) -> Option<&[Goldilocks]>;
    fn put(&mut self, dimension: u8, key: [u8; 32], value: Vec<Goldilocks>);
    fn dirty_entries(&self) -> &[(u8, [u8; 32], Vec<Goldilocks>)];
    fn commit(&mut self) -> [u8; 32];
}
```

Local storage interface for polynomial evaluation shards. Authentication is
provided by the Lens commitment layer; the store has no opinion on correctness.

| method | description |
|---|---|
| `get(dim, key)` | Return the Goldilocks slice for this (dimension, key) pair, or None |
| `put(dim, key, value)` | Write a field-element slice; marks the entry dirty |
| `dirty_entries()` | All entries written since last `commit()` |
| `commit()` | Flush dirty entries; return a 32-byte shard sub-root (hemera hash of dirty key list) |

`commit()` sub-root is for change-set tracking only. The authoritative
polynomial commitment is computed by `dim::commit_dim` in `bbg::dim`.

**dimension constants** — `bbg::storage::dim`:

| constant | value | polynomial |
|---|---|---|
| `PARTICLES` | 0 | BBG_poly public |
| `AXONS_OUT` | 1 | BBG_poly public |
| `AXONS_IN` | 2 | BBG_poly public |
| `NEURONS` | 3 | BBG_poly public |
| `LOCATIONS` | 4 | BBG_poly public |
| `COINS` | 5 | BBG_poly public |
| `CARDS` | 6 | BBG_poly public |
| `FILES` | 7 | BBG_poly public |
| `TIME` | 8 | BBG_poly public |
| `SIGNALS` | 9 | BBG_poly public |
| `COMMITMENTS` | 10 | A(x) private |
| `NULLIFIERS` | 11 | N(x) private |

---

## NetworkStore

```rust
pub trait NetworkStore: Send + Sync {
    fn fetch(&self, particle: &Cid) -> Option<Vec<u8>>;
    fn das_sample(&self, particle: &Cid, offset: u64, length: u64) -> Option<QueryProof>;
}
```

L3 content retrieval. Injected by cybergraph at init; BBG does not own transport.

| method | description |
|---|---|
| `fetch(particle)` | Retrieve raw particle content bytes by particle. None if unreachable |
| `das_sample(particle, offset, length)` | Open a Lens proof for a byte range (DAS challenge/response) |

---

## TieredStore

```rust
pub struct TieredStore { /* private */ }
```

Composes all tiers into a single `ShardStore`. Routing:

- **write** — HOT always; write-through to WARM for durability
- **read** — HOT → WARM → COLD cascade; first hit wins
- **commit** — flushes HOT + WARM per block; COLD via `archive()` at checkpoints
- **promote / evict** — explicit, driven by soma focus signals

### construction

```rust
// memory-only (default, light clients and tests)
let store = TieredStore::default();

// full node on Apple Silicon
let store = TieredStore::new(Box::new(UnimemStore::new()))  // HOT
    .with_warm(Box::new(FjallStore::open("data/warm")?))    // WARM
    .with_cold(Box::new(RedbStore::open("data/cold")?))     // COLD
    .with_network(Box::new(my_network_impl));               // L3

// x86 validator
let store = TieredStore::new(Box::new(MemStore::new()))
    .with_warm(Box::new(FjallStore::open("data/warm")?))
    .with_cold(Box::new(RedbStore::open("data/cold")?));
```

### methods

```rust
impl TieredStore {
    pub fn new(hot: Box<dyn ShardStore>) -> Self
    pub fn with_warm(self, warm: Box<dyn ShardStore>) -> Self
    pub fn with_cold(self, cold: Box<dyn ShardStore>) -> Self
    pub fn with_network(self, net: Box<dyn NetworkStore>) -> Self

    pub fn promote(&mut self, dimension: u8, key: &[u8; 32]) -> bool
    pub fn evict(&mut self, dimension: u8, key: &[u8; 32])
    pub fn fetch_content(&self, particle: &Cid) -> Option<Vec<u8>>
    pub fn archive(&mut self) -> Option<[u8; 32]>
}
```

| method | description |
|---|---|
| `new(hot)` | Create with a HOT backend. WARM, COLD, NETWORK are absent until added |
| `with_warm(warm)` | Add WARM tier (SSD, durability). Builder pattern, consumes self |
| `with_cold(cold)` | Add COLD tier (HDD, archival). Builder pattern, consumes self |
| `with_network(net)` | Inject L3 content fetcher. Builder pattern, consumes self |
| `promote(dim, key)` | Pull (dim, key) from WARM or COLD into HOT. Returns true if found. Called by soma prefetch |
| `evict(dim, key)` | Move HOT entry to WARM. Called by soma when focus drops below threshold |
| `fetch_content(particle)` | Delegate to `NetworkStore::fetch`. None if no network tier or unreachable |
| `archive()` | Commit COLD tier. Returns COLD sub-root, or None if no COLD tier present |

`TieredStore` itself implements `ShardStore`. The per-block flow:

```
block arrives →
  bbg.insert(signal)              // put() into TieredStore → HOT + WARM
  bbg.finalize_block()            // TieredStore.commit() → flush HOT + WARM
  if epoch boundary:
    TieredStore.archive()         // flush COLD (archival checkpoints only)
```

---

## backends

### MemStore  `bbg::storage::mem`

```rust
pub struct MemStore { /* HashMap */ }
impl MemStore { pub fn new() -> Self }
impl Default for MemStore { ... }
impl ShardStore for MemStore { ... }
```

In-memory HashMap. Always available, no feature flag. 50 ns reads.
Used as HOT tier on x86 or when unimem is unavailable.

---

### UnimemStore  `bbg::storage::unimem`

```rust
#[cfg(all(target_os = "macos", feature = "backend-unimem"))]
pub struct UnimemStore { /* IOSurface Blocks */ }

impl UnimemStore {
    pub fn new() -> Self
    pub fn block(&self, dimension: u8, key: &[u8; 32]) -> Option<&Block>
}
```

IOSurface-backed pinned Blocks. Each value lives in its own Block whose
physical pages are visible without copying to CPU, AMX, Metal GPU, and ANE.

`get()` returns a `&[Goldilocks]` backed directly by pinned IOSurface memory —
no copy at any stage.

`block(dim, key)` — return the raw `Block` for direct binding to
`aruminium` (Metal GPU) or `rane` (ANE). None if the entry fell back to heap.

Falls back to heap `Vec<Goldilocks>` if `Block::open` fails (non-Apple-Silicon
CI environments). Apple Silicon only (`cfg(target_os = "macos")`).

**feature**: `backend-unimem`

---

### FjallStore  `bbg::storage::fjall`

```rust
#[cfg(feature = "backend-ssd")]
pub struct FjallStore { /* fjall Keyspace, 12 partitions, write-through cache */ }

impl FjallStore {
    pub fn open(path: impl Into<PathBuf>) -> fjall::Result<Self>
    pub fn load(&self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>>
}
```

LSM-tree backed by `fjall` 2.x. One partition per dimension. 20 μs reads.

Write-through: every `put` lands in both the in-memory cache and fjall.
`get` reads from cache. Cold reads after reopen use `load(dim, key)`.

`commit()` calls `keyspace.persist(PersistMode::SyncAll)` — all dirty entries
are durable after commit returns.

**feature**: `backend-ssd`

---

### RedbStore  `bbg::storage::redb`

```rust
#[cfg(feature = "backend-hdd")]
pub struct RedbStore { /* redb Database, 12 tables, write-through cache */ }

impl RedbStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, redb::DatabaseError>
    pub fn load(&self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>>
}
```

B-tree MVCC backed by `redb` 2.x. One table per dimension. Sequential 200 MB/s.

Write-through: every `put` lands in both the in-memory cache and redb.
Dirty entries are grouped by dimension at commit time so each redb table is
opened exactly once per write transaction. `load(dim, key)` for cold reads.

**feature**: `backend-hdd`

---

## serialisation

Field elements are stored on disk as 8-byte little-endian u64 (one per
`Goldilocks` element). This matches the canonical Goldilocks wire format used
throughout BBG.

```
Goldilocks(v) → v.as_u64().to_le_bytes()   // 8 bytes
bytes[0..8]   → Goldilocks::new(u64::from_le_bytes(b))
```

Helpers (not public, used internally by FjallStore and RedbStore):
`storage::serialize_goldilocks`, `storage::deserialize_goldilocks`.
