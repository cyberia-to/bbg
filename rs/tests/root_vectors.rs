// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! Root derivation vectors and properties.
//!
//! Dimension version 3 (2026-10-16): v2 (keyed trie) is the default, every
//! trie leaf carries `DIMENSION_VERSION`, `NeuronRecord` carries the chain
//! `nonce`, a link without conviction writes only topology, a zero balance is
//! an absent row.
//!
//! Hash-agnostic: no digest is pinned by hand. Every expected root is
//! recomputed here by an independent reference of specs/state.md — records
//! laid out in the spec's field order, the trie built recursively from
//! `node(S)`, the flat tables serialized by hand, the 14 leaves folded with
//! hemera's permutation directly — so the vectors follow hemera (0.3 now,
//! 0.4 next) without re-pinning, and a derivation change still fails them.
//! v2 roots are also checked against from-scratch rebuilds after random
//! batches, pruning and replay — the property the incremental path exists to
//! preserve.

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
        b.state.neurons.insert(p(i), NeuronRecord { focus: 10_000, karma: i as u64, stake: 0, nonce: 7 * i as u64 });
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

// ── the reference derivation (specs/state.md), independent of the engine ───

mod reference {
    use bbg::dim::{bytes32_limbs, u64_limbs, TABLE_VERSION};
    use bbg::state::BbgState;
    use bbg::trie::{branch_digest, leaf_digest, Digest, EMPTY};
    use hemera::field::Goldilocks as H;
    use nebu::Goldilocks as G;

    type Rec = ([u8; 32], Vec<G>);

    fn u(vals: &[u64]) -> Vec<G> {
        vals.iter().flat_map(|&v| u64_limbs(v)).collect()
    }

    fn pos(v: u64) -> [u8; 32] {
        let mut k = [0u8; 32];
        k[..8].copy_from_slice(&v.to_le_bytes());
        k
    }

    fn bit(k: &[u8; 32], i: usize) -> bool {
        (k[i / 8] >> (7 - i % 8)) & 1 == 1
    }

    /// `node(S)` — the recursive definition, no incremental state.
    pub fn trie(mut leaves: Vec<([u8; 32], Digest)>) -> Digest {
        leaves.sort_by(|a, b| a.0.cmp(&b.0));
        fn node(s: &[([u8; 32], Digest)]) -> Digest {
            match s.len() {
                0 => EMPTY,
                1 => s[0].1,
                _ => {
                    let (lo, hi) = (&s[0].0, &s[s.len() - 1].0);
                    let i = (0..256).find(|&i| bit(lo, i) != bit(hi, i)).expect("distinct keys");
                    let split = s.iter().position(|(k, _)| bit(k, i)).expect("both sides");
                    branch_digest(i as u16, &node(&s[..split]), &node(&s[split..]))
                }
            }
        }
        node(&leaves)
    }

    fn trie_of(recs: Vec<Rec>) -> Digest {
        trie(recs.into_iter().map(|(k, v)| (k, leaf_digest(&k, &v))).collect())
    }

    /// The 13 dimension records in spec field order: (v2 leaf values, v1 values).
    fn records(s: &BbgState) -> [(Vec<Rec>, Vec<Rec>); 13] {
        let same = |r: Vec<Rec>| (r.clone(), r);
        let axons = |m: &std::collections::BTreeMap<[u8; 32], Vec<[u8; 32]>>| {
            let v2 = m
                .iter()
                .map(|(p, list)| {
                    let set = trie(list.iter().map(|a| (*a, leaf_digest(a, &[]))).collect());
                    let mut vals = u(&[list.len() as u64]);
                    vals.extend_from_slice(&set);
                    (*p, vals)
                })
                .collect();
            let v1 = m
                .iter()
                .map(|(p, list)| {
                    let mut vals = u(&[list.len() as u64]);
                    for a in list {
                        vals.extend_from_slice(&bytes32_limbs(a));
                    }
                    (*p, vals)
                })
                .collect();
            (v2, v1)
        };
        let cat = |parts: &[&[G]]| parts.concat();
        [
            same(s.particles.iter().map(|(k, r)| (*k, u(&[r.energy, r.pi_star, r.weight, r.s_yes, r.s_no, r.meta_score]))).collect()),
            axons(&s.axons_out),
            axons(&s.axons_in),
            same(s.neurons.iter().map(|(k, r)| (*k, u(&[r.focus, r.karma, r.stake, r.nonce]))).collect()),
            same(s.locations.iter().map(|(k, r)| (*k, u(&[r.lat as u32 as u64, r.lon as u32 as u64]))).collect()),
            same(s.coins.iter().map(|(k, r)| (*k, u(&[r.total_supply]))).collect()),
            same(s.cards.iter().map(|(k, r)| (*k, cat(&[&bytes32_limbs(&r.owner), &bytes32_limbs(&r.particle)]))).collect()),
            same(s.files.iter().map(|(k, r)| (*k, u(&[r.available as u64, r.chunk_count as u64]))).collect()),
            same(s.time.iter().map(|(h, root)| (pos(*h), bytes32_limbs(root).to_vec())).collect()),
            same(
                s.signals
                    .iter()
                    .map(|(st, r)| {
                        (
                            pos(*st),
                            cat(&[
                                &bytes32_limbs(&r.neuron),
                                &bytes32_limbs(&r.network),
                                &u(&[r.link_count as u64, r.block_height]),
                                &bytes32_limbs(&r.proof_hash),
                            ]),
                        )
                    })
                    .collect(),
            ),
            same(s.balances.iter().map(|(k, v)| (*k, u(&[*v]))).collect()),
            same(s.commitments.iter().map(|(k, v)| (*k, vec![*v])).collect()),
            (
                s.nullifiers.iter().map(|k| (*k, vec![])).collect(),
                s.nullifiers.iter().map(|k| (*k, vec![G::ONE])).collect(),
            ),
        ]
    }

