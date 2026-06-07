---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# committed graph statistics

Four statistics, committed in `BBG_root`, form the bbg → [[inf]] cost/recursion interface. Because they live under the root, inf's cost model and recursion bounds rest on proven values rather than trusted inputs.

## the four statistics

| field | type | meaning | exactness |
|---|---|---|---|
| `node_count` | u64 | number of particles (graph nodes) | exact |
| `relation_sizes` | [u64; 11] | entry count per committed relation | exact |
| `max_degree` | u64 | max out/in adjacency-list length | exact |
| `diameter_bound` | u64 | sound upper bound on graph diameter | upper bound |

`relation_sizes` is indexed in the canonical order folded into `compute_root`: particles, axons_out, axons_in, neurons, locations, coins, cards, files, time, signals, balances.

## the commitment

```
stats_commit = H(node_count ‖ relation_sizes[0..11] ‖ max_degree ‖ diameter_bound)
             (each value little-endian u64; H = hemera)

BBG_root = H( commit(BBG_poly) ‖ commit(A) ‖ commit(N) ‖ stats_commit )
```

Serialization is canonical (single LE encoding per value), so the commitment is deterministic and cross-platform.

## why committed

inf computes query cost and recursion bounds as:

```
inf cost = reads (bbg read_cost) + combine (inf coefficients) + recursion (bound × per-iter)
```

The recursion `bound` consumes `diameter_bound`; the read and combine terms consume `relation_sizes`, `node_count`, `max_degree`. Every term is either a fixed inf coefficient or a committed bbg statistic, so the total cost is static once the root is fixed.

If the statistics were a trusted side input, the recursion bound (and therefore termination and cost) would be unproven. Committing them in the root makes the same values that *bound* recursion also *cost* it — one interface serves both Q1 (bounded iteration) and Q2 (static cost).

## exactness and the diameter bound

`node_count`, `relation_sizes`, and `max_degree` are computed exactly from the committed BTreeMaps at `compute_root` time — no estimation.

`diameter_bound` is the one non-local statistic. bbg commits a sound upper bound:

- default: the trivial connected-graph worst case `node_count − 1`.
- installed: a tighter bound from [[tru]] via `set_diameter_bound`, proven by tru's convergence computation.

Either way the committed value is ≥ the true diameter, so any inf recursion bounded by it terminates with a complete result. A looser bound costs more iterations; it is never unsound.

## ownership split

| concern | owner |
|---|---|
| `read_cost(op)` — Lens-opening costs | bbg ([[bbg/specs/query]]) |
| `statistics(root)` — the four values above | bbg (this spec) |
| relational-operator coefficients | inf |
| query plan + bounded iteration count | inf |
| tighter `diameter_bound` (proven) | tru → installed in bbg |

## interface

```rust
// bbg
impl Bbg {
    fn statistics(&self) -> GraphStats;      // read by inf; committed in root
    fn set_diameter_bound(&mut self, b: u64); // tru installs a proven bound
}

struct GraphStats {
    node_count: u64,
    relation_sizes: [u64; 11],
    max_degree: u64,
    diameter_bound: u64,
}
```

See [[bbg/specs/query]] for `read_cost` and the Lens-opening proof types, [[inf/cost]] for how inf consumes these statistics.
