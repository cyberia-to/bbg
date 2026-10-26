// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! Positions — dimension version 4 (`specs/state.md` rules 8–10; soft3
//! `specs/chain-economy.md` §3).
//!
//! The kernel-side operations (lock, unlock, payout owner) are modelled here
//! the way chain-economy §3.3–§3.5 writes them through bbg: the card record
//! in `cards`, the stake at `stake_key(card)`, the pool at `focus_key(τ)`,
//! topology through `insert` with amount 0. Every property is checked against
//! an independent model or a from-scratch replay.

use std::collections::BTreeMap;

use bbg::state::{axon_id, balance_key};
use bbg::transition::NativeChange;
use bbg::{
    Bbg, BbgState, CardRecord, Cards, Cyberlink, Particle, RootVersion, Side, Signal, SignalRecord, SideSums,
    Valence, card_id, focus_key, stake_key,
};

fn p(seed: u8) -> [u8; 32] {
    [seed; 32]
}

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
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u64) as usize]
    }
    fn valence(&mut self) -> Valence {
        self.pick(&[Valence::Against, Valence::Neutral, Valence::For])
    }
}

const TOKEN: [u8; 32] = [0xAA; 32];

/// The model of the cards dimension and the stakes: card → (record, stake).
type Model = BTreeMap<Particle, (CardRecord, u64)>;

fn set_stake(s: &mut BbgState, card: &Particle, stake: u64) {
    // a zero balance is an absent row (rule 4)
    if stake == 0 {
        s.balances.remove(&stake_key(card));
    } else {
        s.balances.insert(stake_key(card), stake);
    }
}

/// Kind 2 with `amount > 0` (chain-economy §3.3): create or increase a
/// position. Returns false (and touches nothing) on a valence mismatch.
#[allow(clippy::too_many_arguments)]
fn lock(b: &mut Bbg, m: &mut Model, author: Particle, from: Particle, to: Particle, valence: Valence, amount: u64, height: u64) -> bool {
    let card = card_id(&author, &from, &to);
    let record = match b.state.cards.get(&card) {
        Some(r) if r.valence != valence => return false,
        Some(r) => CardRecord { since: height, ..*r },
        None => CardRecord { owner: author, particle: axon_id(&from, &to), author, valence, since: height },
    };
    let stake = b.state.stake(&card) + amount;
    b.state.cards.insert(card, record);
    set_stake(&mut b.state, &card, stake);
    let topology = Signal {
        neuron: author,
        links: vec![Cyberlink { from, to, token: TOKEN, amount: 0, valence: valence.to_i8() }],
        box_moves: vec![],
        height,
    };
    b.insert(&topology).unwrap();
    m.insert(card, (record, stake));
    true
}

/// Kind 6 (chain-economy §3.4): partial or full unlock; the card goes at zero.
fn unlock(b: &mut Bbg, m: &mut Model, card: Particle, amount: u64) {
    let stake = b.state.stake(&card) - amount;
    set_stake(&mut b.state, &card, stake);
    if stake == 0 {
        b.state.cards.remove(&card);
        m.remove(&card);
    } else {
        m.get_mut(&card).unwrap().1 = stake;
    }
}

/// Kind 7 (chain-economy §3.5): re-point the payout owner.
fn payout(b: &mut Bbg, m: &mut Model, card: Particle, owner: Particle) {
    let record = CardRecord { owner, ..b.state.cards[&card] };
    b.state.cards.insert(card, record);
    m.get_mut(&card).unwrap().0 = record;
}

