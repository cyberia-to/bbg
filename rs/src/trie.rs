// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Keyed trie — the v2 dimension commitment (see `specs/state.md` § dimension
//! commitment).
//!
//! A canonical binary Merkle-Patricia trie over 32-byte keys. The shape is a
//! pure function of the key set (path-compressed: a branch exists only where
//! two keys first differ), so the root is a pure function of the
//! `(key, value)` set — insertion order, deletions and reinsertions leave no
//! trace. Every node digest is a chain of [`compress4`] — the same hemera
//! permutation zheng replays in-circuit for the root fold — so a path can be
//! verified by the same primitive the root is built from.
//!
//! Costs: `set` / `remove` / `prove` are O(depth) compressions, depth ≈ log₂ n
//! for hashed keys (bounded by 256). The whole point: a block that touches `c`
//! records costs O(c · log n), never O(n).
//!
//! Derivation (frozen; `RootVersion::V2`, dimension version 3 — the version
//! enters every leaf, so records committed under another dimension version
//! never share a digest):
//!
//! ```text
//! EMPTY            = [0, 0, 0, 0]
//! leaf(k, v)       = fold(compress4, [LEAF_TAG, |v|, DIMENSION_VERSION, 0], [k₀..₃, k₄..₇, chunk₁(v), …])
//!                    k as eight 32-bit limbs, chunks of 4 limbs, zero-padded
//! branch(i, l, r)  = compress4(compress4([BRANCH_TAG, i, 0, 0], l), r)
//! node(S)          = EMPTY                          if S = ∅
//!                  = leaf(k, v)                     if S = {(k, v)}
//!                  = branch(i, node(S₀), node(S₁))  otherwise, i = first bit
//!                    index on which the keys of S differ (MSB-first), S_b = keys
//!                    whose bit i is b
//! commitment(S)    = node(S)
//! ```

use nebu::Goldilocks;
use crate::root::compress4;

use crate::dim::{bytes32_limbs, DIMENSION_VERSION};

/// A 4-limb hemera digest — one root-fold leaf.
pub type Digest = [Goldilocks; 4];

/// Domain tag of a leaf chain IV: `"bbg-leaf"` as a little-endian u64.
pub const LEAF_TAG: u64 = u64::from_le_bytes(*b"bbg-leaf");
/// Domain tag of a branch chain IV: `"bbg-node"` as a little-endian u64.
pub const BRANCH_TAG: u64 = u64::from_le_bytes(*b"bbg-node");

/// The digest of the empty trie.
pub const EMPTY: Digest = [Goldilocks::ZERO; 4];

/// Bit `i` of a key, MSB-first: bit 0 is the most significant bit of byte 0.
#[inline]
pub fn key_bit(key: &[u8; 32], i: u16) -> bool {
    (key[(i / 8) as usize] >> (7 - (i % 8))) & 1 == 1
}

/// The first bit index on which two distinct keys differ.
fn first_diff_bit(a: &[u8; 32], b: &[u8; 32]) -> u16 {
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        let d = x ^ y;
        if d != 0 {
            return (i as u16) * 8 + d.leading_zeros() as u16;
        }
    }
    unreachable!("first_diff_bit on equal keys")
}

/// `leaf(k, v)` — the digest of one `(key, values)` entry.
///
/// The key enters as eight 32-bit limbs and every value field is expected in
/// an injective encoding ([`crate::dim::u64_limbs`], [`bytes32_limbs`]), so
/// two distinct records never share a preimage.
pub fn leaf_digest(key: &[u8; 32], vals: &[Goldilocks]) -> Digest {
    let mut acc = [
        Goldilocks::new(LEAF_TAG),
        Goldilocks::new(vals.len() as u64),
        Goldilocks::new(DIMENSION_VERSION),
        Goldilocks::ZERO,
    ];
    let k = bytes32_limbs(key);
    for half in k.chunks(4) {
        let mut block = [Goldilocks::ZERO; 4];
        block.copy_from_slice(half);
        acc = compress4(&acc, &block);
    }
    for chunk in vals.chunks(4) {
        let mut block = [Goldilocks::ZERO; 4];
        block[..chunk.len()].copy_from_slice(chunk);
        acc = compress4(&acc, &block);
    }
    acc
}

