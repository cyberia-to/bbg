//! Commit-cost benchmark: ingest N synthetic links in blocks of B, root per block.
//!
//! `commit_cost N B [v1|v2]` — prints wall time per 10k links (flat ⇔ O(changes)).
use std::time::Instant;

use bbg::types::NeuronRecord;
use bbg::{Bbg, Cyberlink, Signal};

fn h(seed: u64) -> [u8; 32] {
    let h = hemera::hash(&seed.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.as_bytes()[..32]);
    out
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
    /// Zipf-like index: heavy head, long tail (hubs in axons_in).
    fn zipf(&mut self, n: u64) -> u64 {
        let r = self.next() % (1 << 32);
        let x = (r as f64 / (1u64 << 32) as f64).powf(3.0);
        (x * n as f64) as u64
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10_000);
    let b: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
    let mode = args.get(3).map(String::as_str).unwrap_or("v1");
    let mut bbg = Bbg::new();
    if mode == "v2" {
        bbg.state.set_root_version(bbg::state::RootVersion::V2);
    }
    const NEURONS: u64 = 1000;
    for i in 0..NEURONS {
        bbg.state.neurons.insert(h(1_000_000 + i), NeuronRecord { focus: u64::MAX / 2, karma: 0, stake: 0 });
    }
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let start = Instant::now();
    let mut lap = Instant::now();
    let mut links_in_block = 0;
    for i in 0..n {
        let from = h(2_000_000 + rng.next() % (n / 2 + 1));
        let to = h(3_000_000 + rng.zipf(n / 4 + 1));
        let sig = Signal {
            neuron: h(1_000_000 + rng.next() % NEURONS),
            links: vec![Cyberlink { from, to, token: h(7), amount: 1 + rng.next() % 1000, valence: 1 }],
            box_moves: vec![],
            height: bbg.state.height,
        };
        bbg.insert(&sig).unwrap();
        links_in_block += 1;
        if links_in_block == b {
            bbg.finalize_block();
            links_in_block = 0;
        }
        if (i + 1).is_multiple_of(10_000) {
            let dt = lap.elapsed();
            eprintln!(
                "{:>9} links  +{:>8.3} s  ({:>7.1} us/link)  particles={} axons_out={} axons_in={}",
                i + 1,
                dt.as_secs_f64(),
                dt.as_secs_f64() * 1e6 / 10_000.0,
                bbg.state.particles.len(),
                bbg.state.axons_out.len(),
                bbg.state.axons_in.len()
            );
            lap = Instant::now();
        }
    }
    if links_in_block > 0 {
        bbg.finalize_block();
    }
    let total = start.elapsed();
    let root = bbg.state.root();
    println!(
        "mode={mode} links={n} block={b} total={:.3}s per_link={:.1}us height={} root={}",
        total.as_secs_f64(),
        total.as_secs_f64() * 1e6 / n as f64,
        bbg.state.height,
        root.iter().map(|x| format!("{x:02x}")).collect::<String>()
    );
}
