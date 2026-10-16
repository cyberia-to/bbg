//! Replay a cyberlinks CSV (`particle_from,particle_to,neuron,height,…`) into
//! bbg, one block per source height, and report the commit cost per link.
//!
//! `replay_pussy CSV [v1|v2] [amount]` — the CSV is the decompressed
//! `cyberlinks_indexed.csv` of a chain snapshot; `amount` is the conviction
//! per link (default 1; 0 = genesis semantics, topology only).
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::time::Instant;

use bbg::types::NeuronRecord;
use bbg::{Bbg, Cyberlink, RootVersion, Signal};

/// `(height, from, to, neuron)` of one cyberlink row.
type Row = (u64, [u8; 32], [u8; 32], [u8; 32]);

fn h(s: &str) -> [u8; 32] {
    let h = hemera::hash(s.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.as_bytes()[..32]);
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("CSV path");
    let mode = args.get(2).map(String::as_str).unwrap_or("v2");
    let amount: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1);
    let file = std::fs::File::open(path).expect("open csv");
    let mut rows: Vec<Row> = Vec::new();
    for line in BufReader::new(file).lines().skip(1) {
        let line = line.expect("read line");
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 4 {
            continue;
        }
        let height: u64 = f[3].parse().expect("height");
        rows.push((height, h(f[0]), h(f[1]), h(f[2])));
    }
    rows.sort_by_key(|r| r.0);
    let mut bbg = Bbg::new();
    if mode == "v2" {
        bbg.state.set_root_version(RootVersion::V2);
    }
    let mut neurons: BTreeMap<[u8; 32], ()> = BTreeMap::new();
    for r in &rows {
        if neurons.insert(r.3, ()).is_none() {
            bbg.state.neurons.insert(r.3, NeuronRecord { focus: u64::MAX / 2, karma: 0, stake: 0 });
        }
    }
    let total_links = rows.len();
    let start = Instant::now();
    let mut lap = Instant::now();
    let mut last_height = None;
    let mut blocks = 0u64;
    let mut done = 0usize;
    for (height, from, to, neuron) in rows {
        if last_height.is_some_and(|h| h != height) {
            bbg.finalize_block();
            blocks += 1;
        }
        last_height = Some(height);
        let sig = Signal {
            neuron,
            links: vec![Cyberlink { from, to, token: h("pussy"), amount, valence: 1 }],
            box_moves: vec![],
            height: bbg.state.height,
        };
        bbg.insert(&sig).unwrap();
        done += 1;
        if done.is_multiple_of(5000) {
            let dt = lap.elapsed();
            eprintln!(
                "{done:>7} links  +{:>8.3} s  ({:>8.1} us/link)  blocks={blocks} particles={}",
                dt.as_secs_f64(),
                dt.as_secs_f64() * 1e6 / 5000.0,
                bbg.state.particles.len()
            );
            lap = Instant::now();
        }
    }
    bbg.finalize_block();
    blocks += 1;
    let total = start.elapsed();
    println!(
        "mode={mode} amount={amount} links={total_links} blocks={blocks} neurons={} particles={} max_degree={} total={:.3}s per_link={:.1}us root={}",
        neurons.len(),
        bbg.state.particles.len(),
        bbg.state.statistics().max_degree,
        total.as_secs_f64(),
        total.as_secs_f64() * 1e6 / total_links as f64,
        bbg.state.root().iter().map(|x| format!("{x:02x}")).collect::<String>()
    );
}
