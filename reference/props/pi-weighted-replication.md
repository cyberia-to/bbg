---
tags: cyber, cip
crystal-type: process
crystal-domain: cyber
status: draft
date: 2026-03-18
diffusion: 0.00010722364868599256
springs: 0.00007019991600688145
heat: 0.00003419142694206788
focus: 0.00008151008453347325
gravity: 0
density: 0
---
# π-weighted replication

storage replication factor proportional to π (cyberank). high-π particles replicate everywhere. low-π particles maintain minimum replication. the network spends storage budget where attention goes.

## the observation

storage is finite. replication is expensive. uniform replication wastes resources: an obscure particle with zero inbound energy gets the same storage as the most-queried particle in the graph.

π measures attention probability. a particle with π = 0.01 accounts for 1% of all queries. a particle with π = 10⁻¹² is effectively never accessed. their storage requirements differ by 10 orders of magnitude.

## replication function

```
replication_factor(particle) = max(R_min, R_base × π(particle) / π_median)

where:
  R_min  = minimum replication (e.g., 3 — survival guarantee)
  R_base = baseline replication at median π (e.g., 10)
  π_median = median π across all particles
```

examples at realistic power-law distribution:

```
particle class        π estimate    replication factor
top-100 particle      ~10⁻²        ~1000 (effectively everywhere)
top-10K particle      ~10⁻⁴        ~100
median particle       ~10⁻⁶        10 (baseline)
tail particle         ~10⁻¹²       3 (minimum)
```

## DAS parameter scaling

DAS (data availability sampling) parameters scale with replication:

```
standard DAS:
  sample k random cells from 2D Reed-Solomon encoding
  confidence: 1 - (1/2)^k for uniform availability
  k = 20 gives 99.9999% confidence

π-weighted DAS:
  high-π particles: more nodes hold data → higher base availability
    → fewer samples needed for same confidence
    → or: same samples give higher confidence

  low-π particles: minimum replication
    → standard sampling required
    → but: queried rarely, so amortized cost is acceptable
```

the network's sampling budget is naturally efficient: high-π content is easy to verify (many replicas), low-π content needs more work per query but is queried rarely.

## storage economics

```
neuron focus expenditure:
  creating a cyberlink costs focus proportional to weight
  focus sustains the particle (temporal decay)
  focus = attention = storage incentive

natural equilibrium:
  high-π particle ← many neurons link to it ← lots of focus sustains it
    → high replication is funded by aggregate attention
  low-π particle ← few neurons link to it ← minimal focus
    → minimum replication matches minimal demand
```

no explicit storage market needed. focus IS the storage payment. π IS the replication signal. the economics emerge from the graph topology.

## interaction with gravity commitment

gravity commitment ([[gravity-commitment]]) makes high-π particles cheaper to verify. π-weighted replication makes them cheaper to store and retrieve. both follow the same power law:

```
high-π particle:
  verification: ~1 KiB proof, ~10 μs (gravity hot layer)
  storage: ~1000 replicas (π-weighted)
  retrieval: sub-ms (ubiquitous availability)

low-π particle:
  verification: ~8 KiB proof, ~200 μs (gravity cold layer)
  storage: 3 replicas (minimum)
  retrieval: seconds (sparse availability, possible DAS reconstruction)
```

the entire stack — proof cost, verification time, storage cost, retrieval latency — follows the attention distribution. the system is efficient because it mirrors how information is actually used.

## interaction with temporal decay

temporal decay reduces axon weight over time:

```
w_eff(t) = A_pq × α^(t - t_last)
```

as weight decays, the particle's aggregate energy decreases, π drops, replication factor drops. if nobody reinforces the link, the particle naturally fades from high replication to minimum replication to eventual pruning.

```
lifecycle:
  t=0:    new particle, high attention   → π high   → R=100
  t=30d:  attention sustained            → π stable → R=100
  t=180d: attention waning               → π drops  → R=30
  t=1y:   mostly forgotten               → π low    → R=3
  t=2y:   below pruning threshold        → pruned   → R=0 (signals preserve record)
```

the storage lifecycle mirrors the attention lifecycle. no admin intervention.

## implementation

```
per-epoch replication adjustment:
  1. compute π for all particles (tri-kernel, already happens for foculus)
  2. compute replication_factor per particle
  3. gossip replication targets to storage nodes
  4. nodes self-select which particles to replicate based on:
     - their available storage
     - particle replication needs
     - proximity in DHT / network topology

node incentive:
  replicate high-π particles → serve more queries → earn more karma
  natural alignment: storing important data is profitable
```

## open questions

1. **replication verification**: how does a node prove it actually stores the data? challenge-response (random query with proof of correct response) or periodic DAS checks. interacts with [[storage-proofs]]
2. **replication lag**: when π changes (new cyberlinks shift attention), replication takes time to adjust. during the lag, newly-important particles may be under-replicated. caching at query nodes provides a buffer
3. **minimum replication guarantee**: R_min = 3 assumes at least 3 honest nodes store every particle. at extreme scale (10^15 particles), is this feasible? or does R_min apply only to particles above pruning threshold?
4. **sybil resistance**: a neuron creating millions of self-links to boost π and replication. focus cost + tri-kernel's diffusion structure limit this, but explicit bounds may be needed

see [[gravity-commitment]] for proof cost scaling, [[storage-proofs]] for retention verification, [[temporal-polynomial]] for time-dimension interaction