/// One random kernel step over a small universe of authors and particles.
fn step(b: &mut Bbg, m: &mut Model, rng: &mut Rng, height: u64) {
    let cards: Vec<Particle> = m.keys().copied().collect();
    match rng.below(10) {
        0..=5 => {
            let author = p(1 + rng.below(5) as u8);
            let (from, to) = (p(20 + rng.below(4) as u8), p(30 + rng.below(4) as u8));
            let valence = rng.valence();
            lock(b, m, author, from, to, valence, 1 + rng.below(1_000), height);
        }
        6..=7 if !cards.is_empty() => {
            let card = rng.pick(&cards);
            let stake = m[&card].1;
            let amount = if rng.below(2) == 0 { stake } else { 1 + rng.below(stake) };
            unlock(b, m, card, amount);
        }
        8 if !cards.is_empty() => {
            let card = rng.pick(&cards);
            payout(b, m, card, p(100 + rng.below(3) as u8));
        }
        _ => {
            // the pool accrues (chain-economy §6.2)
            let pool = b.state.focus_pool(&TOKEN) + 1 + rng.below(50);
            b.state.balances.insert(focus_key(&TOKEN), pool);
        }
    }
}

/// The model's positions grouped by axon, cards ascending: (axon, sums, [(card, record, stake)]).
#[allow(clippy::type_complexity)]
fn grouped(m: &Model) -> Vec<(Particle, SideSums, Vec<(Particle, CardRecord, u64)>)> {
    let mut by_axon: BTreeMap<Particle, Vec<(Particle, CardRecord, u64)>> = BTreeMap::new();
    for (card, (record, stake)) in m {
        by_axon.entry(record.particle).or_default().push((*card, *record, *stake));
    }
    by_axon
        .into_iter()
        .map(|(axon, list)| {
            let mut sums = SideSums::default();
            for (_, r, s) in &list {
                match r.valence.to_i8() {
                    -1 => sums.minus += *s as u128,
                    _ => sums.plus += *s as u128,
                }
            }
            (axon, sums, list)
        })
        .collect()
}

fn check_against_model(s: &BbgState, m: &Model) {
    let snap = s.positions();
    let want = grouped(m);
    assert_eq!(snap.axons().len(), want.len(), "axon count");
    assert_eq!(s.cards.axon_count(), want.len());
    assert_eq!(snap.len(), m.len());
    assert_eq!(snap.total_stake(), m.values().map(|(_, s)| *s as u128).sum::<u128>());
    assert_eq!(
        snap.active_stake(),
        m.values().filter(|(r, _)| r.valence != Valence::Neutral).map(|(_, s)| *s as u128).sum::<u128>()
    );
    for (got, (axon, sums, list)) in snap.axons().iter().zip(&want) {
        assert_eq!(&got.axon, axon);
        assert_eq!(&got.sums, sums, "side sums of an axon");
        assert_eq!(&s.side_sums(axon), sums, "side_sums == snapshot sums");
        let got_list: Vec<_> = got.positions.iter().map(|p| (p.card, p.record, p.stake)).collect();
        assert_eq!(&got_list, list, "positions of an axon, card ascending");
        let on_axon: Vec<_> = s.cards.on_axon(axon).map(|(c, r)| (*c, *r)).collect();
        assert_eq!(on_axon, list.iter().map(|(c, r, _)| (*c, *r)).collect::<Vec<_>>());
        for (card, record, stake) in list {
            let found = snap.find(axon, card).expect("find");
            assert_eq!((found.record, found.stake), (*record, *stake));
            assert_eq!(s.stake(card), *stake);
            assert!(*stake > 0, "a card exists exactly while its stake is positive");
        }
    }
    assert_eq!(s.side_sums(&p(0xEE)), SideSums::default(), "an axon without cards");
    assert!(snap.get(&p(0xEE)).is_none());
}

// ── identities and the record ────────────────────────────────────────────────

