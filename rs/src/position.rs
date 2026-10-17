// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Positions: staked cyberlinks (dimension version 4, `specs/state.md`
//! rules 8–10; soft3 `specs/chain-economy.md` §3).
//!
//! A position is one card — one author's locked stake on one axon. The card
//! record lives in the cards dimension at [`card_id`]; its stake is the native
//! balance at [`stake_key`], and a chain's focus pool is the native balance at
//! [`focus_key`]. Both are 53-byte hemera preimages, so no `balance_key`
//! (64 bytes) — the only key any bbg transfer path writes — can reach them.
//!
//! [`Cards`] keeps, beside the committed map, an uncommitted index
//! `axon → {card → stake_key(card)}` so the chain's epoch snapshot
//! ([`BbgState::positions`]) enumerates positions by axon in O(cards) with no
//! hash per card.

use std::collections::BTreeMap;
use std::ops::Deref;

use hemera::hash as hemera_hash;
use lens::Commitment;

use crate::keyed::Keyed;
use crate::state::BbgState;
use crate::trie::{Digest, TrieProof};
use crate::types::{CardRecord, NeuronId, Particle, Side};

/// Domain tag of [`card_id`] (chain-economy §3.1).
pub const D_CARD: &[u8; 20] = b"soft3/chain/card/v1\0";
/// Domain tag of [`stake_key`].
pub const D_STAKE: &[u8; 21] = b"soft3/chain/stake/v1\0";
/// Domain tag of [`focus_key`].
pub const D_FOCUS: &[u8; 21] = b"soft3/chain/focus/v1\0";

fn tagged(tag: &[u8], parts: &[&[u8; 32]]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(tag.len() + 32 * parts.len());
    buf.extend_from_slice(tag);
    for part in parts {
        buf.extend_from_slice(*part);
    }
    let h = hemera_hash(&buf);
    let b = h.as_bytes();
    let mut out = [0u8; 32];
    out[..b.len().min(32)].copy_from_slice(&b[..b.len().min(32)]);
    out
}

/// The card id of `author`'s position on the axon `(p, q)`:
/// `H(D_CARD ‖ author ‖ p ‖ q)`, 116 bytes in.
pub fn card_id(author: &NeuronId, p: &Particle, q: &Particle) -> Particle {
    tagged(D_CARD, &[author, p, q])
}

/// The balance key holding a position's stake: `H(D_STAKE ‖ card)`, 53 bytes in.
pub fn stake_key(card: &Particle) -> [u8; 32] {
    tagged(D_STAKE, &[card])
}

/// The balance key holding the focus pool of `token`: `H(D_FOCUS ‖ token)`,
/// 53 bytes in.
pub fn focus_key(token: &Particle) -> [u8; 32] {
    tagged(D_FOCUS, &[token])
}

/// The cards dimension: the committed `card → CardRecord` map and the
/// uncommitted axon index (rule 10). Reads go through `Deref` to the map;
/// writes go through [`Self::insert`] / [`Self::remove`], which keep the
/// trie and the index current. There is no in-place mutation: a record is
/// `Copy`, so a writer reads it, changes it and inserts it back.
#[derive(Default)]
pub struct Cards {
    records: Keyed<Particle, CardRecord>,
    /// axon → card → stake_key(card)
    by_axon: BTreeMap<Particle, BTreeMap<Particle, [u8; 32]>>,
}

impl Deref for Cards {
    type Target = BTreeMap<Particle, CardRecord>;
    fn deref(&self) -> &BTreeMap<Particle, CardRecord> {
        &self.records
    }
}

impl Cards {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace the record of `card`; returns the previous record.
    /// A changed `particle` moves the card to its new axon in the index.
    pub fn insert(&mut self, card: Particle, record: CardRecord) -> Option<CardRecord> {
        let old = self.records.insert(card, record);
        let key = match old {
            Some(o) if o.particle == record.particle => return old,
            Some(o) => self.unindex(&o.particle, &card),
            None => stake_key(&card),
        };
        self.by_axon.entry(record.particle).or_default().insert(card, key);
        old
    }

    /// Remove the record of `card`; returns it.
    pub fn remove(&mut self, card: &Particle) -> Option<CardRecord> {
        let old = self.records.remove(card)?;
        self.unindex(&old.particle, card);
        Some(old)
    }

    /// Drop `card` from `axon`'s index entry; returns its cached stake key.
    fn unindex(&mut self, axon: &Particle, card: &Particle) -> [u8; 32] {
        let cards = self.by_axon.get_mut(axon).expect("indexed axon");
        let key = cards.remove(card).expect("indexed card");
        if cards.is_empty() {
            self.by_axon.remove(axon);
        }
        key
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.by_axon.clear();
    }

