---
tags: cyber, cip
crystal-type: entity
crystal-domain: cyber
diffusion: 0.00010722364868599256
springs: 0.0025303868953724814
heat: 0.0017560247382455416
focus: 0.001163932840603834
gravity: 0
density: 1.28
---
# privacy

the [[cyberlink]] is private — who linked what is never disclosed. individual linking decisions are protected because surveillance kills the freedom to link.

the [[cybergraph]] is public — it is the aggregate. [[axons]] (total weight between particle pairs), [[neuron]] summaries (total focus, karma), [[particle]] energy, [[token]] supplies, π distribution — all derived from cyberlinks but revealing no individual contribution.

the polynomial mutator set provides: private ownership, unlinkable transactions, no trusted parties, and O(1) verification — simultaneously.

## privacy boundary

```
                   PUBLIC (aggregate)              PRIVATE (individual)
─────────────────  ──────────────────────────────  ─────────────────────────────
CYBERLINK                                          7-tuple (ν, p, q, τ, a, v, t)
                                                   who linked what
                                                   individual conviction amount
                                                   individual valence
NEURON             total focus                     linking history
                   karma κ                         individual cyberlinks
                   total stake
PARTICLE           CID exists
                   total energy (Σ weight)
                   π* ranking
AXON               H(from, to) exists              which neurons contributed
                   aggregate weight A_{pq}          individual weights
TOKEN              denominations                   individual UTXO values
                   total supply per τ               owner identity
RECORD                                             value, owner, nonce, randomness
TRANSACTION        nullifier polynomial update     which records spent
                   commitment polynomial extend    who spent them
                   Δ per particle                  new owners
                   proof validity                  link between add & remove
FOCUS              π distribution
                   rankings
```

the tri-kernel operates on axons — aggregate weights — not individual cyberlinks. the effective adjacency A^eff_{pq} sums contributions from many neurons. no individual contribution is visible. enough transparency for [[consensus]], enough privacy for participation.

## polynomial mutator set

A(x) and N(x) are independent polynomial commitments, NOT dimensions of BBG_poly. each has its own Brakedown PCS commitment (32 bytes). BBG_root combines all three: H(PCS.commit(BBG_poly) ‖ PCS.commit(A) ‖ PCS.commit(N)).

all hash functions are hemera with domain separation: H_commit uses capacity[11]=DOMAIN_COMMIT, H_nullifier uses capacity[11]=DOMAIN_NULLIFIER.

two polynomial commitments replace AOCL (MMR) and SWBF (bitmap + archived MMR).

### commitment polynomial A(x)

a polynomial over all committed records:

```
A(x) encodes all commitments:
  A(c_i) = v_i for commitment c_i with value v_i

commitment for UTXO u:
  c = H_commit(u.particle ‖ u.value ‖ u.owner ‖ u.nonce ‖ ρ)
  where ρ is hiding randomness contributed by the recipient

properties:
  - extended when a UTXO is created: A'(x) includes new evaluation point
  - append-only in semantic — polynomial degree grows monotonically
  - commitment update: O(1) field operations per new record
  - membership proof: PCS.open(A, c_i) → v_i, one opening, O(1)
```

### nullifier polynomial N(x)

tracks which records have been spent:

```
N(x) = ∏(x - n_i) for all nullifiers n_i

spending UTXO u (with commitment c, randomness ρ):
  1. compute nullifier n = H_nullifier(u ‖ c ‖ ρ)
  2. prove A(c) ≠ 0           (commitment exists — one PCS opening)
  3. prove N(n) ≠ 0           (not yet spent — one PCS opening)
  4. extend: N'(x) = N(x) × (x - n)   (O(1) polynomial update)
  5. provide ZK proof that nullifier derives from valid commitment

double-spend prevention:
  second spend attempt → N(n) = 0 → structural rejection

unlinkability:
  commitment: A(c) — polynomial evaluation at commitment point
  nullifier: N(n) — polynomial zero at nullifier point
  these share ZERO structural similarity visible to any observer
```