/// The tags of chain-economy §3.1 byte for byte, and the identities as hemera
/// over the tagged preimage (hash-agnostic: hemera 0.3 here, 0.4 in the chain).
#[test]
fn identities_are_the_tagged_hemera_preimages_of_chain_economy() {
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    assert_eq!(hex(bbg::D_CARD), "736f6674332f636861696e2f636172642f763100");
    assert_eq!(hex(bbg::D_STAKE), "736f6674332f636861696e2f7374616b652f763100");
    assert_eq!(hex(bbg::D_FOCUS), "736f6674332f636861696e2f666f6375732f763100");
    let h = |parts: &[&[u8]]| -> [u8; 32] { hemera::hash(&parts.concat()).as_bytes()[..32].try_into().unwrap() };
    let (author, from, to) = (p(0x66), p(1), p(2));
    let card = card_id(&author, &from, &to);
    assert_eq!(card, h(&[bbg::D_CARD, &author, &from, &to]));
    assert_eq!(stake_key(&card), h(&[bbg::D_STAKE, &card]));
    assert_eq!(focus_key(&TOKEN), h(&[bbg::D_FOCUS, &TOKEN]));
    // every input byte binds
    assert_ne!(card_id(&author, &to, &from), card);
    assert_ne!(card_id(&p(0x67), &from, &to), card);
    assert_ne!(stake_key(&card), focus_key(&card), "the two tags separate");
    assert_ne!(stake_key(&card), balance_key(&card, &TOKEN));
}

#[test]
fn valence_bytes_round_trip_and_reject_everything_else() {
    for v in i8::MIN..=i8::MAX {
        match Valence::from_i8(v) {
            Some(val) => {
                assert!((-1..=1).contains(&v));
                assert_eq!(val.to_i8(), v);
                assert_eq!(val.byte() as i16, v as i16 + 1, "stored byte is v + 1");
                assert_eq!(Valence::from_byte(val.byte()), Some(val));
                assert_eq!(val.side(), if v == -1 { Side::Minus } else { Side::Plus });
                assert_eq!(val.is_active(), v != 0);
            }
            None => assert!(!(-1..=1).contains(&v)),
        }
    }
    for b in 0..=u8::MAX {
        assert_eq!(Valence::from_byte(b).is_some(), b <= 2);
    }
}

fn sample_card() -> (Particle, CardRecord) {
    let card = card_id(&p(3), &p(4), &p(5));
    (card, CardRecord { owner: p(6), particle: axon_id(&p(4), &p(5)), author: p(3), valence: Valence::Against, since: 0x0102_0304_0506_0708 })
}

/// The native record (domain 7) decodes to the record it encodes; the trie
/// leaf and the v1 table bind every field; the record is provable under the root.
#[test]
fn card_record_round_trips_through_every_encoding() {
    let (card, record) = sample_card();
    let mut b = Bbg::new();
    b.state.cards.insert(card, record);
    // native record: owner 32 ‖ particle 32 ‖ author 32 ‖ valence u8 ‖ since u64 LE
    let native = bbg::transition::records(&b)
        .unwrap()
        .find(|r| r.key[0] == 7)
        .expect("a card record");
    assert_eq!(native.key[1..], card);
    let v = &native.value;
    assert_eq!(v.len(), 105);
    let decoded = CardRecord {
        owner: v[0..32].try_into().unwrap(),
        particle: v[32..64].try_into().unwrap(),
        author: v[64..96].try_into().unwrap(),
        valence: Valence::from_byte(v[96]).unwrap(),
        since: u64::from_le_bytes(v[97..105].try_into().unwrap()),
    };
    assert_eq!(decoded, record);

    // v2: the trie path verifies the record's leaf under the cards leaf of the root
    let leaves = b.state.root_leaves();
    let leaf = Cards::leaf(&card, &record);
    assert!(b.state.cards.prove(&card).verify(&leaves.dims[6], &card, Some(&leaf)));
    let variants = [
        CardRecord { owner: p(7), ..record },
        CardRecord { particle: p(7), ..record },
        CardRecord { author: p(7), ..record },
        CardRecord { valence: Valence::Neutral, ..record },
        CardRecord { since: record.since + 1, ..record },
    ];
    for (i, other) in variants.iter().enumerate() {
        assert_ne!(Cards::leaf(&card, other), leaf, "field {i} binds the leaf");
        let mut s = BbgState::new();
        s.cards.insert(card, *other);
        assert_ne!(s.root(), b.state.root(), "field {i} binds the root");
    }

    // v1: the flat table entry is 28 limbs and authenticates per cell
    let mut v1 = BbgState::new();
    v1.set_root_version(RootVersion::V1);
    v1.cards.insert(card, record);
    let proof = bbg::bbg_query(&v1, bbg::Dim::Cards, &card).unwrap();
    assert!(bbg::verify_entity(&proof, &v1.root(), bbg::Dim::Cards, &card));
    let cert = bbg::certificate::StateCertificate::from_state(&v1, &[bbg::Dim::Cards as u64]).unwrap();
    let cells: Vec<u64> = (0..28).map(|i| cert.cell(bbg::Dim::Cards as u64, 11 + i).unwrap()).collect();
    let limbs = |b: &[u8; 32]| (0..8).map(|i| u32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap()) as u64).collect::<Vec<_>>();
    let mut want = limbs(&record.owner);
    want.extend(limbs(&record.particle));
    want.extend(limbs(&record.author));
    want.extend([record.valence.byte() as u64, 0, record.since & 0xFFFF_FFFF, record.since >> 32]);
    assert_eq!(cells, want);
    assert_eq!(cert.cell(bbg::Dim::Cards as u64, 11 + 28), None, "nothing past the last field");
}

