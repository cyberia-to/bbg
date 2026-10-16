// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! The BBG root: a left fold of hemera compressions over 14 leaves.
//!
//! [`compress4`] is the first four elements of the hemera permutation over
//! `[a ‖ b ‖ 0⁸]`. The root is the fold of `compress4` over the 11 dimension
//! commitments ‖ A ‖ N ‖ stats from the domain-tagged IV `[ROOT_TAG, 0, 0, 0]`.
//!
//! This layout was defined in zheng's `ccs::root` for the retired look
//! argument; it moved here, with identical output, when that argument left
//! zheng's default build. `root_matches_the_frozen_vectors` pins it.

use hemera::field::Goldilocks as HGold;
use hemera::permutation::permute;
use nebu::Goldilocks;

/// The 14 leaves of the BBG root preimage, each a 4-limb digest.
///
/// Order is frozen: the 11 public dimensions in `Dim` order, then A (private
/// commitments), N (nullifiers), stats (committed graph statistics).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootLeaves {
    pub dims: [[Goldilocks; 4]; 11],
    pub a: [Goldilocks; 4],
    pub n: [Goldilocks; 4],
    pub stats: [Goldilocks; 4],
}

impl RootLeaves {
    /// The leaves in committed order.
    pub fn ordered(&self) -> [[Goldilocks; 4]; 14] {
        let mut out = [[Goldilocks::ZERO; 4]; 14];
        out[..11].copy_from_slice(&self.dims);
        out[11] = self.a;
        out[12] = self.n;
        out[13] = self.stats;
        out
    }
}

/// Domain tag for the root chain IV: `"bbg-root"` as a little-endian u64.
pub const ROOT_TAG: u64 = u64::from_le_bytes(*b"bbg-root");

/// The chain IV: `[ROOT_TAG, 0, 0, 0]`.
pub fn root_iv() -> [Goldilocks; 4] {
    [Goldilocks::new(ROOT_TAG), Goldilocks::ZERO, Goldilocks::ZERO, Goldilocks::ZERO]
}

/// One hemera compression: the first 4 elements of `P([a ‖ b ‖ 0⁸])`.
pub fn compress4(a: &[Goldilocks; 4], b: &[Goldilocks; 4]) -> [Goldilocks; 4] {
    let mut state = [HGold::ZERO; 16];
    for i in 0..4 {
        state[i] = HGold::new(a[i].canonicalize().as_u64());
        state[4 + i] = HGold::new(b[i].canonicalize().as_u64());
    }
    permute(&mut state);
    core::array::from_fn(|i| Goldilocks::new(state[i].as_canonical_u64()))
}

/// The BBG root: left fold of [`compress4`] over the ordered leaves from the IV.
pub fn root_from_leaves(leaves: &RootLeaves) -> [Goldilocks; 4] {
    leaves.ordered().iter().fold(root_iv(), |acc, leaf| compress4(&acc, leaf))
}

/// Pack root limbs as 32 bytes: limb i little-endian at bytes `[8i, 8i+8)` —
/// the packing of `BbgState::root()`.
pub fn root_to_bytes(root: &[Goldilocks; 4]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, limb) in root.iter().enumerate() {
        out[i * 8..(i + 1) * 8].copy_from_slice(&limb.as_u64().to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(v: u64) -> Goldilocks {
        Goldilocks::new(v)
    }

    fn sample_leaves() -> RootLeaves {
        RootLeaves {
            dims: core::array::from_fn(|d| core::array::from_fn(|k| g((d * 4 + k + 1) as u64))),
            a: [g(101), g(102), g(103), g(104)],
            n: [g(201), g(202), g(203), g(204)],
            stats: [g(301), g(302), g(303), g(304)],
        }
    }

    fn zero_leaves() -> RootLeaves {
        RootLeaves {
            dims: [[Goldilocks::ZERO; 4]; 11],
            a: [Goldilocks::ZERO; 4],
            n: [Goldilocks::ZERO; 4],
            stats: [Goldilocks::ZERO; 4],
        }
    }

    /// Values produced by zheng 0.4 `ccs::root_from_leaves` (feature
    /// `legacy`) before the move; the root of every existing state depends
    /// on them.
    #[test]
    fn root_matches_the_frozen_vectors() {
        assert_eq!(root_from_leaves(&sample_leaves()).map(|v| v.as_u64()), SAMPLE_ROOT);
        assert_eq!(root_from_leaves(&zero_leaves()).map(|v| v.as_u64()), ZERO_ROOT);
    }

    const SAMPLE_ROOT: [u64; 4] =
        [1839648704373557657, 16245063762079522430, 2767124943183982576, 17812001250628018905];
    const ZERO_ROOT: [u64; 4] =
        [9231417576306038954, 11425091536555908058, 3672555888010110395, 14299998217756054807];

    /// `BbgState::new().root()` under each root version, dimension version 3.
    /// The trie (v2) empty root has every dimension leaf `EMPTY`: the fold over
    /// zero dimension leaves and the stats leaf. The flat (v1) empty root is
    /// unchanged by dimension version 3 (empty tables carry only the
    /// `TABLE_VERSION` header).
    #[test]
    fn empty_state_roots_are_pinned() {
        let mut s = crate::BbgState::new();
        assert_eq!(hex(&s.root()), "515dbb7ebaf6b868442827f72c6926e79e5f11fef77ff91b47839b6e68ec375d");
        s.set_root_version(crate::RootVersion::V1);
        assert_eq!(hex(&s.root()), "65ff61321169d84535f72bb177c5d6f98811369c8f51221dc846be87064b2445");
    }

    fn hex(b: &[u8; 32]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn root_binds_every_leaf() {
        let base = root_from_leaves(&sample_leaves());
        for i in 0..14 {
            for k in 0..4 {
                let mut l = sample_leaves().ordered();
                l[i][k] += g(1);
                let leaves = RootLeaves {
                    dims: l[..11].try_into().unwrap(),
                    a: l[11],
                    n: l[12],
                    stats: l[13],
                };
                assert_ne!(root_from_leaves(&leaves), base, "leaf {i} limb {k}");
            }
        }
    }

    #[test]
    fn compress_is_order_sensitive() {
        let a = [g(1), g(2), g(3), g(4)];
        let b = [g(5), g(6), g(7), g(8)];
        assert_ne!(compress4(&a, &b), compress4(&b, &a));
    }
}
