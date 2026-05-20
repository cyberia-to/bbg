---
tags: bbg, specs
crystal-type: entity
crystal-domain: cyber
---
# pruning

three mechanisms that compose to keep BBG state bounded, useful, and semantically honest.

## design principles

records are pure data. pruning policy is separate from records — no decay parameters,
timestamps, or ttl fields in NeuronRecord, ParticleRecord, or any BBG dimension.

policy lives in `PruneConfig` (parameters) and `PruneState` (runtime bookkeeping).
neither is part of BBG_poly or the state root — pruning is local node behavior, not consensus state.

the three mechanisms apply in priority order:

```
1. rank floor   — absolute protection for high-value content
2. half-life    — time-based candidacy for stale content
3. gravity      — density-based selection under size pressure
```

---

## mechanism 1: rank floor

entries whose φ* exceeds the rank floor are never pruned, regardless of staleness or size pressure.

```
rank_floor = φ* at the (100 − rank_floor_pct) percentile of all particles

protected:  particle.pi_star ≥ rank_floor
eligible:   particle.pi_star <  rank_floor
```

`rank_floor_pct = 20` protects the top 20% of particles by φ*. the floor is recomputed
each epoch from the live distribution — it adapts automatically as the graph grows or shrinks.

---

## mechanism 2: half-life

an edge becomes stale when no signal has touched it within `half_life_epochs`.

```
stale:  current_epoch − last_signal_epoch > half_life_epochs
fresh:  current_epoch − last_signal_epoch ≤ half_life_epochs
```

stale, unprotected edges are candidates for pruning. re-signaling an edge — even at
amount = 0 — resets it to fresh. the cost of keeping an edge alive is focus: the
economic signal that a neuron still cares.

`last_signal_epoch` is tracked per-axon in `PruneState.last_touched`, not in BBG_poly.

---

## mechanism 3: gravity

```
density(axon) ≈ weight / storage_bytes
```

gravity determines which candidates are pruned first: lowest density goes first.
it also decides when size pressure overrides the half-life gate.

```
size ≤ max_bytes:  prune only stale candidates, ordered by density ascending
size >  max_bytes:  extend to ALL unprotected entries, ordered by density ascending,
                    until size ≤ max_bytes
```

gravity is the pressure valve. under normal conditions only stale low-density edges go.
under storage pressure the market overrides half-life — even fresh low-density edges compete.

---

## pruning cycle

called at every epoch boundary, after the `time[height]` snapshot is written:

```
prune(bbg_state, prune_state, config, epoch):

  1. rank_floor  ← percentile(particles.pi_star, config.rank_floor_pct)
  2. over_budget ← estimate_bytes(bbg_state) > config.max_bytes

  3. if not over_budget:
       candidates ← axons where pi_star < rank_floor
                               AND epoch − last_touched > half_life_epochs
     else:
       candidates ← ALL axons where pi_star < rank_floor

  4. sort candidates by weight ascending  (low weight = low density = prune first)

  5. for each candidate:
       if over_budget AND estimate_bytes ≤ max_bytes: stop
       remove axon from particles, axons_out, axons_in, axon_edges, last_touched
```

---

## config

```
PruneConfig:
  max_bytes:        u64   — BBG storage budget         (default: 1 GiB)
  rank_floor_pct:   u8    — top-N% protection floor    (default: 20)
  half_life_epochs: u64   — staleness window in epochs (default: 1000)
```

---

## state (policy layer, outside BBG_poly)

```
PruneState:
  last_touched: Map<AxonId, epoch>
```

updated on every successful `insert(signal)` — once per axon touched by the signal.
never committed to BBG_poly, never included in state root computation.

---

## size estimate

```
estimate_bytes(state):
  particles:  len × 80    (32-byte key + 6 × u64 fields)
  axons_out:  Σ (32 + n × 32)  per entry, n = list length
  axons_in:   Σ (32 + n × 32)  per entry
  neurons:    len × 56    (32-byte key + 3 × u64 fields)
```

the estimate is monotonically proportional to real state size. it understates by
ignoring time, signals, coins, cards, files, and balances dimensions — those are
smaller and grow more slowly than the graph dimensions.

---

## what this replaces

the previous `apply_decay_and_prune()` applied continuous mathematical decay
(`weight × 99/100` per epoch) with a fixed prune floor. problems with that model:

- no rank protection — high-value content could be pruned
- no staleness concept — active and abandoned edges treated identically
- no size pressure response — no connection between pruning and storage budget
- hardcoded constants — not configurable per deployment

see [[state]] for the overall state transition model, [[architecture]] for evaluation dimensions.