// ── determinism ──────────────────────────────────────────────────────────────

/// The cards dimension, the stakes and the index are a function of the final
/// position set: arrival order, replaced intermediate records, removed decoys
/// and a moved axon leave no trace — under both root versions.
#[test]
fn root_and_snapshot_are_independent_of_arrival_order() {
    let mut rng = Rng(0xCA4D);
    let mut target: Vec<(Particle, CardRecord, u64)> = (0..60u8)
        .map(|i| {
            let (author, from, to) = (p(1 + i % 7), p(20 + i % 5), p(40 + i % 3));
            let card = card_id(&author, &from, &to);
            let record = CardRecord { owner: p(90 + i % 4), particle: axon_id(&from, &to), author, valence: rng.valence(), since: rng.below(1 << 40) };
            (card, record, 1 + rng.below(1 << 50))
        })
        .map(|(card, record, stake)| (card, (record, stake)))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .map(|(card, (record, stake))| (card, record, stake))
        .collect();
    let build = |order: &[(Particle, CardRecord, u64)], version: RootVersion, rng: &mut Rng| {
        let mut s = BbgState::new();
        s.set_root_version(version);
        for (card, record, stake) in order {
            // an intermediate record on another axon first: the index must move it
            s.cards.insert(*card, CardRecord { particle: p(0xF0), valence: Valence::For, ..*record });
            if rng.below(3) == 0 {
                let decoy = card_id(&p(0xDD), card, card);
                s.cards.insert(decoy, *record);
                s.balances.insert(stake_key(&decoy), 5);
            }
            s.cards.insert(*card, *record);
            s.balances.insert(stake_key(card), *stake);
        }
        for (card, record, _) in order {
            let decoy = card_id(&p(0xDD), card, card);
            if s.cards.remove(&decoy).is_some() {
                assert_eq!(s.cards.get(card), Some(record));
                s.balances.remove(&stake_key(&decoy));
            }
        }
        s
    };
    for version in [RootVersion::V1, RootVersion::V2] {
        let reference = build(&target, version, &mut rng);
        let (root, snap) = (reference.root(), reference.positions());
        assert_eq!(snap.len(), target.len());
        assert_eq!(reference.cards.axon_count(), snap.axons().len());
        for round in 0..5 {
            if round == 4 {
                target.reverse();
            } else {
                for i in (1..target.len()).rev() {
                    let j = rng.below(i as u64 + 1) as usize;
                    target.swap(i, j);
                }
            }
            let other = build(&target, version, &mut rng);
            assert_eq!(other.root(), root, "{version:?} round {round}: root");
            assert_eq!(other.positions(), snap, "{version:?} round {round}: snapshot");
        }
    }
}

// ── enumeration per edge vs a model ──────────────────────────────────────────

