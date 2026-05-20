---
tags: cyber, cip
crystal-type: spec
crystal-domain: cyber
---
# neuron state

oblivious resolution of neuron-owned outputs against the polynomial mutator set. a neuron discovers its outputs and proves their validity without revealing its identity to any external party.

## the problem

private outputs in BBG are indexed by stealth commitment keys:

```
c = H_commit(particle ‖ pk_stealth ‖ nonce)
A_live[c] = commit_jali(value, ρ)      ← RLWE-encrypted, 4 KB per entry
```

where `pk_stealth = CSIDH.derive(pk_recipient, r)` is a one-time stealth key. the sender publishes `(c, r_pub)` per output; the recipient scans by computing `CSIDH.derive(r_pub, sk_self)` and checking against each `c`.

scanning is O(N) in total outputs on chain — one CSIDH-512 group action per published `r_pub`. CSIDH-512 costs ~1–5 ms per derivation. at 100,000 outputs per epoch, a full historical scan takes minutes of CPU on the client side. a light client that avoids downloading the full chain has no tractable path to recovering its own outputs under the current scheme.

three gaps:

1. **scan cost** — O(N) CSIDH operations, paid entirely by the client
2. **delegation risk** — delegating the scan requires sharing `sk_self` (full spending key), which is unsafe
3. **no light-client path** — no compact proof that a server found ALL outputs belonging to a key

## key hierarchy extension

split the neuron key into two independent keys with an asymmetric security boundary:

```
neuron keys:
  sk_spend    — spending authority, never leaves the device
  sk_view     — viewing key, derived from sk_spend, safe to share with scan services

  derivation: sk_view = H_key(sk_spend, "view")   (hemera, domain-separated)
  pk_view     = CSIDH.pub(sk_view)                 (CSIDH-512 public key)
  pk_spend    = CSIDH.pub(sk_spend)                (existing recipient public key)

sk_view → identifies outputs (viewing)
sk_spend → authorizes spends (never derivable from sk_view)
```

## augmented output encoding

add a compact scan tag to each output. the tag is computable with `sk_view` alone; spending still requires `sk_spend`.

```
sender, given (pk_view, pk_spend) of recipient:
  r:        ephemeral randomness (sender private)
  r_pub:    CSIDH.pub(r)                          ← published

  shared:   CSIDH.derive(pk_view, r)              ← CSIDH group action on view key
  tag:      H_tag(shared ‖ "scan")                ← 32 bytes, published per output
  pk_stealth: H_tag(shared ‖ "spend") · pk_spend  ← derivation, for spending key only
  c:        H_commit(particle ‖ pk_stealth ‖ nonce) ← commitment key, published
  A_live[c] = commit_jali(value, ρ)              ← RLWE ciphertext, in Ikat table

block publishes per output: (c, r_pub, tag)
A_live stores RLWE ciphertext at key c
```

the tag is a deterministic function of `(r_pub, sk_view)`. the stealth key is a deterministic function of `(r_pub, sk_spend)`. the two derivations are independent — knowing `tag` does not help derive `pk_stealth` or `sk_spend`.

## output accumulator polynomial

define P_out(x) as the epoch-scoped output accumulator polynomial:

```
P_out(x) = Π (x - c_i)    for all unspent c_i in the current epoch

maintained by full nodes alongside A_live:
  new output c:  P_out := P_out · (x - c)    O(deg) field muls
  output spent:  P_out := P_out / (x - c)    O(deg) field ops (polynomial division)
  committed via: Ikat.commit(P_out)           carried in BBG_root per epoch

degree bound: max outputs per epoch (e.g., 2^16 = 65,536)
epoch boundary: archive P_out_k, reset P_out = 1
```

P_out is an Ikat-committed polynomial over the Goldilocks field. its roots are exactly the unspent commitment keys in the epoch. historical epochs carry archived Ikat commitments via `dim::TIME`.

## oblivious scan via polynomial GCD

the client constructs a query polynomial whose roots are the candidate commitment keys the client would own IF those outputs were sent to it, then asks the server to compute the intersection with P_out without learning which roots the client is testing.

```
client (has sk_view, downloads all (r_pub_i, tag_i) pairs for target epoch):
  1. for each published r_pub_i in the epoch:
       shared_i   = CSIDH.derive(r_pub_i, sk_view)   ← one CSIDH op per output
       tag_cand_i = H_tag(shared_i ‖ "scan")
       if tag_cand_i == tag_i:
         c_cand_i = H_commit(particle_i ‖ derive_stealth(shared_i, pk_spend) ‖ nonce_i)
         add c_cand_i to candidate set S

  2. build query polynomial:
       Q(x) = Π (x - c_cand_i)    for all c_cand_i ∈ S
       Q has degree |S| = number of candidate matches

  3. send commit(Q) to scan service        ← ~48 bytes, reveals nothing about sk_view
```

