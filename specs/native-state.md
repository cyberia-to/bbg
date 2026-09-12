---
title: native state transitions
tags: bbg, storage, spec
---
# native state transitions

The native node prepares a BBG transition under an exclusive mutable borrow.
A prepared signal applies the signal and its global history-position header,
then finalizes one block. An intent stores its exact record without advancing
the block height. The caller commits the returned typed-state changes together
with its operation, economics and receipt in one Database transaction, then
publishes the preparation. Dropping a preparation restores its previous logical
state, checkpoint and pruning state, and refreshes the root cache. Preparation
errors perform the same restoration before returning. A batch contains 1 to
64 events under one preparation; any failing event restores the whole batch.
Each event exposes its resulting height/root snapshot in order. No callbacks
run here.

Undo records cover only touched keys and affected adjacency lists. The state
graph is not cloned. Each preparation allows at most 65536 recorded keys and
8 MiB of encoded key/value bytes independently in its undo and resulting write
sets. A single record has the same 8 MiB bound. At epoch boundaries, pruning
candidate collection is limited to 65536 entries; exceeding a limit rejects
the transition and restores the preceding state. Root calculation retains the
existing BBG full-state cost. Pruning ranking retains its existing global scan.

Native preparation rejects repeated nullifiers within one signal, previously
spent nullifiers, overflowing balance additions, height overflow, oversized
link counts and inconsistent signal/header arguments. The coordinator remains
responsible for semantic authorization and proof verification. Signals are
indexed by the supplied global history position; a position already present
is rejected. Native persistence requires an absent checkpoint accumulator.

## typed record format

NativeState keys start with a one-byte domain. Entity keys append their exact
32 bytes; time and signal positions append eight big-endian bytes. Metadata
uses the single-byte key 0. Values contain exact little-endian integers and
fixed-width byte arrays. No key or u64 value is reduced into a field element.

| Domain | Record |
|---|---|
| 0 | version u32=1, height u64, state root 32, checkpoint height u64, checkpoint root 32, diameter option u8 + optional u64, pruning max bytes u64, rank floor u8, half-life u64 |
| 1 | particle: energy, pi_star, weight, s_yes, s_no, meta_score (six u64) |
| 2, 3 | outgoing/incoming adjacency: count u64 followed by ordered 32-byte entries |
| 4 | neuron: focus, karma, stake (three u64) |
| 5 | location: lat, lon (two i32) |
| 6 | coin: total supply u64 |
| 7 | card: owner 32, particle 32 |
| 8 | file: available u8 (0 or 1), chunk count u32 |
| 9 | time: root 32 |
| 10 | signal: neuron 32, network 32, link count u32, height u64, proof hash 32 |
| 11 | commitment: canonical field representative u64 |
| 12 | nullifier: empty value |
| 13 | public balance: u64 |
| 14 | intent: neuron 32, inception height u64, scope hash 32, signature 64 |
| 15 | axon reverse edge: from 32, to 32 |
| 16 | last touched epoch: u64 |

The record iterator emits this exact format in lexicographic key order, with
one record at a time for full-state recovery comparison. Metadata includes the
pruning policy because it changes future transitions. The current authenticated
root commits its existing dimensions and excludes intents and pruning metadata;
durable exact-record validation checks those records independently. Persistence
does not upgrade the existing root or claim new cryptographic guarantees.