/// `branch(i, l, r)` — the digest of a branch deciding on key bit `i`.
pub fn branch_digest(bit: u16, left: &Digest, right: &Digest) -> Digest {
    let iv = [Goldilocks::new(BRANCH_TAG), Goldilocks::new(bit as u64), Goldilocks::ZERO, Goldilocks::ZERO];
    compress4(&compress4(&iv, left), right)
}

#[derive(Clone, Debug)]
enum Node {
    Leaf { key: [u8; 32], digest: Digest },
    /// `dirty`: a descendant changed since `digest` was computed.
    Branch { bit: u16, left: u32, right: u32, digest: Digest, dirty: bool },
}

impl Node {
    #[inline]
    fn digest(&self) -> &Digest {
        match self {
            Node::Leaf { digest, .. } | Node::Branch { digest, .. } => digest,
        }
    }
}

/// A canonical keyed trie with cached node digests.
///
/// Mutations change the shape and mark the touched path; branch digests are
/// recomputed once, bottom-up, on the next [`Trie::root`] — so a batch of `c`
/// changes costs `c` path walks plus one hash per dirty branch, and a bulk
/// load hashes every node exactly once.
#[derive(Clone, Debug, Default)]
pub struct Trie {
    nodes: Vec<Node>,
    free: Vec<u32>,
    root: Option<u32>,
    len: usize,
}

/// A path from the root to the node a key resolves to.
///
/// `steps` are `(bit, sibling digest)` root-to-leaf; `terminal` is the leaf the
/// path ends at (`None` only for the empty trie). A terminal whose key is not
/// the queried key is a proof of non-inclusion: the canonical trie has exactly
/// one node on every key's path end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrieProof {
    pub steps: Vec<(u16, Digest)>,
    pub terminal: Option<([u8; 32], Digest)>,
}

impl TrieProof {
    /// Check the proof against `root` for `key`: `Some(digest)` claims the key
    /// is present with that leaf digest, `None` claims it is absent.
    pub fn verify(&self, root: &Digest, key: &[u8; 32], expected: Option<&Digest>) -> bool {
        let mut cur = match (&self.terminal, expected) {
            (None, None) => {
                return self.steps.is_empty() && *root == EMPTY;
            }
            (None, Some(_)) => return false,
            (Some((k, d)), Some(e)) => {
                if k != key || d != e {
                    return false;
                }
                *d
            }
            (Some((k, d)), None) => {
                if k == key {
                    return false;
                }
                *d
            }
        };
        let mut last: Option<u16> = None;
        for (bit, sibling) in self.steps.iter().rev() {
            if let Some(l) = last
                && *bit >= l
            {
                return false;
            }
            last = Some(*bit);
            cur = if key_bit(key, *bit) {
                branch_digest(*bit, sibling, &cur)
            } else {
                branch_digest(*bit, &cur, sibling)
            };
        }
        cur == *root
    }
}

impl Trie {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of keys.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The commitment: the root digest, [`EMPTY`] for no keys. Hashes the
    /// branches changed since the last call.
    pub fn root(&mut self) -> Digest {
        match self.root {
            None => EMPTY,
            Some(r) => self.hash(r),
        }
    }

    /// Whether some branch digest is stale (a [`Self::root`] call would hash).
    pub fn is_dirty(&self) -> bool {
        self.root.is_some_and(|r| matches!(self.nodes[r as usize], Node::Branch { dirty: true, .. }))
    }

    fn hash(&mut self, idx: u32) -> Digest {
        let (bit, left, right) = match &self.nodes[idx as usize] {
            Node::Leaf { digest, .. } | Node::Branch { digest, dirty: false, .. } => return *digest,
            Node::Branch { bit, left, right, dirty: true, .. } => (*bit, *left, *right),
        };
        let l = self.hash(left);
        let r = self.hash(right);
        let d = branch_digest(bit, &l, &r);
        if let Node::Branch { digest, dirty, .. } = &mut self.nodes[idx as usize] {
            *digest = d;
            *dirty = false;
        }
        d
    }

    fn alloc(&mut self, node: Node) -> u32 {
        if let Some(i) = self.free.pop() {
            self.nodes[i as usize] = node;
            i
        } else {
            self.nodes.push(node);
            (self.nodes.len() - 1) as u32
        }
    }

    fn release(&mut self, i: u32) {
        self.free.push(i);
    }

