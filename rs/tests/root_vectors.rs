// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! Root derivation vectors and properties.
//!
//! v1 roots are pinned to the values the pre-incremental code produced
//! (bbg `441e3c9`, 2026-10-15), so the per-dimension caches cannot drift the
//! legacy derivation. v2 roots are checked against from-scratch rebuilds
//! after random batches, pruning and replay — the property the incremental
//! path exists to preserve.

use bbg::types::{
    CardRecord, CoinRecord, FileRecord, LocationRecord, NeuronRecord, ParticleRecord, SignalRecord,
};
use bbg::{Bbg, BbgState, BoxMove, Cyberlink, PruneConfig, RootVersion, Signal};

fn p(seed: u8) -> [u8; 32] {
    [seed; 32]
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn one_link(neuron: [u8; 32], from: [u8; 32], to: [u8; 32], amount: u64) -> Signal {
    Signal {
        neuron,
        links: vec![Cyberlink { from, to, token: p(0), amount, valence: 1 }],
        box_moves: vec![],
        height: 0,
    }
}

/// Every dimension populated, several blocks finalized.
fn all_dims(version: RootVersion) -> Bbg {
    let mut b = Bbg::new().with_root_version(version);
    for i in 1..4u8 {
        b.state.neurons.insert(p(i), NeuronRecord { focus: 10_000, karma: i as u64, stake: 0 });
    }
    for i in 0..6u8 {
        let s = Signal {
            neuron: p(1 + i % 3),
            links: vec![
                Cyberlink { from: p(10 + i), to: p(20 + i % 2), token: p(0), amount: 3 + i as u64, valence: 1 },
                Cyberlink { from: p(10), to: p(30 + i), token: p(1), amount: 1, valence: 0 },
            ],
            box_moves: vec![BoxMove { nullifier: p(100 + i), commitment: Some((p(200 + i), 11 + i as u64)) }],
            height: i as u64,
        };
        b.insert(&s).unwrap();
        b.finalize_block();
    }
    b.state.locations.insert(p(40), LocationRecord { lat: -5, lon: 110 });
    b.state.coins.insert(p(41), CoinRecord { total_supply: 1_000_000 });
    b.state.cards.insert(p(42), CardRecord { owner: p(1), particle: p(10) });
    b.state.files.insert(p(43), FileRecord { available: true, chunk_count: 9 });
    b.apply_signal_record(
        0,
        SignalRecord { neuron: p(1), network: p(50), link_count: 2, block_height: 0, proof_hash: p(51) },
    );
    b.set_diameter_bound(4);
    b
}

// ── v1: bit-exact with the pre-incremental derivation ─────────────────────────

#[test]
fn v1_empty_root_is_pinned() {
    assert_eq!(hex(&Bbg::new().state.root()), "6778e49e73d2935c3e1c397783a44a421b86f8181f51115601ea9cf0112fdfa2");
}

#[test]
fn v1_one_link_roots_are_pinned() {
    let mut b = Bbg::new();
    b.state.neurons.insert(p(1), NeuronRecord { focus: 1000, karma: 2, stake: 3 });
    b.insert(&one_link(p(1), p(2), p(3), 7)).unwrap();
    assert_eq!(hex(&b.state.root()), "387298caa2d77c723697a61b2d0f8b29c696050781667271d5cc87ae64abbe46");
    b.finalize_block();
    assert_eq!(hex(&b.state.root()), "496832a5608df264c76fde84774c2b60236b1f745f27d6240729d48e8894e7cf");
}

#[test]
fn v1_all_dimension_roots_are_pinned() {
    let mut b = all_dims(RootVersion::V1);
    assert_eq!(hex(&b.state.root()), "0639b2f70cf6e735899a2f38d006eccdc84cbc93d34f18ac81dcc2ad8dbc55db");
    b.finalize_block();
    assert_eq!(hex(&b.state.root()), "ea9a1f882b3c8dc75b12543bb13e291068a1b7ae652f572bb5b72589cb6760d5");
}

#[test]
fn v1_is_the_default_and_v2_differs() {
    assert_eq!(BbgState::new().root_version(), RootVersion::V1);
    assert_ne!(all_dims(RootVersion::V1).state.root(), all_dims(RootVersion::V2).state.root());
    // The version is a view of the records: flipping it flips the root, and
    // the flipped root is the root a state built under that version carries.
    // (Block finalization records the root in `time`, so states that finalized
    // blocks under different versions hold different records — compare before
    // any block is finalized.)
    let build = |version| {
        let mut b = Bbg::new().with_root_version(version);
        b.state.neurons.insert(p(1), NeuronRecord { focus: 1000, karma: 2, stake: 3 });
        b.insert(&one_link(p(1), p(2), p(3), 7)).unwrap();
        b
    };
    let (v1, v2) = (build(RootVersion::V1), build(RootVersion::V2));
    let mut both = build(RootVersion::V1);
    assert_eq!(both.state.root(), v1.state.root());
    both.state.set_root_version(RootVersion::V2);
    assert_eq!(both.state.root(), v2.state.root());
    both.state.set_root_version(RootVersion::V1);
    assert_eq!(both.state.root(), v1.state.root());
}

// ── v2: incremental root == from-scratch root ─────────────────────────────────

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn key(&mut self, space: u64, tag: u8) -> [u8; 32] {
        let mut k = [tag; 32];
        k[..8].copy_from_slice(&(self.next() % space).to_le_bytes());
        k
    }
}