the scan service does not touch sk_view or CSIDH. it performs only polynomial arithmetic:

```
scan service (has Ikat.commit(P_out), commit(Q)):
  G(x) = gcd(P_out(x), Q(x))          ← Euclidean algorithm over Goldilocks field
                                          roots of G = c_i that are in both P_out and Q
                                          = outputs that are unspent AND owned by client

  prove:
    π₁: P_out = G · H₁    (G divides P_out)   ← Ikat polynomial division proof
    π₂: Q     = G · H₂    (G divides Q)        ← polynomial identity
    both proofs are O(1) Ikat openings

  return: (Ikat.commit(G), π₁, π₂)
```

client verification:

```
client:
  verify π₁: Ikat.verify(Ikat.commit(P_out), Ikat.commit(G), Ikat.commit(H₁))
             ← G divides P_out (owned outputs are unspent)
  verify π₂: Ikat.verify(commit(Q), Ikat.commit(G), Ikat.commit(H₂))
             ← G divides Q (owned outputs are among candidates)

  decode G: for each c_cand_i ∈ S, evaluate G(c_cand_i) — if 0, output is owned
  owned outputs = {c_i ∈ S : G(c_i) = 0}
```

what the scan service learns:
- deg(G) — number of owned outputs in the epoch (unavoidable)
- that a query occurred
- commit(Q) — reveals nothing about sk_view since candidate keys are CSIDH-derived and appear as random field elements without sk_view

what the scan service does NOT learn:
- sk_view or sk_spend
- which specific c_i values in P_out were matched
- the client's identity

## ZK scan completeness proof

the GCD intersection proves "these outputs are unspent and among my candidates." it does not prove the scan was complete (no candidate was omitted). completeness requires a ZK proof of scan execution.

define the scan program `scan_exec`:

```
scan_exec(
  public:   epoch scan index — all (r_pub_i, tag_i, c_i) triples for the epoch
            Ikat.commit(P_out)
  private:  sk_view
) → (owned_c_set, zheng_proof)

execution:
  for each (r_pub_i, tag_i, c_i) in epoch scan index:
    shared_i   = CSIDH.derive(r_pub_i, sk_view)
    tag_cand   = H_tag(shared_i ‖ "scan")
    if tag_cand == tag_i: record c_i as owned
  emit owned_c_set
  emit zheng proof of correct execution (sk_view is private witness)
```

the proof is a STARK over the scan_exec circuit. `sk_view` is a private witness. the public claim is:

```
"running scan_exec with witness sk_view over epoch index E produced set O,
 and every output in E was processed — no output was skipped"
```

verification against BBG_root:

```
verify(zheng_π, epoch_index_root, Ikat.commit(P_out), commit(O)) → accept/reject

cost:  ~5 μs (one zheng decider)
size:  ~2 KiB
```

the zheng proof certifies completeness. the GCD proof certifies unspentness. together:

```
owned_outputs = O such that:
  ∀ c ∈ O: P_out(c) = 0                      ← unspent (GCD proof)
  ∀ c tagged to sk_view in epoch E: c ∈ O    ← complete (ZK scan proof)
```

## recursive scan folding

a full history scan across K epochs produces K independent proofs. fold them into a single constant-size accumulator via zheng/HyperNova:

```
scan_acc_0 = zheng.init()

for each epoch k from 0 to K:
  (O_k, π_k) = scan_exec result for epoch k
  scan_acc_k = zheng.fold(scan_acc_{k-1}, π_k)

scan_acc_K:  ~240 bytes (constant, independent of K)
             encodes: "from genesis to epoch K, owned outputs are ∪ O_k"
             carries: aggregate unspentness + completeness for all epochs
```

a light client syncing for the first time:

```
1. fetch BBG_root + scan_acc_K                        (~280 bytes)
2. fetch owned output values: {A_live[c] : c ∈ ∪ O_k} (only owned entries, ~4 KB each)
3. verify scan_acc_K against BBG_root                  (~5 μs, O(1))
4. decrypt A_live[c] entries locally with ρ            (stored at wallet creation)
```

no full block download required. no O(N) scan on the light client. the scan accumulator is produced by a scan service or the client's always-on device; a new device receives it and verifies in microseconds.