    /// Walk `key`'s path: the branch indices passed and the leaf reached.
    fn descend(&self, key: &[u8; 32]) -> (Vec<u32>, u32) {
        let mut path = Vec::new();
        let mut cur = self.root.expect("descend on empty trie");
        loop {
            match &self.nodes[cur as usize] {
                Node::Branch { bit, left, right, .. } => {
                    path.push(cur);
                    cur = if key_bit(key, *bit) { *right } else { *left };
                }
                Node::Leaf { .. } => return (path, cur),
            }
        }
    }

    fn mark(&mut self, path: &[u32]) {
        for &idx in path {
            if let Node::Branch { dirty, .. } = &mut self.nodes[idx as usize] {
                *dirty = true;
            }
        }
    }

    fn replace_child(&mut self, parent: u32, old: u32, new: u32) {
        if let Node::Branch { left, right, .. } = &mut self.nodes[parent as usize] {
            if *left == old {
                *left = new;
            } else {
                debug_assert_eq!(*right, old);
                *right = new;
            }
        }
    }

    /// Set `key`'s leaf digest, inserting the key if absent. Returns `true` if
    /// the key was new.
    pub fn set(&mut self, key: [u8; 32], digest: Digest) -> bool {
        let Some(_) = self.root else {
            let leaf = self.alloc(Node::Leaf { key, digest });
            self.root = Some(leaf);
            self.len = 1;
            return true;
        };
        let (path, leaf) = self.descend(&key);
        let leaf_key = match &self.nodes[leaf as usize] {
            Node::Leaf { key, .. } => *key,
            Node::Branch { .. } => unreachable!(),
        };
        if leaf_key == key {
            if let Node::Leaf { digest: d, .. } = &mut self.nodes[leaf as usize] {
                if *d == digest {
                    return false;
                }
                *d = digest;
            }
            self.mark(&path);
            return false;
        }
        let j = first_diff_bit(&key, &leaf_key);
        // Keep the ancestors deciding on bits before `j`; split below them.
        let mut keep = path.len();
        while keep > 0 {
            let Node::Branch { bit, .. } = self.nodes[path[keep - 1] as usize] else { unreachable!() };
            if bit < j {
                break;
            }
            keep -= 1;
        }
        let split = if keep == path.len() { leaf } else { path[keep] };
        let new_leaf = self.alloc(Node::Leaf { key, digest });
        let (left, right) = if key_bit(&key, j) { (split, new_leaf) } else { (new_leaf, split) };
        let branch = self.alloc(Node::Branch { bit: j, left, right, digest: EMPTY, dirty: true });
        if keep == 0 {
            self.root = Some(branch);
        } else {
            self.replace_child(path[keep - 1], split, branch);
        }
        self.mark(&path[..keep]);
        self.len += 1;
        true
    }

    /// Remove `key`. Returns `true` if it was present.
    pub fn remove(&mut self, key: &[u8; 32]) -> bool {
        let Some(_) = self.root else { return false };
        let (path, leaf) = self.descend(key);
        match &self.nodes[leaf as usize] {
            Node::Leaf { key: k, .. } if k == key => {}
            _ => return false,
        }
        self.release(leaf);
        self.len -= 1;
        let Some(&parent) = path.last() else {
            self.root = None;
            return true;
        };
        let Node::Branch { left, right, .. } = self.nodes[parent as usize] else { unreachable!() };
        let sibling = if left == leaf { right } else { left };
        self.release(parent);
        let rest = &path[..path.len() - 1];
        match rest.last() {
            None => self.root = Some(sibling),
            Some(&grand) => self.replace_child(grand, parent, sibling),
        }
        self.mark(rest);
        true
    }

    /// The leaf digest stored for `key`.
    pub fn get(&self, key: &[u8; 32]) -> Option<Digest> {
        let _ = self.root?;
        let (_, leaf) = self.descend(key);
        match &self.nodes[leaf as usize] {
            Node::Leaf { key: k, digest } if k == key => Some(*digest),
            _ => None,
        }
    }

    /// An inclusion / non-inclusion proof for `key` against [`Self::root`].
    pub fn prove(&mut self, key: &[u8; 32]) -> TrieProof {
        let Some(_) = self.root else {
            return TrieProof { steps: Vec::new(), terminal: None };
        };
        self.root();
        let (path, leaf) = self.descend(key);
        let steps = path
            .iter()
            .map(|&idx| {
                let Node::Branch { bit, left, right, .. } = &self.nodes[idx as usize] else { unreachable!() };
                let sibling = if key_bit(key, *bit) { *left } else { *right };
                (*bit, *self.nodes[sibling as usize].digest())
            })
            .collect();
        let Node::Leaf { key: k, digest } = &self.nodes[leaf as usize] else { unreachable!() };
        TrieProof { steps, terminal: Some((*k, *digest)) }
    }