/// Locks, partial and full unlocks, payout changes and pool accruals at
/// random: after every block the snapshot, the side sums, `on_axon`, `find`,
/// the totals and the stakes equal the model; a snapshot taken at a boundary
/// is unaffected by everything after it.
#[test]
fn enumeration_per_edge_matches_the_model() {
    let mut rng = Rng(0x0E0E);
    let mut b = Bbg::new();
    let mut m = Model::new();
    let mut boundaries = Vec::new();
    for block in 1..=120u64 {
        for _ in 0..(1 + rng.below(12)) {
            step(&mut b, &mut m, &mut rng, block);
        }
        b.finalize_block();
        check_against_model(&b.state, &m);
        if block % 24 == 0 {
            boundaries.push((b.state.positions(), m.clone()));
        }
    }
    assert!(m.len() > 10, "the run holds positions at the end ({})", m.len());
    assert!(m.values().any(|(r, _)| r.valence == Valence::Against) && m.values().any(|(r, _)| r.valence != Valence::Against));
    for (snap, model) in &boundaries {
        let mut s = BbgState::new();
        for (card, (record, stake)) in model {
            s.cards.insert(*card, *record);
            s.balances.insert(stake_key(card), *stake);
        }
        assert_eq!(&s.positions(), snap, "a boundary snapshot is frozen");
    }
}

/// A lock with the opposite valence on a live card is the kernel's to reject;
/// bbg's index follows a record that changes axon and drops emptied axons.
#[test]
fn the_index_follows_moves_and_drops_empty_axons() {
    let mut s = BbgState::new();
    let (card, record) = sample_card();
    let other_axon = axon_id(&p(8), &p(9));
    s.cards.insert(card, record);
    assert_eq!(s.cards.axon_count(), 1);
    assert_eq!(s.cards.on_axon(&record.particle).count(), 1);
    s.cards.insert(card, CardRecord { particle: other_axon, ..record });
    assert_eq!(s.cards.axon_count(), 1);
    assert_eq!(s.cards.on_axon(&record.particle).count(), 0);
    assert_eq!(s.cards.on_axon(&other_axon).count(), 1);
    assert_eq!(s.cards.remove(&card).map(|r| r.particle), Some(other_axon));
    assert_eq!(s.cards.axon_count(), 0);
    assert!(s.cards.remove(&card).is_none());
    assert!(s.positions().is_empty());
    assert_eq!(s.root(), BbgState::new().root(), "an emptied dimension is the empty dimension");
}

// ── recovery ─────────────────────────────────────────────────────────────────

fn run_chain(seed: u64, blocks: u64) -> (Bbg, Model) {
    let mut rng = Rng(seed);
    let mut b = Bbg::new();
    let mut m = Model::new();
    for block in 1..=blocks {
        for _ in 0..(1 + rng.below(10)) {
            step(&mut b, &mut m, &mut rng, block);
        }
        b.finalize_block();
    }
    (b, m)
}

/// A node replays committed blocks from genesis and checks every stored block
/// root `time[h]`; positions, stakes and the pool reproduce bit for bit, and
/// the axon index rebuilt from the persisted card records equals the live one.
#[test]
fn recovery_replay_reproduces_every_stored_root_and_the_index() {
    let (live, model) = run_chain(0xBEEF, 80);
    let (replayed, _) = run_chain(0xBEEF, 80);
    assert_eq!(live.state.time.len(), 80);
    for (h, root) in live.state.time.iter() {
        assert_eq!(replayed.state.time.get(h), Some(root), "stored root at {h}");
    }
    assert_eq!(live.state.root(), replayed.state.root());
    assert!(live.state.focus_pool(&TOKEN) > 0);
    check_against_model(&replayed.state, &model);

    // rebuild the cards dimension and the balances from the persisted stream
    let mut cards = BbgState::new();
    for r in bbg::transition::records(&live).unwrap() {
        let entity: [u8; 32] = match r.key.get(1..).and_then(|k| k.try_into().ok()) {
            Some(k) => k,
            None => continue,
        };
        let v = &r.value;
        match r.key[0] {
            7 => {
                cards.cards.insert(
                    entity,
                    CardRecord {
                        owner: v[0..32].try_into().unwrap(),
                        particle: v[32..64].try_into().unwrap(),
                        author: v[64..96].try_into().unwrap(),
                        valence: Valence::from_byte(v[96]).unwrap(),
                        since: u64::from_le_bytes(v[97..105].try_into().unwrap()),
                    },
                );
            }
            13 => {
                cards.balances.insert(entity, u64::from_le_bytes(v[..8].try_into().unwrap()));
            }
            _ => {}
        }
    }
    assert_eq!(cards.positions(), live.state.positions());
    assert_eq!(cards.cards.commitment(), live.state.cards.commitment());
    assert_eq!(cards.balances.commitment(), live.state.balances.commitment());
}