    /// The cards on `axon`, ascending by card id.
    pub fn on_axon<'a>(&'a self, axon: &Particle) -> impl Iterator<Item = (&'a Particle, &'a CardRecord)> + use<'a> {
        self.by_axon
            .get(axon)
            .into_iter()
            .flat_map(move |cards| cards.keys().map(move |c| (c, &self.records[c])))
    }

    /// Axons holding at least one card.
    pub fn axon_count(&self) -> usize {
        self.by_axon.len()
    }

    /// The v2 commitment: the trie root.
    pub fn commitment(&self) -> Digest {
        self.records.commitment()
    }

    /// The v1 commitment, cached until the next mutation.
    pub fn legacy_commitment(&self, f: impl FnOnce(&BTreeMap<Particle, CardRecord>) -> Commitment) -> Commitment {
        self.records.legacy_commitment(f)
    }

    /// Inclusion / non-inclusion proof against [`Self::commitment`].
    pub fn prove(&self, card: &Particle) -> TrieProof {
        self.records.prove(card)
    }

    /// Records touched since the last commitment read.
    pub fn pending(&self) -> usize {
        self.records.pending()
    }

    /// The leaf digest a record carries under its card id.
    pub fn leaf(card: &Particle, record: &CardRecord) -> Digest {
        Keyed::<Particle, CardRecord>::leaf(card, record)
    }
}

/// Stake on each side of one axon (chain-economy §4.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SideSums {
    /// S⁺: stake of cards with v ∈ {0, +1}
    pub plus: u128,
    /// S⁻: stake of cards with v = −1
    pub minus: u128,
}

impl SideSums {
    fn add(&mut self, side: Side, stake: u64) {
        match side {
            Side::Plus => self.plus += stake as u128,
            Side::Minus => self.minus += stake as u128,
        }
    }
}

/// One position at a snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub card: Particle,
    pub record: CardRecord,
    /// `balances[stake_key(card)]`
    pub stake: u64,
}

/// The positions of one axon at a snapshot, ascending by card id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AxonPositions {
    pub axon: Particle,
    pub sums: SideSums,
    pub positions: Vec<Position>,
}

/// Every position of a state, grouped by axon, axons ascending by id. Owned
/// and immutable: the chain keeps the snapshots of two boundaries until the
/// settlement that reads them (chain-economy §2.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PositionSnapshot {
    axons: Vec<AxonPositions>,
    len: usize,
    total: u128,
    active: u128,
}

impl PositionSnapshot {
    /// Axons holding at least one card, ascending by id.
    pub fn axons(&self) -> &[AxonPositions] {
        &self.axons
    }

    pub fn get(&self, axon: &Particle) -> Option<&AxonPositions> {
        self.axons.binary_search_by(|a| a.axon.cmp(axon)).ok().map(|i| &self.axons[i])
    }

    pub fn find(&self, axon: &Particle, card: &Particle) -> Option<&Position> {
        let a = self.get(axon)?;
        a.positions.binary_search_by(|p| p.card.cmp(card)).ok().map(|i| &a.positions[i])
    }

    /// Number of positions.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Σ stake over every position.
    pub fn total_stake(&self) -> u128 {
        self.total
    }

    /// Σ stake over positions with v ≠ 0 (chain-economy §6.1 `active(E)`).
    pub fn active_stake(&self) -> u128 {
        self.active
    }
}

impl BbgState {
    /// The stake of `card`: `balances[stake_key(card)]`, 0 when absent.
    pub fn stake(&self, card: &Particle) -> u64 {
        self.balances.get(&stake_key(card)).copied().unwrap_or(0)
    }

    /// The focus pool of `token`: `balances[focus_key(token)]`, 0 when absent.
    pub fn focus_pool(&self, token: &Particle) -> u64 {
        self.balances.get(&focus_key(token)).copied().unwrap_or(0)
    }

    /// S⁺ and S⁻ of `axon`, in O(cards on the axon) with no hashing.
    pub fn side_sums(&self, axon: &Particle) -> SideSums {
        let mut sums = SideSums::default();
        for (record, key) in self.indexed(axon) {
            sums.add(record.valence.side(), self.balance_at(key));
        }
        sums
    }

    /// Every position, grouped by axon, in O(cards) with no hashing.
    pub fn positions(&self) -> PositionSnapshot {
        let mut snap = PositionSnapshot::default();
        for (axon, cards) in &self.cards.by_axon {
            let mut sums = SideSums::default();
            let positions: Vec<Position> = cards
                .iter()
                .map(|(card, key)| {
                    let record = self.cards[card];
                    let stake = self.balance_at(key);
                    sums.add(record.valence.side(), stake);
                    if record.valence.is_active() {
                        snap.active += stake as u128;
                    }
                    Position { card: *card, record, stake }
                })
                .collect();
            snap.len += positions.len();
            snap.total += sums.plus + sums.minus;
            snap.axons.push(AxonPositions { axon: *axon, sums, positions });
        }
        snap
    }

    fn indexed<'a>(&'a self, axon: &Particle) -> impl Iterator<Item = (&'a CardRecord, &'a [u8; 32])> + use<'a> {
        self.cards
            .by_axon
            .get(axon)
            .into_iter()
            .flat_map(move |cards| cards.iter().map(move |(c, key)| (&self.cards[c], key)))
    }

    fn balance_at(&self, key: &[u8; 32]) -> u64 {
        self.balances.get(key).copied().unwrap_or(0)
    }
}