    /// v1 dimension leaf: Brakedown over the hand-serialized flat table.
    fn flat(recs: Vec<Rec>) -> Digest {
        let count = 3 + recs.iter().map(|(_, v)| 8 + v.len()).sum::<usize>();
        let mut fields = vec![G::new(TABLE_VERSION), G::new(count as u64), G::new(recs.len() as u64)];
        for (k, v) in &recs {
            fields.extend(bytes32_limbs(k));
            fields.extend_from_slice(v);
        }
        fields.resize(fields.len().next_power_of_two(), G::ZERO);
        use lens::Lens;
        let c = lens::brakedown::Brakedown::commit(&lens::MultilinearPoly::new(fields));
        let mut b = [0u8; 32];
        b.copy_from_slice(&c.as_bytes()[..32]);
        bbg::dim::goldilocks_from_bytes32(&b)
    }

    /// compress4 from hemera's permutation: the first 4 limbs of P([a ‖ b ‖ 0⁸]).
    fn compress(a: &Digest, b: &Digest) -> Digest {
        let mut st = [H::ZERO; 16];
        for i in 0..4 {
            st[i] = H::new(a[i].as_u64());
            st[4 + i] = H::new(b[i].as_u64());
        }
        hemera::permutation::permute(&mut st);
        std::array::from_fn(|i| G::new(st[i].as_canonical_u64()))
    }

    /// The root of `s` under `trie` (v2) or flat (v1) dimension leaves.
    pub fn root(s: &BbgState, trie_leaves: bool) -> [u8; 32] {
        let leaves: Vec<Digest> = records(s)
            .into_iter()
            .map(|(v2, v1)| if trie_leaves { trie_of(v2) } else { flat(v1) })
            .collect();
        let stats = bbg::dim::goldilocks_from_bytes32(&s.statistics().commit());
        let iv = [G::new(u64::from_le_bytes(*b"bbg-root")), G::ZERO, G::ZERO, G::ZERO];
        let acc = leaves.iter().chain(std::iter::once(&stats)).fold(iv, |acc, l| compress(&acc, l));
        let mut out = [0u8; 32];
        for (i, l) in acc.iter().enumerate() {
            out[i * 8..i * 8 + 8].copy_from_slice(&l.as_u64().to_le_bytes());
        }
        out
    }
}

fn check(b: &Bbg) {
    let trie = b.state.root_version() == RootVersion::V2;
    assert_eq!(hex(&b.state.root()), hex(&reference::root(&b.state, trie)), "{:?}", b.state.root_version());
}

// ── both root versions against the reference ─────────────────────────────────

#[test]
fn empty_roots_match_the_reference() {
    for version in [RootVersion::V1, RootVersion::V2] {
        check(&Bbg::new().with_root_version(version));
    }
}

#[test]
fn one_link_roots_match_the_reference() {
    for version in [RootVersion::V1, RootVersion::V2] {
        let mut b = Bbg::new().with_root_version(version);
        b.state.neurons.insert(p(1), NeuronRecord { focus: 1000, karma: 2, stake: 3, nonce: 4 });
        b.insert(&one_link(p(1), p(2), p(3), 7)).unwrap();
        check(&b);
        b.finalize_block();
        check(&b);
    }
}