/// Rebuild a state from scratch out of the final record set of `src`.
///
/// Adjacency lists are re-added in reverse order so the rebuild also proves
/// list order does not leak into the v2 root.
fn rebuild(src: &BbgState) -> BbgState {
    let mut s = BbgState::new();
    s.set_root_version(RootVersion::V2);
    for (k, v) in src.particles.iter() {
        s.particles.insert(*k, ParticleRecord { ..*v });
    }
    for (k, list) in src.axons_out.iter() {
        for a in list.iter().rev() {
            s.axons_out.add(*k, *a);
        }
    }
    for (k, list) in src.axons_in.iter() {
        for a in list.iter().rev() {
            s.axons_in.add(*k, *a);
        }
    }
    for (k, v) in src.neurons.iter() {
        s.neurons.insert(*k, NeuronRecord { ..*v });
    }
    for (k, v) in src.time.iter() {
        s.time.insert(*k, *v);
    }
    for (k, v) in src.signals.iter() {
        s.signals.insert(*k, SignalRecord { ..*v });
    }
    for (k, v) in src.commitments.iter() {
        s.commitments.insert(*k, *v);
    }
    for k in src.nullifiers.iter() {
        s.nullifiers.insert(*k);
    }
    for (k, v) in src.balances.iter() {
        s.balances.insert(*k, *v);
    }
    s.diameter_override = src.diameter_override;
    s.height = src.height;
    s
}

#[test]
fn v2_incremental_root_matches_from_scratch_after_random_batches() {
    let mut rng = Rng(0x5EED);
    let mut b = Bbg::new().with_root_version(RootVersion::V2);
    for i in 0..8u8 {
        b.state.neurons.insert(p(i), NeuronRecord { focus: u64::MAX / 4, karma: 0, stake: 0 });
    }
    for round in 0..40u64 {
        for _ in 0..25 {
            let sig = Signal {
                neuron: p((rng.next() % 8) as u8),
                links: vec![Cyberlink {
                    from: rng.key(60, 0xA0),
                    to: rng.key(60, 0xB0),
                    token: p((rng.next() % 2) as u8),
                    amount: 1 + rng.next() % 50,
                    valence: 1,
                }],
                box_moves: if rng.next().is_multiple_of(3) {
                    vec![BoxMove { nullifier: rng.key(1 << 20, 0xC0), commitment: Some((rng.key(1 << 20, 0xD0), 5)) }]
                } else {
                    vec![]
                },
                height: b.state.height,
            };
            // a replayed nullifier is a double spend: skip, state untouched
            let _ = b.insert(&sig);
        }
        b.finalize_block();
        let fresh = rebuild(&b.state);
        assert_eq!(b.state.root(), fresh.root(), "round {round}");
        assert_eq!(b.state.root_leaves(), fresh.root_leaves(), "round {round}");
    }
}

