//! DIAGNOSTIC (never merged, never a PR): print bbg's `root_from_leaves` for
//! the 14-leaf sample zheng's `root_matches_the_bbg_frozen_vectors` pins.
use bbg::{RootLeaves, root_from_leaves};
use nebu::Goldilocks;

fn g(v: u64) -> Goldilocks {
    Goldilocks::new(v)
}

#[test]
fn print_the_root_vectors() {
    let sample = RootLeaves {
        dims: core::array::from_fn(|d| core::array::from_fn(|k| g((d * 4 + k + 1) as u64))),
        a: [g(101), g(102), g(103), g(104)],
        n: [g(201), g(202), g(203), g(204)],
        stats: [g(301), g(302), g(303), g(304)],
    };
    let zero = RootLeaves {
        dims: [[Goldilocks::ZERO; 4]; 11],
        a: [Goldilocks::ZERO; 4],
        n: [Goldilocks::ZERO; 4],
        stats: [Goldilocks::ZERO; 4],
    };
    println!("DIAG sample {:?}", root_from_leaves(&sample).map(|v| v.as_u64()));
    println!("DIAG zero {:?}", root_from_leaves(&zero).map(|v| v.as_u64()));
}
