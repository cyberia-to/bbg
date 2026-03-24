# bbg roadmap

only unfinished proposals remain here. executed proposals have moved to:
- **reference/** — the spec (WHAT and HOW)
- **docs/explanation/** — the rationale (WHY)

## remaining proposals

| proposal | in reference? | what's missing |
|----------|--------------|----------------|
| [[storage-proofs]] | **partial** → reference/storage.md has proof types table | per-node storage/size/replication proof circuits need full spec |
| [[verifiable-query]] | **partial** → reference/query.md has interface + cost model | query compiler algorithm (CozoDB → CCS) needs implementation detail |

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