#[test]
fn all_dimension_roots_match_the_reference() {
    for version in [RootVersion::V1, RootVersion::V2] {
        let mut b = all_dims(version);
        check(&b);
        b.finalize_block();
        check(&b);
    }
}

#[test]
fn the_reference_trie_is_the_engine_trie_on_boundary_and_random_keys() {
    let mut rng = Rng(0xF00D);
    let mut keys: Vec<[u8; 32]> = (0..300).map(|_| rng.key(1 << 40, 0x11)).collect();
    keys.extend([[0u8; 32], [0xFF; 32]]);
    for b in 0..256usize {
        let mut k = [0u8; 32];
        k[b / 8] = 0x80 >> (b % 8);
        keys.push(k);
    }
    keys.sort();
    keys.dedup();
    let entries: Vec<([u8; 32], bbg::trie::Digest)> =
        keys.iter().map(|k| (*k, bbg::trie::leaf_digest(k, &[]))).collect();
    let mut t = bbg::Trie::from_entries(entries.iter().rev().copied());
    assert_eq!(t.root(), reference::trie(entries));
}

#[test]
fn v2_is_the_default_and_v1_differs() {
    assert_eq!(BbgState::new().root_version(), RootVersion::V2);
    assert_ne!(all_dims(RootVersion::V1).state.root(), all_dims(RootVersion::V2).state.root());
    // The version is a view of the records: flipping it flips the root, and
    // the flipped root is the root a state built under that version carries.
    // (Block finalization records the root in `time`, so states that finalized
    // blocks under different versions hold different records — compare before
    // any block is finalized.)
    let build = |version| {
        let mut b = Bbg::new().with_root_version(version);
        b.state.neurons.insert(p(1), NeuronRecord { focus: 1000, karma: 2, stake: 3, nonce: 0 });
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
    for (k, v) in src.locations.iter() {
        s.locations.insert(*k, LocationRecord { ..*v });
    }
    for (k, v) in src.coins.iter() {
        s.coins.insert(*k, CoinRecord { ..*v });
    }
    for (k, v) in src.cards.iter() {
        s.cards.insert(*k, CardRecord { ..*v });
    }
    for (k, v) in src.files.iter() {
        s.files.insert(*k, FileRecord { ..*v });
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
        b.state.neurons.insert(p(i), NeuronRecord { focus: u64::MAX / 4, karma: 0, stake: 0, nonce: 0 });
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
    pruned.state.neurons.insert(p(1), NeuronRecord { focus: 1 << 40, karma: 0, stake: 0, nonce: 0 });
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
        b.state.neurons.insert(p(1), NeuronRecord { focus: 1 << 30, karma: 0, stake: 0, nonce: 0 });
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

// ── dimension version 3 engine rules ─────────────────────────────────────────

#[test]
fn the_nonce_is_under_the_root() {
    for version in [RootVersion::V1, RootVersion::V2] {
        let build = |nonce| {
            let mut s = BbgState::new();
            s.set_root_version(version);
            s.neurons.insert(p(1), NeuronRecord { focus: 0, karma: 0, stake: 0, nonce });
            s.root()
        };
        assert_ne!(build(0), build(1), "{version:?}");
        let mut s = BbgState::new();
        s.set_root_version(version);
        s.neurons.insert(p(1), NeuronRecord { focus: 0, karma: 0, stake: 0, nonce: 0 });
        s.neurons.get_mut(&p(1)).unwrap().nonce += 1;
        assert_eq!(s.root(), build(1), "{version:?}: the guard re-hashes the nonce");
    }
}

/// Recovery: a node replays committed blocks from scratch and checks every
/// stored block root (`time[h]`) before readiness. A long random run with
/// removals (pruning off, zero balances dropped, conviction-free links) must
/// reproduce every stored root bit for bit.
#[test]
fn recovery_replay_reproduces_every_stored_root() {
    let run = |b: &mut Bbg| {
        let mut rng = Rng(0xC0FFEE);
        for i in 0..6u8 {
            b.state.neurons.insert(p(i), NeuronRecord { focus: u64::MAX / 4, karma: 0, stake: 0, nonce: 0 });
        }
        for _block in 0..60 {
            for _ in 0..(1 + rng.next() % 30) {
                let neuron = p((rng.next() % 6) as u8);
                b.state.neurons.get_mut(&neuron).unwrap().nonce += 1;
                let sig = Signal {
                    neuron,
                    links: vec![Cyberlink {
                        from: rng.key(80, 0xA0),
                        to: rng.key(80, 0xB0),
                        token: p((rng.next() % 2) as u8),
                        amount: if rng.next().is_multiple_of(2) { 0 } else { 1 + rng.next() % 9 },
                        valence: 1,
                    }],
                    box_moves: vec![],
                    height: b.state.height,
                };
                b.insert(&sig).unwrap();
            }
            b.finalize_block();
        }
    };
    let mut live = Bbg::new();
    run(&mut live);
    let mut recovered = Bbg::new();
    run(&mut recovered);
    assert_eq!(live.state.time.len(), 60);
    for (h, root) in live.state.time.iter() {
        assert_eq!(recovered.state.time.get(h), Some(root), "stored root at {h}");
    }
    assert_eq!(live.state.root(), recovered.state.root());
    assert_eq!(rebuild(&live.state).root(), live.state.root(), "and a from-scratch rebuild agrees");
}

// ── conviction-free links and zero rows (since dimension version 2) ──────────

#[test]
fn a_link_without_conviction_writes_only_topology() {
    let mut b = Bbg::new();
    b.state.neurons.insert(p(1), NeuronRecord { focus: 100, karma: 0, stake: 0, nonce: 0 });
    b.state.particles.insert(p(3), ParticleRecord::zero());
    b.insert(&one_link(p(1), p(2), p(3), 0)).unwrap();
    let axon = bbg::state::axon_id(&p(2), &p(3));
    assert_eq!(b.state.axons_out.get(&p(2)).unwrap(), &vec![axon]);
    assert_eq!(b.state.axons_in.get(&p(3)).unwrap(), &vec![axon]);
    assert_eq!(b.state.axon_edges.get(&axon), Some(&(p(2), p(3))));
    assert!(!b.state.particles.contains_key(&axon), "no axon-particle");
    assert_eq!(b.state.particles.len(), 1, "no target record created");
    assert!(b.state.balances.is_empty(), "no balance rows");
    assert_eq!(b.state.neurons.get(&p(1)).unwrap().focus, 100);
    // with conviction the same link writes the full effect
    b.insert(&one_link(p(1), p(2), p(3), 5)).unwrap();
    assert_eq!(b.state.particles.get(&axon).unwrap().weight, 5);
    assert_eq!(b.state.particles.get(&p(3)).unwrap().energy, 5);
    assert_eq!(b.state.neurons.get(&p(1)).unwrap().focus, 95);
}

#[test]
fn a_zero_balance_is_an_absent_row() {
    let mut b = Bbg::new();
    b.state.neurons.insert(p(1), NeuronRecord { focus: 100, karma: 0, stake: 0, nonce: 0 });
    b.insert(&one_link(p(1), p(2), p(3), 5)).unwrap();
    let from = bbg::balance_key(&p(2), &p(0));
    let to = bbg::balance_key(&p(3), &p(0));
    assert!(!b.state.balances.contains_key(&from), "spending from nothing leaves no row");
    assert_eq!(b.state.balances.get(&to), Some(&5));
    // spend it all back: the row disappears, the root equals a state that never had it
    b.insert(&one_link(p(1), p(3), p(2), 5)).unwrap();
    assert!(!b.state.balances.contains_key(&to));
    assert_eq!(b.state.balances.get(&from), Some(&5));
    let fresh = rebuild(&b.state);
    assert_eq!(b.state.root(), fresh.root());
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

/// A bulk load (one root read after 20k touched records: the dimensions hash
/// on parallel threads) commits exactly what the same writes commit when the
/// root is read every 97 writes (single thread, partial flushes).
#[test]
fn bulk_and_incremental_commitment_reads_agree() {
    let script = |b: &mut Bbg, every: Option<usize>| {
        let mut rng = Rng(0xB01C);
        for i in 0..12_000usize {
            let key = rng.key(4_000, 0xE0);
            let to = rng.key(4_000, 0xF0);
            let amount = rng.next() % 3;
            b.state.neurons.insert(key, NeuronRecord { focus: u64::MAX / 4, karma: 0, stake: 0, nonce: i as u64 });
            b.insert(&one_link(key, key, to, amount)).unwrap();
            if every.is_some_and(|e| i % e == 0) {
                b.state.root();
            }
        }
    };
    let mut bulk = Bbg::new();
    script(&mut bulk, None);
    let mut steady = Bbg::new();
    script(&mut steady, Some(97));
    assert_eq!(bulk.state.root(), steady.state.root());
    assert_eq!(bulk.state.root(), rebuild(&steady.state).root());
}