    /// Build from scratch — the reference the incremental path must match.
    pub fn from_entries<I: IntoIterator<Item = ([u8; 32], Digest)>>(entries: I) -> Self {
        let mut t = Self::new();
        for (k, d) in entries {
            t.set(k, d);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(v: u64) -> Goldilocks {
        Goldilocks::new(v)
    }

    fn k(seed: u64) -> [u8; 32] {
        let h = hemera::hash(&seed.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(&h.as_bytes()[..32]);
        out
    }

    fn d(seed: u64) -> Digest {
        leaf_digest(&k(seed), &[g(seed)])
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
    }

    #[test]
    fn empty_root_is_zero_and_single_leaf_is_its_digest() {
        let mut t = Trie::new();
        assert_eq!(t.root(), EMPTY);
        t.set(k(1), d(1));
        assert_eq!(t.root(), d(1));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn root_is_a_function_of_the_set() {
        let mut a = Trie::new();
        let mut b = Trie::new();
        for i in 0..64 {
            a.set(k(i), d(i));
        }
        for i in (0..64).rev() {
            b.set(k(i), d(i));
        }
        assert_eq!(a.root(), b.root());
        // remove + reinsert leaves no trace
        a.remove(&k(17));
        a.set(k(17), d(17));
        assert_eq!(a.root(), b.root());
        // removal matches a trie that never had the key
        a.remove(&k(5));
        let mut c = Trie::from_entries((0..64).filter(|&i| i != 5).map(|i| (k(i), d(i))));
        assert_eq!(a.root(), c.root());
    }

    #[test]
    fn random_batches_match_from_scratch() {
        let mut rng = Rng(7);
        let mut t = Trie::new();
        let mut model = std::collections::BTreeMap::new();
        for round in 0..200u64 {
            for _ in 0..8 {
                let key = rng.next() % 300;
                if rng.next().is_multiple_of(4) {
                    t.remove(&k(key));
                    model.remove(&k(key));
                } else {
                    let val = round * 1000 + key;
                    let digest = leaf_digest(&k(key), &[g(val)]);
                    t.set(k(key), digest);
                    model.insert(k(key), digest);
                }
            }
            let mut fresh = Trie::from_entries(model.iter().map(|(a, b)| (*a, *b)));
            assert_eq!(t.root(), fresh.root(), "round {round}");
            assert_eq!(t.len(), model.len());
        }
    }

    #[test]
    fn proofs_verify_and_bind() {
        let mut t = Trie::new();
        for i in 0..40 {
            t.set(k(i), d(i));
        }
        let root = t.root();
        for i in 0..40 {
            let p = t.prove(&k(i));
            assert!(p.verify(&root, &k(i), Some(&d(i))));
            assert!(!p.verify(&root, &k(i), None), "present key cannot be proven absent");
            assert!(!p.verify(&root, &k(i), Some(&d(i + 1))), "wrong digest rejected");
        }
        let absent = k(1000);
        let p = t.prove(&absent);
        assert!(p.verify(&root, &absent, None));
        assert!(!p.verify(&root, &absent, Some(&d(1000))));
        // a proof against another root fails
        t.set(k(41), d(41));
        assert!(!p.verify(&t.root(), &absent, None));
        // empty trie: absence of anything
        let mut e = Trie::new();
        assert!(e.prove(&absent).verify(&EMPTY, &absent, None));
    }

    #[test]
    fn adjacent_sequential_keys_compress() {
        // u64 keys in the first 8 bytes: depth stays ≈ log₂ n, not 256.
        let mut t = Trie::new();
        for i in 0..1024u64 {
            let mut key = [0u8; 32];
            key[..8].copy_from_slice(&i.to_le_bytes());
            t.set(key, leaf_digest(&key, &[g(i)]));
        }
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&777u64.to_le_bytes());
        let p = t.prove(&key);
        assert!(p.steps.len() <= 10, "depth {}", p.steps.len());
        assert!(p.verify(&t.root(), &key, Some(&leaf_digest(&key, &[g(777)]))));
    }

    /// Keys on the structural edges of the trie: the extremes of the key
    /// space, keys differing only in the first or the last bit, a full chain
    /// of single-bit keys (every branch depth 0..=255 occurs), and u64 keys at
    /// both ends of their range.
    fn boundary_keys() -> Vec<[u8; 32]> {
        let mut keys = vec![[0u8; 32], [0xFF; 32]];
        let mut last = [0u8; 32];
        last[31] = 1;
        keys.push(last);
        let mut first = [0u8; 32];
        first[0] = 0x80;
        keys.push(first);
        let mut almost = [0xFF; 32];
        almost[31] = 0xFE;
        keys.push(almost);
        for bit in 0..256u16 {
            let mut k = [0u8; 32];
            k[(bit / 8) as usize] = 0x80 >> (bit % 8);
            keys.push(k);
        }
        for v in [1u64, u64::MAX, u64::MAX - 1, 1 << 63] {
            let mut k = [0u8; 32];
            k[..8].copy_from_slice(&v.to_le_bytes());
            keys.push(k);
        }
        keys.sort();
        keys.dedup();
        keys
    }

    #[test]
    fn boundary_keys_match_from_scratch_and_prove_at_every_step() {
        let keys = boundary_keys();
        let mut rng = Rng(0xB0B);
        let mut t = Trie::new();
        let mut model = std::collections::BTreeMap::new();
        for step in 0..1500u64 {
            let key = keys[(rng.next() % keys.len() as u64) as usize];
            if rng.next().is_multiple_of(3) {
                assert_eq!(t.remove(&key), model.remove(&key).is_some());
            } else {
                let digest = leaf_digest(&key, &[g(step)]);
                assert_eq!(t.set(key, digest), model.insert(key, digest).is_none());
            }
            let mut fresh = Trie::from_entries(model.iter().map(|(a, b)| (*a, *b)));
            assert_eq!(t.root(), fresh.root(), "step {step}");
            if step % 25 == 0 {
                let root = t.root();
                for k in &keys {
                    let proof = t.prove(k);
                    assert!(proof.steps.len() <= 256);
                    assert!(proof.verify(&root, k, model.get(k)), "step {step}");
                }
            }
        }
        // the single-bit chain alone: depth 256 at the all-zero key
        let chain: Vec<[u8; 32]> = keys.iter().copied().filter(|k| k.iter().map(|b| b.count_ones()).sum::<u32>() <= 1).collect();
        let mut t = Trie::from_entries(chain.iter().map(|k| (*k, leaf_digest(k, &[]))));
        let zero = [0u8; 32];
        let proof = t.prove(&zero);
        assert_eq!(proof.steps.len(), 256, "all-zero key sits under every single-bit branch");
        assert!(proof.verify(&t.root(), &zero, Some(&leaf_digest(&zero, &[]))));
        // removing down to empty returns to EMPTY
        for k in &chain {
            assert!(t.remove(k));
        }
        assert_eq!(t.root(), EMPTY);
        assert!(t.is_empty());
    }

    #[test]
    fn the_dimension_version_enters_every_leaf() {
        let key = k(3);
        let vals = crate::dim::u64_limbs(5);
        let limbs = bytes32_limbs(&key);
        let mut acc = [g(LEAF_TAG), g(2), g(crate::dim::DIMENSION_VERSION), g(0)];
        acc = compress4(&acc, &[limbs[0], limbs[1], limbs[2], limbs[3]]);
        acc = compress4(&acc, &[limbs[4], limbs[5], limbs[6], limbs[7]]);
        acc = compress4(&acc, &[vals[0], vals[1], g(0), g(0)]);
        assert_eq!(leaf_digest(&key, &vals), acc);
        assert_eq!(crate::dim::DIMENSION_VERSION, 4);
    }

    #[test]
    fn digests_are_domain_separated() {
        let key = k(1);
        assert_ne!(leaf_digest(&key, &[]), leaf_digest(&key, &[g(0)]), "length is bound");
        assert_ne!(branch_digest(0, &d(1), &d(2)), branch_digest(1, &d(1), &d(2)), "bit index is bound");
        assert_ne!(branch_digest(0, &d(1), &d(2)), branch_digest(0, &d(2), &d(1)), "sides are bound");
    }
}