## value retrieval

after identifying owned c values, retrieve the RLWE-encrypted values from A_live:

```
retrieve A_live[c] with proof:
  request:  c (commitment key)
  response: (commit_jali(v, ρ), Ikat.open(A_live, c))
  verify:   Ikat.verify(Ikat.commit(A_live), c, commit_jali(v, ρ), π) → accept/reject

decrypt locally:
  (v, ρ) = jali.decrypt(commit_jali(v, ρ), sk_spend)
  balance += v
```

the Ikat opening proves the server returned the correct RLWE ciphertext. decryption requires `sk_spend` (never leaves the device). the server proves correctness without knowing `ρ` or `v`.

batch retrieval: all owned outputs from a single epoch in one Ikat batch opening (~200 bytes proof regardless of batch size).

## oblivious value retrieval (maximum privacy)

to prevent the server from learning WHICH c values the client is retrieving (access pattern), use two-server polynomial PIR:

```
client:
  split query set {c_1, ..., c_k} into two random shares S_1, S_2 such that S_1 ⊕ S_2 = S
  (S is a random linear combination over the Goldilocks field)
  send S_1 to server A, S_2 to server B

each server evaluates A_live at their share of the query — polynomial evaluation
client recombines: A_live[c_i] = server_A(S_1) + server_B(S_2)

neither server learns {c_1, ..., c_k} individually
```

PIR requires two non-colluding full nodes. the polynomial structure of A_live (Ikat commitment, Goldilocks field) makes this a native polynomial evaluation — no new cryptographic primitives.

## privacy tiers

| mode | scan cost | server learns | data downloaded | proof |
|---|---|---|---|---|
| local full scan | O(N) CSIDH ops, client | nothing | all (r_pub, tag) pairs | none |
| delegated scan | O(N) CSIDH ops, service | sk_view + balance pattern | owned outputs only | none |
| GCD + ZK | O(N) CSIDH ops, client | output count, query occurred | commit(Q), proofs | GCD + ZK completeness |
| folded accumulator | O(N) total, amortized | varies by scan mode | BBG_root + scan_acc | O(1) zheng fold |
| oblivious retrieval | — | nothing (2-server) | owned entries | Ikat batch opening |

the folded accumulator mode is the default for any client that is not always online. local full scan is the maximum-privacy baseline. oblivious retrieval applies only after c values are known.

## incremental updates

after initial sync, new outputs arrive per block. the client (or delegated scan service) processes only the new epoch's outputs:

```
per block:
  download new (r_pub_i, tag_i, c_i) triples       (~64 bytes × outputs_per_block)
  run scan_exec on new triples only                 (incremental, O(new outputs))
  fold new π into existing scan_acc                 (zheng.fold, O(1))
```

the incremental update cost is proportional to new outputs per block, not total chain history.

## protocol requirements

this spec requires two protocol additions not present in the current genies stealth address scheme:

1. **tag field in output announcement** — each output publishes `tag = H_tag(shared ‖ "scan")` alongside `(c, r_pub)`. this is a 32-byte addition to the public announcement, covered by the block proof.

2. **P_out epoch polynomial** — full nodes maintain the output accumulator polynomial per epoch and expose Ikat.commit(P_out) alongside Ikat.commit(A_live) in BBG_root. epoch archival of P_out_k follows the same path as A_live archival (epoch boundary, write to dim::TIME).

both additions are backward-compatible additions to the block structure. existing spending paths and conservation checks are unchanged.

## implementation gaps

not yet implemented:

1. **sk_view derivation** — `bbg::crypto::derive_view_key(sk_spend)` — hemera H_key with "view" domain tag
2. **scan tag field** — `OutputAnnouncement` struct gains `tag: [u8; 32]` — block proof covers it
3. **P_out epoch polynomial** — `bbg::state::OutputAccumulator` — maintained alongside A_live
4. **GCD service** — `bbg::scan::gcd_query(commit_Q, epoch) → (commit_G, π₁, π₂)` — polynomial Euclidean algorithm over Goldilocks + Ikat division proofs
5. **scan_exec circuit** — STARK over the scanning loop — sk_view as private witness
6. **zheng scan folding** — `bbg::scan::fold_epoch(scan_acc, π_scan) → scan_acc'` — HyperNova accumulator step
7. **PIR retrieval** — `bbg::scan::pir_retrieve(c_set, server_a, server_b)` — two-server linear PIR over Goldilocks

see [[privacy]] for the commitment scheme and A_live structure, [[state]] for BBG_root composition, [[query]] for Ikat opening protocol
