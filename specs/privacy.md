---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
---
# privacy

the [[cyberlink]] is private — who linked what, and how much conviction, is never disclosed. individual linking decisions are protected because surveillance kills the freedom to link. this is the reason the entire stack was built from the ground up.

the [[cybergraph]] is public — it is the aggregate. [[axons]] (total weight between particle pairs), [[neuron]] summaries (total focus, karma), [[particle]] energy, [[token]] supplies, φ* distribution — all derived from cyberlinks but revealing no individual contribution, no individual amount.

## privacy boundary

```
                   PUBLIC (validators + everyone)   PRIVATE (nobody)
─────────────────  ────────────────────────────────  ──────────────────────────────
CYBERLINK                                            7-tuple (ν, p, q, τ, a, v, t)
                                                     who linked what
                                                     individual conviction amount
                                                     individual valence
NEURON             total focus                       linking history
                   karma κ                           individual cyberlinks
                   total stake
PARTICLE           CID exists                        who contributed
                   total energy (Σ weight)           individual contribution amounts
                   φ* ranking
AXON               H(from, to) exists                which neurons contributed
                   aggregate weight A_{pq}            individual weights
TOKEN              denominations                     individual UTXO values
                   total supply per τ                 owner identity
RECORD                                               value, owner, nonce, randomness
TRANSACTION        nullifiers (spent)                which records spent
                   new commitment keys               who spent them
                   new block state roots             transaction amounts
                   ring sum validity                 per-transaction Δ per particle
FOCUS              φ* distribution
                   rankings
```

## state model

**public knowledge graph state** — queryable by anyone, updated per block:

```
KG_state:
  particle_energies:  Map<CID, u64>           total energy per particle
  axon_weights:       Map<(CID, CID), u64>    aggregate weight per axon
  neuron_summaries:   Map<NeuronId, Summary>  total focus, karma
  rankings:           Vec<(CID, φ*)>          ordered particle rankings
  token_supplies:     Map<τ, u64>             total supply per token type
```

**private transaction state** — never leaves the prover:

```
per transaction (private):
  input UTXOs:       RLWE commitment preimages (particle, value, owner, nonce, ρ)
  output UTXOs:      new preimages
  individual Δ:      how much this transaction moved each particle's energy
  nullifiers:        derived from inputs (public — needed for double-spend check)
  new commitments:   RLWE commitments to output values (public — recipients find them)
```

## homomorphic backbone

the stack embeds the jali algebra — `R_q = F_p[x]/(xⁿ+1)` — with Ikat as its polynomial commitment scheme. ring commitments over R_q are additively homomorphic by construction. this eliminates the heaviest part of the block proof: conservation no longer requires a ZK circuit.

### RLWE value commitment

```
jali parameters:   n = 512, log₂(q) ≈ 60, noise budget ≈ 40–50 bits
ciphertext size:   n × 8 bytes = 4 KB per commitment

commit_jali(v, ρ):  RLWE encryption of value v with randomness ρ
                    = (a·ρ + v + e₁, −a·ρ + e₂)  mod q
                    where e₁, e₂ are small noise terms

additive homomorphism:
  commit_jali(v₁) + commit_jali(v₂) = commit_jali(v₁ + v₂) + noise
  noise stays small as long as Σ noise < q/2
```

### conservation check (no ZK circuit)

```
conservation verification — pure ring arithmetic:

  S = Σ commit_jali(v_input) − Σ commit_jali(v_output) − commit_jali(fee)

  valid:   ||S|| < q/2   (S is noise-only — no embedded nonzero message)
  fraud:   ||S|| ≥ q/2   (S encodes a nonzero value — inputs ≠ outputs + fee)

cost per block: O(T) polynomial additions in R_q — sub-μs per addition
validator verifies: one polynomial norm check, no proof required
```

noise budget for realistic block sizes: at n=512, each addition adds ~√q noise in expectation. for T=10,000 transactions per block, accumulated noise is well within the ~2^30 budget before approaching q/2.

### comparison: RLWE vs ElGamal/Pedersen

