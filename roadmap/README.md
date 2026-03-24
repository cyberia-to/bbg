# bbg proposals

design proposals for bbg authenticated state layer evolution. proposals compose with zheng-2 proof architecture and hemera-2 hash. see [[full-pipeline]] for how all proposals across bbg, zheng, hemera, and nox integrate into one continuous fold from device to light client.

## integrated architecture

| proposal | status | target |
|----------|--------|--------|
| [[full-pipeline]] | draft | end-to-end: signal creation → light client in one continuous fold |

## polynomial state

| proposal | status | target |
|----------|--------|--------|
| [[algebraic-nmt]] | draft | 9 NMT trees → 1 polynomial, 30× fewer constraints per cyberlink |
| [[mutator-set-polynomial]] | draft | SWBF + MMR → polynomial, O(1) non-membership proofs |
| [[temporal-polynomial]] | draft | continuous time queries, time.root elimination |
| [[unified-polynomial-state]] | draft | 13 sub-roots → 1 polynomial, LogUp eliminated |

migration: algebraic-nmt → mutator-set-polynomial → unified-polynomial-state. each phase independently valuable. temporal-polynomial composes with any phase.

## storage and replication

| proposal | status | target |
|----------|--------|--------|
| [[signal-first]] | draft | bbg state as materialized view over signal log, solves storage proofs |
| [[algebraic-das]] | draft | PCS openings replace NMT paths in DAS: 25 KiB → 9 KiB, 157× fewer constraints |
| [[pi-weighted-replication]] | draft | replication ∝ π, storage budget follows attention |
| [[storage-proofs]] | draft → resolved by signal-first | proving data retention at all 4 tiers |

signal-first reframes storage-proofs: prove signal availability, derive everything else. π-weighted replication allocates resources by importance.

## query

| proposal | status | target |
|----------|--------|--------|
| [[verifiable-query]] | draft | arbitrary CozoDB queries → polynomial opening proofs |

## implemented

| proposal | status |
|----------|--------|
| [[valence]] | implemented — migrated to spec |

## structural sync mapping

every proposal serves one of the five [[structural sync|structural-sync]] layers:

| layer | property | proposals |
|-------|----------|-----------|
| 1. validity | state transition correct | (served by zheng — see zheng props) |
| 3. completeness | nothing omitted | [[algebraic-nmt]], [[unified-polynomial-state]], [[verifiable-query]] |
| 4. availability | data physically exists | [[signal-first]], [[algebraic-das]], [[pi-weighted-replication]], [[storage-proofs]] |
| temporal | state over time | [[temporal-polynomial]] |
| privacy | private state | [[mutator-set-polynomial]] |

layers 2 (ordering) and 5 (merge) are already specified in [[sync]] — no proposals needed.

the polynomial proposals (algebraic-nmt → mutator-set → temporal → unified) form a single migration path: hash trees → polynomials across all layers. structural sync's completeness guarantee (layer 3) evolves from structural (NMT sorting invariant) to algebraic (PCS soundness) without losing the property.

## combined targets (all proposals)

```
                        bbg-1 (current)         bbg-2 (all proposals)   improvement
per-cyberlink:          ~94K constraints        ~3K constraints         30×
non-membership proof:   128 KB witness + MMR    32 bytes + 1 opening    O(1)
historical query:       3 × O(log n) NMT walk  1 polynomial opening    O(1)
sub-root count:         13                      1                       13×
cross-index (LogUp):    ~1.5K constraints       0 (structural)         ∞
storage proof:          open problem            signal availability     solved
replication cost:       uniform                 ∝ π (power-law)        natural
query proofs:           namespace only          arbitrary CozoDB        general
light client witness:   ~416 bytes roots        32 bytes polynomial     13×
```

## recommended sequence

```
phase 1: signal-first               reframe storage proofs
phase 2: algebraic-nmt              30× per-cyberlink (public indexes)
phase 3: mutator-set-polynomial     O(1) privacy proofs
phase 4: pi-weighted-replication    natural storage allocation
phase 5: temporal-polynomial        continuous time queries
phase 6: verifiable-query           arbitrary provable queries
phase 7: unified-polynomial-state   one polynomial for everything
```

## cross-repo dependencies

| zheng-2 proposal | bbg interaction |
|------------------|-----------------|
| [[gravity-commitment]] | verification cost + replication + query routing all follow π |
| [[proof-carrying]] | fold nullifier updates into signal accumulator |
| [[universal-accumulator]] | checkpoint = 200 bytes, enables signal-first recovery |
| [[folding-first]] | multi-hop queries fold per hop |
| [[brakedown-pcs]] | updateable PCS enables incremental polynomial state |
| [[structural-sync]] | defines the 5-layer sync model that bbg proposals implement (layers 1-5) |

## lifecycle

| status | meaning |
|--------|---------|
| draft | idea captured, open for discussion |
| accepted | approved — ready to implement |
| rejected | decided against, kept for rationale |
| implemented | done — migrated to relevant spec file |
