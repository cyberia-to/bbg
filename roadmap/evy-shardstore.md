---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: implemented
date: 2026-05-20
---
# evy shardstore

five additions to `ShardStore` that let it serve as the ECS storage substrate for evy. all decisions below are made — no open questions remain.

## context

evy routes all ECS component storage through bbg's `ShardStore` trait — transforms, animation phase, render buffers, and authenticated BBG polynomial state share one API. the four additions below are the only bbg changes required before `evy_ecs_storage` can be written. bbg has no knowledge of Bevy or entities; the trait stays at `(dim, key, Goldilocks)`.

## additions

### 1. ephemeral dimension

```rust
// storage/mod.rs — dim module
pub const EPHEMERAL: u8 = 12;
```

`put(EPHEMERAL, ...)` skips `self.dirty.push`. `get(EPHEMERAL, ...)` works normally. `commit()` ignores ephemeral entries. applies to all five backends: `mem`, `unimem`, `fjall`, `redb`, `tiered`. ~5 LOC per backend.

used for: Transform, AnimationPhase, VFXLifetime, RenderScratch — local-only state that must not gossip to the network or contribute to BBG_root.

### 2. mutating trait methods

add to `ShardStore`:

```rust
/// Returns mutable slice for in-place update.
/// Caller must call mark_dirty after writing (no-op for EPHEMERAL).
/// Returns None on backends that do not support in-place mutation (fjall, redb).
fn get_mut(&mut self, dimension: u8, key: &[u8; 32]) -> Option<&mut [Goldilocks]>;

/// Marks an entry dirty for the next commit(). Required after get_mut writes.
/// No-op for EPHEMERAL dimension.
fn mark_dirty(&mut self, dimension: u8, key: [u8; 32]);

/// Removes an entry. Returns the previous value, or None if absent.
/// Must clear the dirty log entry if present.
fn remove(&mut self, dimension: u8, key: &[u8; 32]) -> Option<Vec<Goldilocks>>;
```

`MemStore`: trivial HashMap ops.
`UnimemStore`: `get_mut` returns `&mut [Goldilocks]` over the pinned Block (mirrors existing `put` safety block); `remove` frees the Block and evicts from dirty.
`fjall`/`redb`: `get_mut` returns `None`; `remove` issues a delete.

`remove` is needed for Grid slot reuse with generation bumping. without it evy cannot reclaim unimem cells.

### 3. per-dimension iteration

add to `ShardStore`:

```rust
fn iter(&self, dimension: u8)
    -> Box<dyn Iterator<Item = (&[u8; 32], &[Goldilocks])> + '_>;
```

boxed to preserve trait object safety (`TieredStore` uses `dyn ShardStore`). concrete iterators require GATs and break object safety.

`MemStore`/`UnimemStore`: filter by dimension key prefix.
`fjall`/`redb`: prefix scan (dimension byte is the key prefix).
`TieredStore`: delegates to hot store; falls through to warm on miss.

### 4. UnimemStore slot pool

not a trait addition — `UnimemStore`-only:

```rust
impl UnimemStore {
    /// Pre-allocate one contiguous IOSurface for `dimension` sized for
    /// `expected_count` entries of `bytes_per_entry` each.
    /// Subsequent put() into this dimension allocates cells from the pool.
    pub fn reserve_pool(
        &mut self,
        dimension: u8,
        expected_count: usize,
        bytes_per_entry: usize,
    ) -> Result<(), unimem::Error>;

    /// Pool Block + cell index for direct GPU/ANE descriptor binding.
    /// Address is stable for the cell's lifetime (until remove()).
    pub fn cell(&self, dimension: u8, key: &[u8; 32]) -> Option<(&Block, usize)>;
}
```

`put()` checks if the dimension has a pool and routes to cell allocation instead of per-entry Block. `UnimemStore` tracks `pool_dims: HashMap<u8, Pool>` internally. ~80 LOC.

without this, per-entity Block allocation dominates render-frame cost on Apple Silicon.

## concurrency

`&mut self` exclusive borrow is kept. evy batches writes per-thread and merges at frame boundary. this preserves BBG's clean ownership model and requires no interior mutability.

## acceptance criteria

1. `dim::EPHEMERAL = 12` exists; all 5 backends skip ephemeral writes in `commit()`
2. `get_mut`, `mark_dirty`, `remove` on trait; implemented on `MemStore` and `UnimemStore`; `get_mut` returns `None` on `fjall`/`redb`; all documented
3. `iter(dim)` on all 5 backends, returns boxed iterator
4. `UnimemStore::reserve_pool` + `cell` implemented; pool cells have stable address until `remove()`
5. `cargo test --workspace --all-features` passes
6. `bbg/specs/storage.md` updated: EPHEMERAL dimension, new trait methods, `reserve_pool`/`cell`, concurrency decision

## scope

- session 1: EPHEMERAL across all backends + tests
- session 2: `get_mut` / `mark_dirty` / `remove` + `iter` on all backends + tests
- session 3: `reserve_pool` / `cell` on `UnimemStore` + benchmark vs per-entry Block + spec update