```
                     ElGamal / Pedersen (EC)    jali RLWE (BFV)
homomorphism:        additive (exact)            additive (with noise)
noise:               none                        ~√q per addition, bounded
ciphertext size:     32–64 bytes                 4 KB
addition cost:       ~1–2 μs (EC point)          sub-μs (poly addition)
post-quantum:        no (breaks under DLOG)      yes (RLWE hardness)
in stack:            not native (needs new EC)   jali is a strata algebra
```

ElGamal is not used because it requires adding a DDH-hard elliptic curve group outside the five strata algebras, and the stack is post-quantum by design throughout. RLWE ring addition is faster than EC point addition; the cost is only ciphertext size.

## polynomial mutator set

A_live(x) and N_live(x) are epoch-scoped multilinear evaluation tables over domain d = 2^16. they are **not** dimensions of BBG_poly — each has its own Lens commitment. BBG_root combines all three:

```
BBG_root = H(Lens.commit(BBG_poly) ‖ Ikat.commit(A_live) ‖ Brakedown.commit(N_live))
```

A_live uses Ikat (jali's PCS) because its entries are ring commitments. N_live uses Brakedown because its entries are field elements (nullifier flags).

### commitment polynomial A_live(x)

```
A_live[c] = commit_jali(v, ρ)    RLWE commitment to value v at key c
A_live[c] = 0                    key c not present

c = H_commit(particle ‖ owner ‖ nonce)   (key — does not embed value)
v is encoded only inside the RLWE ciphertext — not recoverable without ρ

mint: compute c, set A_live[c] = commit_jali(v, ρ)
      c published (recipients scan for their key)
      v hidden inside RLWE ciphertext

storage: each A_live entry is n Goldilocks field elements (ring element)
         fits natively in ShardStore Vec<Goldilocks>
```

### nullifier polynomial N_live(x)

```
N_live[n] = 0    nullifier n not yet spent this epoch
N_live[n] = 1    nullifier n spent this epoch

n = H_nullifier(record ‖ c ‖ ρ)   (Goldilocks field element)
n is published on spend — reveals that some UTXO was spent
n is unlinkable to c without knowing ρ
```

### epoch archive

```
at epoch boundary, before reset:
  A_k_root = Ikat.commit(A_live_epoch_k)      32 bytes
  N_k_root = Brakedown.commit(N_live_epoch_k) 32 bytes
  written to dim::TIME, key = H(epoch_k)

A_live and N_live reset to empty tables at epoch start.
```

### zheng accumulator

```
zheng_acc_k = zheng.fold(zheng_acc_{k-1}, A_k_root, N_k_root, epoch_k)

meaning:   no double spend through epoch k
size:      ~240 bytes constant
verify:    O(1)

Checkpoint = (BBG_root, zheng_acc, block_height)   ~280 bytes
```

## ownership — genies stealth addresses

[[genies]] — `F_q (CSIDH-512)` with Porphyry — provides post-quantum stealth addresses. each UTXO output is addressed to a one-time stealth key derived from the recipient's genies public key and the sender's randomness. the recipient scans the chain and identifies their UTXOs without revealing the link between their identity and the outputs.

```
stealth key derivation (sender):
  pk_stealth = CSIDH.derive(pk_recipient, r)   one-time address
  c = H_commit(particle ‖ pk_stealth ‖ nonce)

recipient scan:
  for each published c: check H_commit(particle ‖ CSIDH.derive(pk_self, r) ‖ nonce) == c?

ownership proof (ZK, small):
  prove knowledge of sk such that CSIDH.derive(pk_stealth, sk) is valid
  ~500 constraints — hemera-based, cheap
```

## block proof

with homomorphic conservation, the block proof is reduced to ownership and preimage proofs only.

```
BLOCK PROOF:
  public:   old_state_root, new_state_root
            nullifiers[]             (spent this block)
            new_commitment_keys[]    (c values for new UTXOs)
            ring_sum_check           (Σ inputs − Σ outputs − fee, verified by validator)
            block_height

  private:  per transaction:
              RLWE commitment preimages (particle, value, owner, nonce, ρ)
              ownership proofs (genies stealth key proofs)
              Ikat membership proofs (A_live[c] ≠ 0 for each input)
              zheng_acc + N_live non-membership (for each nullifier)
              individual Δ per particle (private — never published)

VALIDATOR CHECKS:
  1. ring_sum_check: ||Σ A_live[c_in] − Σ commit_jali(v_out) − commit_jali(fee)|| < q/2
  2. block proof is valid (ownership + preimage + range)
  3. nullifiers not already in N_live
  4. new_state_root commits to correct KG_state transition
```

the individual Δ per particle stays private inside the block proof. validators see new particle energies (public), not how each transaction contributed.

## block circuit

conservation is removed from the circuit. what remains:

```
CONSTRAINTS (per transaction in batch):
  commitment preimage:     ~736   hemera H_commit
  Ikat membership:         ~200   A_live opening
  zheng_acc check:         ~300   accumulator verify
  N_live non-membership:   ~200   Brakedown opening
  nullifier derivation:    ~500   hemera H_nullifier
  genies ownership:        ~500   stealth key proof
  range (64-bit values):   ~128   bit decomposition
  KG_state Δ binding:      ~400   aggregate correctness

per-transaction subtotal:  ~2,964 constraints   (was ~3,250 before homomorphism)
conservation:              0 constraints        (ring arithmetic, not ZK)
proof generation:          sub-second
proof size:                ~2 KiB
verification:              ~5–10 μs
```

the ring sum check removes conservation from the proof entirely and moves it to direct arithmetic verification by validators. this is the correct use of the homomorphic backbone.

## token functions

| function | mechanism | amounts visible? |
|---|---|---|
| mint | A_live[c] = commit_jali(v, ρ); c published | no — v inside RLWE |
| spend | N_live[n] = 1; ring sum checked | no — v is private witness |
| transfer | spend inputs + mint outputs | no |
| balance check | holder decrypts their own RLWE commitments with ρ | only to holder |
| double-spend reject | N_live[n] = 1 at block verify | structural reject |
| conservation | ring sum ||S|| < q/2 | totals only (encrypted) |

UTXOs never expire. epoch archive makes all historical A_k_root accessible via the time dimension.

## record model

```
Record:
  particle: F_p⁴    32 bytes   content identifier
  value:    u64       8 bytes   energy amount (PRIVATE — encoded in RLWE ciphertext)
  owner:    F_p⁴    32 bytes   stealth key (genies-derived, one-time)
  nonce:    F_p       8 bytes   random for uniqueness

commitment key:  c = H_commit(particle ‖ owner ‖ nonce)   (no value — 32 bytes)
value commitment: commit_jali(value, ρ) stored at A_live[c]
```

## implementation gaps

storage layer (ShardStore, TieredStore) is correct — jali ring elements are `Vec<Goldilocks>` (n elements per entry), native to the existing interface.

what is not yet implemented:

1. **jali RLWE commitment** — `bbg::crypto::commit_jali(v, ρ)` — BFV scheme over R_q. parameters: n=512, log₂(q)=60.
2. **ring sum check** — `bbg::prove::verify_conservation(inputs, outputs, fee)` — polynomial norm check.
3. **A_live entry type** — store RLWE ciphertext (n Goldilocks elements) per key instead of scalar 1.
4. **genies stealth addresses** — `bbg::crypto::stealth_derive` and scan — depends on genies crate availability.
5. **Ikat commitment for A_live** — `Ikat.commit(A_live)` replacing Brakedown for private commitment table.
6. **epoch reset** — `reset_epoch()` on dim::COMMITMENTS and dim::NULLIFIERS at epoch boundaries.
7. **epoch archive** — write A_k_root ‖ N_k_root to dim::TIME before reset.
8. **zheng_acc** — fold at epoch boundaries; carry in Checkpoint.
9. **block circuit** — ownership + preimage + range only; conservation removed.

see [[architecture]] for the layer model, [[state]] for transaction types, [[storage]] for backend configuration, [[strata]] for the jali algebra.
