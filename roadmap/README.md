# bbg roadmap

only unfinished proposals remain here. executed proposals have moved to:
- **reference/** — the spec (WHAT and HOW)
- **docs/explanation/** — the rationale (WHY)

## remaining proposals

| proposal | in reference? | what's missing |
|----------|--------------|----------------|
| [[storage-proofs]] | **partial** → reference/storage.md has proof types table | per-node storage/size/replication proof circuits need full spec |
| [[verifiable-query]] | **partial** → reference/query.md has interface + cost model | query compiler algorithm (CozoDB → CCS) needs implementation detail |
| [[provable-reads]] | **no** | the `look` primitive opens an MLE fingerprint, not the record cell — unsound placeholder, zero production consumers. needs: value-soundness fix (corner opening + position binding); then split — point reads → record-opening (Option B), set queries → set-level proofs (Option D, the [[verifiable-query]] compiler). prerequisite for inf R2b. has a full options/pros-cons comparison table |
| [[evy-shardstore]] | **partial** → reference/storage.md defines `ShardStore` trait | 5 additions to enable ECS storage substrate for [[evy/specs/evy]]: EPHEMERAL dimension, `get_mut`+`mark_dirty`+`remove`, `iter(dim)`, `UnimemStore::reserve_pool` |

## executed (now in reference + explanation)

| former proposal | reference | explanation |
|---|---|---|
| algebraic-nmt | indexes.md, state.md, architecture.md | why-polynomial-state.md |
| unified-polynomial-state | state.md, architecture.md | why-polynomial-state.md |
| mutator-set-polynomial | privacy.md | polynomial-privacy.md |
| signal-first | sync.md, storage.md | why-signal-first.md |
| algebraic-das | data-availability.md | data-availability.md |
| full-pipeline | architecture.md (pipeline section) | architecture-overview.md |
| temporal-polynomial | temporal.md | (absorbed into reference) |
| pi-weighted-replication | storage.md | (absorbed into reference) |
