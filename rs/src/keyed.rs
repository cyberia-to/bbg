// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Committed dimension maps.
//!
//! Every committed dimension of [`crate::BbgState`] is a [`Keyed`] map (or a
//! [`KeyedSet`] / [`AxonIndex`]): a `BTreeMap` for reads — `Deref` exposes the
//! whole read API — plus a [`Trie`] that every mutation updates in place, so
//! the v2 commitment is always current at O(log n) per touched record and the
//! v1 (legacy Brakedown) commitment is cached until the next mutation. There
//! is no way to mutate a record without the trie seeing it: `get_mut` and the
//! entry API hand out a guard that re-hashes the leaf on drop.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::OnceLock;

use lens::Commitment;
use nebu::Goldilocks;

use crate::dim::{bytes32_limbs, u64_limbs};
use crate::trie::{leaf_digest, Digest, Trie, TrieProof};
use crate::types::Particle;

/// A key of a committed dimension: its 32-byte trie path.
pub trait TrieKey: Ord + Copy {
    fn key_bytes(&self) -> [u8; 32];
}

impl TrieKey for [u8; 32] {
    fn key_bytes(&self) -> [u8; 32] {
        *self
    }
}

/// Heights and steps: little-endian in the first 8 bytes — the key layout the
/// v1 `time` / `signals` polynomials already use.
impl TrieKey for u64 {
    fn key_bytes(&self) -> [u8; 32] {
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&self.to_le_bytes());
        key
    }
}

/// A committed record: its v2 leaf value fields, in frozen order, in the
/// injective encoding (`u64` → two 32-bit limbs, 32 bytes → eight).
pub trait LeafValue {
    fn leaf_vals(&self) -> Vec<Goldilocks>;
}

impl LeafValue for u64 {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        u64_limbs(*self).to_vec()
    }
}

impl LeafValue for Goldilocks {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        vec![*self]
    }
}

impl LeafValue for [u8; 32] {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        bytes32_limbs(self).to_vec()
    }
}

impl LeafValue for () {
    fn leaf_vals(&self) -> Vec<Goldilocks> {
        Vec::new()
    }
}

/// A committed `K → V` dimension.
pub struct Keyed<K, V> {
    map: BTreeMap<K, V>,
    trie: Trie,
    legacy: OnceLock<Commitment>,
}

impl<K: TrieKey, V: LeafValue> Default for Keyed<K, V> {
    fn default() -> Self {
        Self { map: BTreeMap::new(), trie: Trie::new(), legacy: OnceLock::new() }
    }
}

impl<K, V> Deref for Keyed<K, V> {
    type Target = BTreeMap<K, V>;
    fn deref(&self) -> &BTreeMap<K, V> {
        &self.map
    }
}

impl<K: TrieKey, V: LeafValue> Keyed<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    fn touched(&mut self, key: &K) {
        self.legacy = OnceLock::new();
        match self.map.get(key) {
            Some(v) => {
                self.trie.set(key.key_bytes(), leaf_digest(&key.key_bytes(), &v.leaf_vals()));
            }
            None => {
                self.trie.remove(&key.key_bytes());
            }
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let old = self.map.insert(key, value);
        self.touched(&key);
        old
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let old = self.map.remove(key)?;
        self.touched(key);
        Some(old)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.trie = Trie::new();
        self.legacy = OnceLock::new();
    }

    /// Mutable access to `key`'s record; the leaf is re-hashed when the guard
    /// drops.
    pub fn get_mut(&mut self, key: &K) -> Option<ValueMut<'_, K, V>> {
        if !self.map.contains_key(key) {
            return None;
        }
        Some(ValueMut { keyed: self, key: *key })
    }

    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        Entry { keyed: self, key }
    }

    /// The v2 commitment: the trie root.
    pub fn commitment(&self) -> Digest {
        self.trie.root()
    }

    /// The v1 commitment, computed by `f` over the map and cached until the
    /// next mutation.
    pub fn legacy_commitment(&self, f: impl FnOnce(&BTreeMap<K, V>) -> Commitment) -> Commitment {
        *self.legacy.get_or_init(|| f(&self.map))
    }

    /// Inclusion / non-inclusion proof against [`Self::commitment`].
    pub fn prove(&self, key: &K) -> TrieProof {
        self.trie.prove(&key.key_bytes())
    }

    /// The leaf digest a record carries under its key.
    pub fn leaf(key: &K, value: &V) -> Digest {
        leaf_digest(&key.key_bytes(), &value.leaf_vals())
    }
}