#[test]
fn v2_pruning_and_reinsertion_leave_no_trace() {
    let config = PruneConfig { max_bytes: u64::MAX, rank_floor_pct: 0, half_life_epochs: 0 };
    let mut pruned = Bbg::new().with_root_version(RootVersion::V2).with_prune_config(config);
    pruned.state.neurons.insert(p(1), NeuronRecord { focus: 1 << 40, karma: 0, stake: 0 });
    // link A survives (refreshed every epoch), link B goes stale and is pruned
    pruned.insert(&one_link(p(1), p(2), p(3), 10)).unwrap();
    pruned.insert(&one_link(p(1), p(4), p(5), 10)).unwrap();
    for _ in 0..(bbg::state::EPOCH_BLOCKS * 2) {
        pruned.insert(&one_link(p(1), p(2), p(3), 1)).unwrap();
        pruned.finalize_block();
    }
    let aid_b = bbg::state::axon_id(&p(4), &p(5));
    assert!(!pruned.state.particles.contains_key(&aid_b), "stale link pruned");
    assert!(pruned.state.axons_out.get(&p(4)).is_none());
    let fresh = rebuild(&pruned.state);
    assert_eq!(pruned.state.root(), fresh.root(), "root after prune == from-scratch root");
    // reinsert B: the index is a set, the trie forgets the removal
    pruned.insert(&one_link(p(1), p(4), p(5), 10)).unwrap();
    let fresh = rebuild(&pruned.state);
    assert_eq!(pruned.state.root(), fresh.root());
}

#[test]
fn v2_replay_reproduces_every_block_root() {
    let script = |b: &mut Bbg| {
        b.state.neurons.insert(p(1), NeuronRecord { focus: 1 << 30, karma: 0, stake: 0 });
        for i in 0..30u8 {
            b.insert(&one_link(p(1), p(10 + i % 7), p(50 + i % 5), 2 + i as u64)).unwrap();
            if i % 3 == 2 {
                b.finalize_block();
            }
        }
    };
    let mut a = Bbg::new().with_root_version(RootVersion::V2);
    let mut b = Bbg::new().with_root_version(RootVersion::V2);
    script(&mut a);
    script(&mut b);
    assert_eq!(a.state.height, 10);
    assert_eq!(a.state.time.len(), 10);
    for h in 0..10u64 {
        assert_eq!(a.state.time.get(&h), b.state.time.get(&h), "block {h} root");
    }
    assert_eq!(a.state.root(), b.state.root());
    assert_eq!(a.checkpoint.root, b.checkpoint.root);
}

#[test]
fn v2_root_is_pinned() {
    // The v2 derivation is frozen by specs/state.md; this vector pins it.
    let mut b = all_dims(RootVersion::V2);
    let before = hex(&b.state.root());
    b.finalize_block();
    let after = hex(&b.state.root());
    assert_eq!(before, "7461bb8f3e39458e1e9af283c815eeb4eec75a08471f1237b934f2fec71e3f9d");
    assert_eq!(after, "2e0a2a28fe99abc6c81b4e09670ecb3d80f758ef2499ad6dae0304be4287910c");
    let empty = Bbg::new().with_root_version(RootVersion::V2);
    assert_eq!(hex(&empty.state.root()), "515dbb7ebaf6b868442827f72c6926e79e5f11fef77ff91b47839b6e68ec375d");
}

// ── v2 proofs against dimension commitments ───────────────────────────────────

#[test]
fn v2_record_proofs_verify_against_the_dimension_leaf() {
    let b = all_dims(RootVersion::V2);
    let leaves = b.state.root_leaves();
    // particles: the target of the first link carries energy
    let key = p(20);
    let record = b.state.particles.get(&key).unwrap();
    let proof = b.state.particles.prove(&key);
    let leaf = bbg::Keyed::<[u8; 32], ParticleRecord>::leaf(&key, record);
    assert!(proof.verify(&leaves.dims[0], &key, Some(&leaf)));
    assert!(!proof.verify(&leaves.dims[1], &key, Some(&leaf)), "wrong dimension");
    // absence in particles
    let absent = p(0xEE);
    assert!(b.state.particles.prove(&absent).verify(&leaves.dims[0], &absent, None));
    // nullifiers (N)
    assert!(b.state.nullifiers.prove(&p(100)).verify(&leaves.n, &p(100), Some(&bbg::trie::leaf_digest(&p(100), &[]))));
    // axons_out: the outer leaf and a member
    let set_root = b.state.axons_out.set_root(&p(10)).unwrap();
    let degree = b.state.axons_out.get(&p(10)).unwrap().len() as u64;
    assert!(b.state.axons_out.prove(&p(10)).verify(
        &leaves.dims[1],
        &p(10),
        Some(&bbg::AxonIndex::leaf(&p(10), degree, &set_root))
    ));
    let axon = bbg::state::axon_id(&p(10), &p(30));
    assert!(b.state.axons_out.prove_member(&p(10), &axon).unwrap().verify(
        &set_root,
        &axon,
        Some(&bbg::trie::leaf_digest(&axon, &[]))
    ));
}
