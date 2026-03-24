# bbg

authenticated state layer for [[cyber]]. individual [[cyberlinks]] are private — who linked what is never disclosed. the [[cybergraph]] is the public aggregate: [[axons]], [[neuron]] summaries, [[particle]] energy, [[token]] supplies, π* distribution. all derived from cyberlinks, revealing no individual contribution.

## three laws

**bounded locality.** no global recompute for local change. every operation's cost is proportional to what it touches. at 10¹⁵ nodes, global operations are physically impossible.

**constant-cost verification.** any computation produces a proof verifiable in 10-50 μs via [[zheng]]-2 folding. verifier work is independent of prover work.

**structural security.** guarantees from data structure invariants, not protocol correctness. a tree whose internal nodes carry min/max namespace labels cannot lie about completeness — the structure itself prevents it.

## structure

13 sub-roots under BBG_root. each is 32 bytes ([[hemera]]-2 output). total: 416 bytes.

```
PUBLIC NMTs (9 roots)
  particles.root       all particles: content + axons, energy, π*
  axons_out.root       by source (outgoing axon index)
  axons_in.root        by target (incoming axon index)
  neurons.root         focus, karma, stake per neuron
  locations.root       proof of location
  coins.root           fungible token denominations
  cards.root           names and knowledge assets
  files.root           content availability (DAS)
  time.root            temporal index (7 namespaces)

PRIVATE STATE (3 roots)
  cyberlinks.root      MMR peaks hash (append-only commitment list)
  spent.root           MMR root (archived consumption proofs)
  balance.root         hash of active consumption bitmap (SWBF 128 KB)

FINALIZATION (1 root)
  signals.root         MMR (finalized signal batches)
```

## key numbers

```
hash output:          32 bytes (hemera, 24 rounds, ~736 constraints/perm)
proof size:           1-5 KiB (zheng-2)
verification:         10-50 μs
private transfer:     ~40,000 constraints, sub-second proving
cross-index (LogUp):  ~500 constraints per axon update (15× savings)
light client join:    one zheng verification + namespace sync
```

## specification

| document | content |
|----------|---------|
| [[architecture]] | three laws, ontology, 13 sub-roots, privacy model |
| [[state]] | BBG root, state diagram, checkpoint, state transitions |
| [[indexes]] | 9 NMT indexes, leaf structures, namespace semantics |
| [[privacy]] | mutator set (AOCL + SWBF), record model, transfer circuit |
| [[cross-index]] | LogUp cross-index consistency, batch verification |
| [[sync]] | sync at three scales: local (device CRDT), global (foculus), query (light client) |
| [[data-availability]] | 2D Reed-Solomon, NMT commitment, fraud proofs, DAS |
| [[temporal]] | edge decay, pruning protocol, storage reclamation |
| [[storage]] | tiered storage model, private record lifecycle |

## explanations

| document | question |
|----------|----------|
| [[architecture-overview]] | one polynomial, one root — the full architecture at a glance |
| [[why-polynomial-state]] | why polynomial > hash trees: 33x fewer constraints, 5 TB eliminated |
| [[polynomial-privacy]] | the privacy boundary: commitment and nullifier polynomials |
| [[why-signal-first]] | state is derived from signals: fold(genesis, signals[0..h]) |
| [[nmt explained|nmt]] | NMT's surviving role: cold storage optimization, not authentication |
| [[signal-sync explained|signal-sync]] | why signal DAG, VDF in the age of agents, structural BFT elimination |
| [[foculus-vs-crdt]] | why π convergence replaces CRDTs at global scale |
| [[data-availability explained|data-availability]] | algebraic DAS, erasure coding, and provable availability |

## open design

| proposal | status | topic |
|----------|--------|-------|
| [[valence]] | implemented | ternary epistemic field in cyberlink 7-tuple |
| [[storage-proofs]] | draft | proving data retention at all storage tiers |

## the stack

| repo | role | github |
|------|------|--------|
| [[nebu]] | field arithmetic | [nebu](https://github.com/cyberia-to/nebu) |
| [[hemera]] | hash function | [hemera](https://github.com/cyberia-to/hemera) |
| [[nox]] | virtual machine | [nox](https://github.com/cyberia-to/nox) |
| [[zheng]] | proof system | [zheng](https://github.com/cyberia-to/zheng) |
| [[mudra]] | communication primitives | [mudra](https://github.com/cyberia-to/mudra) |
| [[trident]] | language compiler | [trident](https://github.com/cyberia-to/trident) |

## license

Cyber License: Don't trust. Don't fear. Don't beg.