/// A mutable record; re-hashes its leaf on drop.
pub struct ValueMut<'a, K: TrieKey, V: LeafValue> {
    keyed: &'a mut Keyed<K, V>,
    key: K,
}

impl<K: TrieKey, V: LeafValue> Deref for ValueMut<'_, K, V> {
    type Target = V;
    fn deref(&self) -> &V {
        self.keyed.map.get(&self.key).expect("guarded key is present")
    }
}

impl<K: TrieKey, V: LeafValue> DerefMut for ValueMut<'_, K, V> {
    fn deref_mut(&mut self) -> &mut V {
        self.keyed.map.get_mut(&self.key).expect("guarded key is present")
    }
}

impl<K: TrieKey, V: LeafValue> Drop for ValueMut<'_, K, V> {
    fn drop(&mut self) {
        let key = self.key;
        self.keyed.touched(&key);
    }
}

/// The entry API: insert-if-absent, then a guarded `&mut V`.
pub struct Entry<'a, K: TrieKey, V: LeafValue> {
    keyed: &'a mut Keyed<K, V>,
    key: K,
}

impl<'a, K: TrieKey, V: LeafValue> Entry<'a, K, V> {
    pub fn or_insert(self, default: V) -> ValueMut<'a, K, V> {
        self.or_insert_with(|| default)
    }

    pub fn or_insert_with(self, default: impl FnOnce() -> V) -> ValueMut<'a, K, V> {
        if !self.keyed.map.contains_key(&self.key) {
            self.keyed.map.insert(self.key, default());
        }
        ValueMut { keyed: self.keyed, key: self.key }
    }

    pub fn or_default(self) -> ValueMut<'a, K, V>
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}

/// A committed key set (the nullifier dimension N).
#[derive(Default)]
pub struct KeyedSet<K: TrieKey> {
    inner: Keyed<K, ()>,
}

impl<K: TrieKey> KeyedSet<K> {
    pub fn new() -> Self {
        Self { inner: Keyed::new() }
    }

    /// Returns `true` if the key was new.
    pub fn insert(&mut self, key: K) -> bool {
        self.inner.insert(key, ()).is_none()
    }