### comparison with SWBF

```
                          SWBF + MMR (previous)           polynomial
witness size:             128 KB (active window)          32 bytes (PCS commitment)
non-membership proof:     O(1) bitmap + O(log N) MMR     O(1) PCS opening
membership proof:         O(log N) MMR                   O(1) PCS opening
update (add nullifier):   O(1) bitmap + periodic archive O(1) polynomial extend
archived state:           growing MMR chunks              absorbed in polynomial
constraints per spend:    ~40,000                         ~5,000
density leakage:          bloom filter bits leak density  none (opaque commitment)

at 10^9 nullifiers:
  SWBF+MMR: 128 KB witness + ~30 hemera calls for archived chunks
  polynomial: 32-byte commitment + 1 PCS opening (~200 byte proof)
```

## record model

```
Record:
  particle: F_p⁴    32 bytes   content identifier
  value:    u64       8 bytes   energy amount
  owner:    F_p⁴    32 bytes   owner public key hash
  nonce:    F_p       8 bytes   random for uniqueness

commitment(r, ρ) = H_commit(r.particle ‖ r.value ‖ r.owner ‖ r.nonce ‖ ρ)
```

## private transfer circuit

```
PUBLIC INPUTS:
  A_commit:        F_p⁴               commitment polynomial PCS commitment
  N_commit:        F_p⁴               nullifier polynomial PCS commitment
  nullifiers:      [F_p; 4]           nullifier values for spent inputs
  additions:       [F_p⁴; 4]          new commitment evaluation points
  deltas:          [(F_p⁴, i64); 8]   per-particle value changes
  fee:             u64                 transaction fee

PRIVATE WITNESS:
  input_records, input_secrets, input_randomness
  commitment_openings (PCS proofs for A(c_i))
  non_membership_openings (PCS proofs for N(n_i) ≠ 0)
  output_records, output_randomness
  input_enabled, output_enabled

CONSTRAINTS:
  input validation (4 inputs):              ~4,000
    commitment correctness: ~736 per input (hemera for H_commit)
    A(c) membership: ~200 per input (PCS opening verification)
    N(n) non-membership: ~200 per input (PCS opening verification)
    nullifier derivation: ~500 per input
    ownership proof: ~736 per input
  output validation (4 outputs):             ~3,500
  conservation:                                ~100
  delta consistency:                           ~300
  uniqueness:                                   ~50

TOTAL:                                       ~5,000 constraints (was ~40,000)
proof generation (zheng-2):                  sub-second
proof size:                                  ~2 KiB
verification:                                ~5 μs
```

## proof maintenance

every UTXO holder keeps proofs synchronized as the polynomial state evolves:

```
new UTXO created:     commitment polynomial A(x) extended — holder's PCS proof still valid
                      (polynomial extension does not invalidate existing openings)
old UTXO spent:       nullifier polynomial N(x) extended — holder's non-membership proof
                      needs refresh (new nullifier could affect opening)
                      refresh cost: O(1) field operations per new nullifier

total user cost:
  average: O(1) per block (was O(log L · log N) with SWBF+MMR)
  for 10^9 users, 10-year UTXO: ~1-2 field operations per block
  constraints per block for maintenance: negligible
```

## privacy preservation

the polynomial commitment reveals nothing about individual records. PCS opening proofs are zero-knowledge. the privacy guarantees match or exceed the previous SWBF approach:

- SWBF: bloom filter bits leak probabilistic information about nullifier density
- polynomial: commitment is one opaque 32-byte digest. no density leakage
- commitment isolation: opening A(c) reveals nothing about N(n) — A(x) and N(x) are independent polynomial commitments with separate PCS instances

see [[architecture]] for the layer model, [[state]] for transaction types, [[cross-index]] for why LogUp is eliminated