// ── the reserved keys ────────────────────────────────────────────────────────

/// No bbg transfer path moves a stake or the pool. Random links with
/// conviction (`insert`, and the native signal path) and native local credits
/// run with every owner, target and token drawn from a pool that contains the
/// card ids, the stake keys, the pool key and the token themselves; after each
/// step every reserved balance and every card is exactly as before, and the
/// transfers did move ordinary balances.
#[test]
fn transfer_paths_never_move_a_stake_or_the_pool() {
    let mut b = Bbg::new();
    let mut m = Model::new();
    let mut rng = Rng(0x57A4E);
    for _ in 0..40 {
        step(&mut b, &mut m, &mut rng, 1);
    }
    let pool = focus_key(&TOKEN);
    b.state.balances.insert(pool, 1_000_000);
    let reserved: Vec<[u8; 32]> = m.keys().map(stake_key).chain([pool, focus_key(&p(0))]).collect();
    let mut ids: Vec<[u8; 32]> = m.keys().copied().collect();
    ids.extend(reserved.iter().copied());
    ids.extend([TOKEN, p(0), p(1), p(2), p(20), p(30)]);
    let snapshot = |s: &BbgState| -> (Vec<Option<u64>>, Vec<(Particle, CardRecord)>) {
        (reserved.iter().map(|k| s.balances.get(k).copied()).collect(), s.cards.iter().map(|(c, r)| (*c, *r)).collect())
    };
    let before = snapshot(&b.state);
    let ordinary_before = b.state.balances.len();
    let neuron = p(1);
    b.state.neurons.insert(neuron, bbg::NeuronRecord { focus: u64::MAX / 2, karma: 0, stake: 0, nonce: 0 });
    for i in 0..400u64 {
        let (from, to, token) = (rng.pick(&ids), rng.pick(&ids), rng.pick(&ids));
        let amount = 1 + rng.below(1 << 20);
        let link = Cyberlink { from, to, token, amount, valence: 1 };
        match i % 3 {
            0 => {
                b.insert(&Signal { neuron, links: vec![link], box_moves: vec![], height: i }).unwrap();
            }
            1 => {
                let signal = Signal { neuron, links: vec![link], box_moves: vec![], height: i };
                let header = SignalRecord { neuron, network: p(9), link_count: 1, block_height: i, proof_hash: p(8), step: i };
                let prepared = b.prepare_native_batch(&[NativeChange::Signal { signal: &signal, header: &header }]).unwrap();
                if rng.below(2) == 0 {
                    prepared.publish();
                } // else dropped: restored
            }
            _ => {
                let (who, token) = (rng.pick(&ids), rng.pick(&ids));
                let prepared = b
                    .prepare_native_batch(&[NativeChange::LocalCredit { neuron: &who, token: &token, amount, focus: 0 }])
                    .unwrap();
                prepared.publish();
            }
        }
        assert_eq!(snapshot(&b.state), before, "step {i}: a reserved balance or a card moved");
    }
    assert!(b.state.balances.len() > ordinary_before + 100, "the transfers wrote ordinary balances");
    // structurally: no owner/token pair of the pool addresses a reserved key
    for o in &ids {
        for t in &ids {
            assert!(!reserved.contains(&balance_key(o, t)));
        }
    }
    check_against_model(&b.state, &m);
}