    pub fn remove(&mut self, key: &K) -> bool {
        self.inner.remove(key).is_some()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &K> {
        self.inner.keys()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn commitment(&self) -> Digest {
        self.inner.commitment()
    }

    pub fn legacy_commitment(&self, f: impl FnOnce(&BTreeMap<K, ()>) -> Commitment) -> Commitment {
        self.inner.legacy_commitment(f)
    }

    pub fn prove(&self, key: &K) -> TrieProof {
        self.inner.prove(key)
    }
}

/// A committed adjacency index (`axons_out` / `axons_in`): particle → set of
/// axon ids.
///
/// Reads see insertion-ordered lists (`Deref` to the map). The commitment is a
/// trie of leaves `(particle, [degree, root(set)])` where `set` is itself a
/// trie over the axon ids with empty leaf values — appending to a hub costs
/// O(log degree), never O(degree).
#[derive(Default)]
pub struct AxonIndex {
    map: BTreeMap<Particle, Vec<Particle>>,
    sets: BTreeMap<Particle, Trie>,
    trie: Trie,
    /// degree → how many particles have it; the last key is the max degree.
    degrees: BTreeMap<usize, usize>,
    legacy: OnceLock<Commitment>,
}

impl Deref for AxonIndex {
    type Target = BTreeMap<Particle, Vec<Particle>>;
    fn deref(&self) -> &BTreeMap<Particle, Vec<Particle>> {
        &self.map
    }
}

impl AxonIndex {
    pub fn new() -> Self {
        Self::default()
    }

    fn member_digest(axon: &Particle) -> Digest {
        leaf_digest(axon, &[])
    }

    /// The outer leaf of `particle`: `[degree (2 limbs), root(set) (4 limbs)]`.
    pub fn leaf(particle: &Particle, degree: u64, set_root: &Digest) -> Digest {
        let mut vals = u64_limbs(degree).to_vec();
        vals.extend_from_slice(set_root);
        leaf_digest(particle, &vals)
    }

    /// Move one particle from degree `old` to degree `new` in the histogram.
    fn degree_moved(&mut self, old: usize, new: usize) {
        if old == new {
            return;
        }
        if old > 0 {
            let n = self.degrees.get_mut(&old).expect("histogram tracks every list");
            *n -= 1;
            if *n == 0 {
                self.degrees.remove(&old);
            }
        }
        if new > 0 {
            *self.degrees.entry(new).or_insert(0) += 1;
        }
    }

    fn degree(&self, particle: &Particle) -> usize {
        self.map.get(particle).map_or(0, Vec::len)
    }

    fn touched(&mut self, particle: &Particle) {
        self.legacy = OnceLock::new();
        match (self.map.get(particle), self.sets.get(particle)) {
            (Some(list), Some(set)) => {
                let digest = Self::leaf(particle, list.len() as u64, &set.root());
                self.trie.set(*particle, digest);
            }
            _ => {
                self.trie.remove(particle);
            }
        }
    }

    /// Add `axon` to `particle`'s list. Returns `true` if it was new.
    pub fn add(&mut self, particle: Particle, axon: Particle) -> bool {
        let set = self.sets.entry(particle).or_default();
        if !set.set(axon, Self::member_digest(&axon)) {
            return false;
        }
        let list = self.map.entry(particle).or_default();
        list.push(axon);
        let len = list.len();
        self.degree_moved(len - 1, len);
        self.touched(&particle);
        true
    }

    /// Replace `particle`'s whole list; a repeated axon keeps its first
    /// position (the list mirrors the set). Returns the previous list. The
    /// map-shaped mutator undo/restore uses.
    pub fn insert(&mut self, particle: Particle, list: Vec<Particle>) -> Option<Vec<Particle>> {
        let old = self.remove(&particle);
        let mut set = Trie::new();
        let list: Vec<Particle> = list.into_iter().filter(|a| set.set(*a, Self::member_digest(a))).collect();
        if list.is_empty() {
            return old;
        }
        self.degree_moved(0, list.len());
        self.map.insert(particle, list);
        self.sets.insert(particle, set);
        self.touched(&particle);
        old
    }

    /// Remove `particle`'s whole list. Returns it.
    pub fn remove(&mut self, particle: &Particle) -> Option<Vec<Particle>> {
        let old = self.map.remove(particle)?;
        self.sets.remove(particle);
        self.degree_moved(old.len(), 0);
        self.touched(particle);
        Some(old)
    }

    /// Remove `axon` from `particle`'s list, dropping the list when it empties.
    /// Returns `true` if it was present.
    pub fn remove_axon(&mut self, particle: &Particle, axon: &Particle) -> bool {
        let Some(set) = self.sets.get_mut(particle) else { return false };
        if !set.remove(axon) {
            return false;
        }
        let list = self.map.get_mut(particle).expect("list mirrors set");
        let before = list.len();
        list.retain(|a| a != axon);
        let after = list.len();
        if after == 0 {
            self.map.remove(particle);
            self.sets.remove(particle);
        }
        self.degree_moved(before, after);
        self.touched(particle);
        true
    }

    /// Keep the first `len` axons of `particle`'s list, dropping the rest from
    /// the list and the set — the undo of the appends a reverted block made
    /// (lists are append-only). `len = 0` removes the list. Returns how many
    /// axons were dropped.
    pub fn truncate(&mut self, particle: &Particle, len: usize) -> usize {
        let before = self.degree(particle);
        if before <= len {
            return 0;
        }
        if len == 0 {
            return self.remove(particle).map_or(0, |l| l.len());
        }
        let list = self.map.get_mut(particle).expect("degree > 0");
        let dropped = list.split_off(len);
        let set = self.sets.get_mut(particle).expect("list mirrors set");
        for axon in &dropped {
            set.remove(axon);
        }
        self.degree_moved(before, len);
        self.touched(particle);
        dropped.len()
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.sets.clear();
        self.trie = Trie::new();
        self.degrees.clear();
        self.legacy = OnceLock::new();
    }

    /// The longest list — O(log) from the degree histogram, maintained across
    /// every mutation.
    pub fn max_degree(&self) -> usize {
        self.degrees.last_key_value().map_or(0, |(d, _)| *d)
    }

    /// The v2 commitment: the outer trie root.
    pub fn commitment(&self) -> Digest {
        self.trie.root()
    }

    /// The set root of `particle`'s axon ids.
    pub fn set_root(&self, particle: &Particle) -> Option<Digest> {
        self.sets.get(particle).map(Trie::root)
    }

    pub fn legacy_commitment(
        &self,
        f: impl FnOnce(&BTreeMap<Particle, Vec<Particle>>) -> Commitment,
    ) -> Commitment {
        *self.legacy.get_or_init(|| f(&self.map))
    }

    /// Proof of `particle`'s outer leaf against [`Self::commitment`].
    pub fn prove(&self, particle: &Particle) -> TrieProof {
        self.trie.prove(particle)
    }

    /// Proof of `axon` against `particle`'s set root.
    pub fn prove_member(&self, particle: &Particle, axon: &Particle) -> Option<TrieProof> {
        self.sets.get(particle).map(|s| s.prove(axon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trie::EMPTY;

    fn p(seed: u8) -> Particle {
        [seed; 32]
    }

    #[test]
    fn every_mutation_path_updates_the_trie() {
        let mut m: Keyed<[u8; 32], u64> = Keyed::new();
        m.insert(p(1), 10);
        let expected = |entries: &[([u8; 32], u64)]| {
            Trie::from_entries(entries.iter().map(|(k, v)| (*k, Keyed::<[u8; 32], u64>::leaf(k, v)))).root()
        };
        assert_eq!(m.commitment(), expected(&[(p(1), 10)]));
        *m.get_mut(&p(1)).unwrap() += 5;
        assert_eq!(m.commitment(), expected(&[(p(1), 15)]));
        *m.entry(p(2)).or_insert(0) += 7;
        assert_eq!(m.commitment(), expected(&[(p(1), 15), (p(2), 7)]));
        *m.entry(p(1)).or_insert(999) = 1;
        assert_eq!(m.commitment(), expected(&[(p(1), 1), (p(2), 7)]));
        m.remove(&p(2));
        assert_eq!(m.commitment(), expected(&[(p(1), 1)]));
        m.remove(&p(1));
        assert_eq!(m.commitment(), EMPTY);
        assert!(m.get_mut(&p(3)).is_none());
        m.insert(p(4), 4);
        m.clear();
        assert_eq!(m.commitment(), EMPTY);
        assert!(m.is_empty());
    }

    #[test]
    fn legacy_cache_resets_on_mutation() {
        let mut m: Keyed<[u8; 32], u64> = Keyed::new();
        let count = std::cell::Cell::new(0);
        let f = |map: &BTreeMap<[u8; 32], u64>| {
            count.set(count.get() + 1);
            Commitment(hemera::hash(&(map.len() as u64).to_le_bytes()))
        };
        m.insert(p(1), 1);
        let a = m.legacy_commitment(f);
        let b = m.legacy_commitment(f);
        assert_eq!(a, b);
        assert_eq!(count.get(), 1, "cached between mutations");
        m.insert(p(2), 2);
        let c = m.legacy_commitment(f);
        assert_ne!(a, c);
        assert_eq!(count.get(), 2, "recomputed after a mutation");
    }

    #[test]
    fn axon_index_is_a_set_with_a_nested_root() {
        let mut ix = AxonIndex::new();
        assert!(ix.add(p(1), p(10)));
        assert!(ix.add(p(1), p(11)));
        assert!(!ix.add(p(1), p(10)), "duplicates are ignored");
        assert_eq!(ix.get(&p(1)).unwrap().len(), 2);
        assert_eq!(ix.max_degree(), 2);
        let set = Trie::from_entries([p(10), p(11)].iter().map(|a| (*a, AxonIndex::member_digest(a)))).root();
        let outer = Trie::from_entries([(p(1), AxonIndex::leaf(&p(1), 2, &set))]).root();
        assert_eq!(ix.commitment(), outer);
        // order independence of the set
        let mut other = AxonIndex::new();
        other.add(p(1), p(11));
        other.add(p(1), p(10));
        assert_eq!(ix.commitment(), other.commitment());
        // removal collapses the list and the max degree
        assert!(ix.remove_axon(&p(1), &p(11)));
        assert!(!ix.remove_axon(&p(1), &p(11)));
        assert_eq!(ix.max_degree(), 1);
        assert!(ix.remove_axon(&p(1), &p(10)));
        assert!(ix.get(&p(1)).is_none());
        assert_eq!(ix.max_degree(), 0);
        assert_eq!(ix.commitment(), EMPTY);
        // whole-list insert/remove (the undo/restore shape) commits the same set
        ix.insert(p(1), vec![p(11), p(10)]);
        assert_eq!(ix.commitment(), other.commitment());
        assert_eq!(ix.max_degree(), 2);
        assert_eq!(ix.remove(&p(1)).map(|l| l.len()), Some(2));
        assert_eq!(ix.commitment(), EMPTY);
    }

    /// Random add / remove_axon / truncate / remove / insert against a model
    /// list map: after every operation the index commits exactly what an index
    /// built fresh from the model commits, and the max degree is the model's.
    #[test]
    fn axon_index_matches_a_fresh_build_under_random_operations() {
        let mut x: u64 = 0xA11CE;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut ix = AxonIndex::new();
        let mut model: BTreeMap<Particle, Vec<Particle>> = BTreeMap::new();
        for step in 0..2000u32 {
            let particle = p((next() % 6) as u8);
            let mut axon = [0u8; 32];
            axon[..8].copy_from_slice(&(next() % 40).to_le_bytes());
            match next() % 10 {
                0..=5 => {
                    let list = model.entry(particle).or_default();
                    let new = !list.contains(&axon);
                    if new {
                        list.push(axon);
                    }
                    assert_eq!(ix.add(particle, axon), new);
                }
                6 => {
                    let had = model.get(&particle).is_some_and(|l| l.contains(&axon));
                    if let Some(l) = model.get_mut(&particle) {
                        l.retain(|a| a != &axon);
                    }
                    assert_eq!(ix.remove_axon(&particle, &axon), had);
                }
                7 => {
                    let len = (next() % 8) as usize;
                    let before = model.get(&particle).map_or(0, Vec::len);
                    if let Some(l) = model.get_mut(&particle) {
                        l.truncate(len);
                    }
                    assert_eq!(ix.truncate(&particle, len), before.saturating_sub(len));
                }
                8 => {
                    assert_eq!(ix.remove(&particle), model.remove(&particle).filter(|l| !l.is_empty()));
                }
                _ => {
                    // whole-list restore, with a repeated axon the list drops
                    let list: Vec<Particle> = (0..(next() % 5)).map(|i| p(100 + (i % 3) as u8)).collect();
                    let mut dedup: Vec<Particle> = Vec::new();
                    for a in &list {
                        if !dedup.contains(a) {
                            dedup.push(*a);
                        }
                    }
                    ix.insert(particle, list);
                    model.insert(particle, dedup);
                }
            }
            model.retain(|_, l| !l.is_empty());
            let mut fresh = AxonIndex::new();
            for (k, list) in &model {
                for a in list {
                    fresh.add(*k, *a);
                }
            }
            assert_eq!(ix.commitment(), fresh.commitment(), "step {step}");
            assert_eq!(&*ix, &model, "step {step}");
            assert_eq!(ix.max_degree(), model.values().map(Vec::len).max().unwrap_or(0), "step {step}");
        }
    }

    #[test]
    fn axon_membership_proofs_verify() {
        let mut ix = AxonIndex::new();
        for i in 0..20u8 {
            ix.add(p(1), p(100 + i));
        }
        let set_root = ix.set_root(&p(1)).unwrap();
        let proof = ix.prove_member(&p(1), &p(105)).unwrap();
        assert!(proof.verify(&set_root, &p(105), Some(&AxonIndex::member_digest(&p(105)))));
        let absent = ix.prove_member(&p(1), &p(200)).unwrap();
        assert!(absent.verify(&set_root, &p(200), None));
        let outer = ix.prove(&p(1));
        assert!(outer.verify(&ix.commitment(), &p(1), Some(&AxonIndex::leaf(&p(1), 20, &set_root))));
    }
}
