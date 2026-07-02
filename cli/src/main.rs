// ---
// tags: bbg, cli, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! `bbg` — the authenticated state store, on the command line.
//!
//! A low-level console for bbg's state: apply signals, advance blocks, read a
//! dimension, prove a key, check the root. It is not the signal lifecycle —
//! that is cybergraph; `apply` is the raw `insert` cybergraph calls after its
//! gate. The store is a tape-frame signal log, replayed on open (bbg is
//! signal-first: state = fold(genesis, log)). See specs/cli.md.

mod frame;

use std::path::PathBuf;

use bbg::{Bbg, Cyberlink, IntentRecord, Signal};
use clap::{Parser, Subcommand};

use frame::Event;

#[derive(Parser)]
#[command(name = "bbg", about = "the authenticated state store, on the command line")]
struct Cli {
    /// store directory (a tape-frame signal log). absent → ephemeral, in-memory.
    #[arg(long, global = true)]
    store: Option<PathBuf>,
    /// machine-readable output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// print BBG_root (hex).
    Root,
    /// print the current block height.
    Height,
    /// print graph statistics.
    Stats,
    /// read the record at (dimension, key).
    Get { dim: String, key: String },
    /// dump a dimension (or all) — human or --json.
    Dump { dim: Option<String> },
    /// apply a one-cyberlink signal (the raw insert).
    Apply {
        #[arg(long)] neuron: String,
        #[arg(long)] from: String,
        #[arg(long)] to: String,
        #[arg(long, default_value = "0")] token: String,
        #[arg(long, default_value_t = 1)] amount: u64,
        #[arg(long, default_value_t = 0)] valence: i8,
    },
    /// persist an unsealed intent (scope declaration).
    Intend {
        #[arg(long)] neuron: String,
        #[arg(long, default_value_t = 0)] h0: u64,
        #[arg(long)] scope: String,
    },
    /// finalize a block: snapshot time, advance root + height, prune at epoch.
    Finalize,
    /// run pruning now.
    Prune,
    /// prove a key in a dimension; print the value and the in-process verdict.
    Prove { dim: String, key: String },
}

fn main() {
    let cli = Cli::parse();
    std::process::exit(run(cli));
}

fn run(cli: Cli) -> i32 {
    let mut bbg = match open_store(cli.store.as_ref()) {
        Ok(b) => b,
        Err(e) => { eprintln!("store error: {e}"); return 3; }
    };

    match cli.command {
        Command::Root => println!("{}", hex(&bbg.state.root)),
        Command::Height => println!("{}", bbg.state.height),
        Command::Stats => print_stats(&bbg, cli.json),

        Command::Get { dim, key } => return cmd_get(&bbg, &dim, &key, cli.json),
        Command::Dump { dim } => print_dump(&bbg, dim.as_deref(), cli.json),

        Command::Apply { neuron, from, to, token, amount, valence } => {
            let (n, f, t, tok) = match (h32(&neuron), h32(&from), h32(&to), h32(&token)) {
                (Some(n), Some(f), Some(t), Some(tok)) => (n, f, t, tok),
                _ => { eprintln!("bad hex key"); return 3; }
            };
            if !bbg.state.neurons.contains_key(&n) {
                eprintln!("warning: neuron {} not present — no focus debit", hex(&n));
            }
            let signal = Signal {
                neuron: n,
                links: vec![Cyberlink { from: f, to: t, token: tok, amount, valence }],
                box_moves: vec![],
                height: bbg.state.height,
            };
            if let Err(e) = bbg.insert(&signal) { eprintln!("rejected: {e:?}"); return 2; }
            append(cli.store.as_ref(), &frame::encode_signal(&signal));
            println!("{}", hex(&bbg.state.root));
        }

        Command::Intend { neuron, h0, scope } => {
            let (n, s) = match (h32(&neuron), h32(&scope)) {
                (Some(n), Some(s)) => (n, s),
                _ => { eprintln!("bad hex key"); return 3; }
            };
            let intent = IntentRecord { neuron: n, h0, scope_hash: s, signature: [0u8; 64] };
            let key = bbg.apply_intent(&intent);
            append(cli.store.as_ref(), &frame::encode_intent(&intent));
            println!("{}", hex(&key));
        }

        Command::Finalize => {
            bbg.finalize_block();
            append(cli.store.as_ref(), &frame::encode_finalize());
            println!("height {}  root {}", bbg.state.height, hex(&bbg.state.root));
        }

        Command::Prune => {
            let epoch = bbg.state.height / bbg::state::EPOCH_BLOCKS;
            let before = bbg.state.axon_edges.len();
            bbg::prune::prune(&mut bbg.state, &mut bbg.prune_state, &bbg.prune_config, epoch);
            println!("pruned: axons {} → {}", before, bbg.state.axon_edges.len());
        }

        Command::Prove { dim, key } => return cmd_prove(&bbg, &dim, &key, cli.json),
    }
    0
}

// ── store: a tape-frame log, replayed on open ────────────────────────────────

fn open_store(store: Option<&PathBuf>) -> std::io::Result<Bbg> {
    let mut bbg = Bbg::new();
    let Some(dir) = store else { return Ok(bbg) };
    let log = dir.join("log");
    if !log.exists() { return Ok(bbg); }
    let bytes = std::fs::read(&log)?;
    for event in frame::decode_events(&bytes) {
        match event {
            Event::Signal(s) => { let _ = bbg.insert(&s); }
            Event::Intent(i) => { bbg.apply_intent(&i); }
            Event::Finalize => bbg.finalize_block(),
        }
    }
    Ok(bbg)
}

fn append(store: Option<&PathBuf>, frame: &[u8]) {
    let Some(dir) = store else { return };
    if let Err(e) = std::fs::create_dir_all(dir) { eprintln!("store write error: {e}"); return; }
    use std::io::Write;
    match std::fs::OpenOptions::new().create(true).append(true).open(dir.join("log")) {
        Ok(mut f) => { let _ = f.write_all(frame); }
        Err(e) => eprintln!("store write error: {e}"),
    }
}

// ── inspect ──────────────────────────────────────────────────────────────────

fn print_stats(bbg: &Bbg, json: bool) {
    let s = bbg.statistics();
    if json {
        println!(
            "{{\"node_count\":{},\"max_degree\":{},\"diameter_bound\":{},\"relation_sizes\":{:?}}}",
            s.node_count, s.max_degree, s.diameter_bound, s.relation_sizes
        );
    } else {
        println!("node_count      {}", s.node_count);
        println!("max_degree      {}", s.max_degree);
        println!("diameter_bound  {}", s.diameter_bound);
        println!("relation_sizes  {:?}", s.relation_sizes);
    }
}

fn cmd_get(bbg: &Bbg, dim: &str, key: &str, json: bool) -> i32 {
    let Some(k) = h32(key) else { eprintln!("bad hex key"); return 3 };
    let line = match dim {
        "particles" => bbg.state.particles.get(&k).map(|p| {
            if json { format!("{{\"energy\":{},\"pi_star\":{},\"weight\":{}}}", p.energy, p.pi_star, p.weight) }
            else { format!("energy {}  pi_star {}  weight {}", p.energy, p.pi_star, p.weight) }
        }),
        "neurons" => bbg.state.neurons.get(&k).map(|n| {
            if json { format!("{{\"focus\":{},\"karma\":{},\"stake\":{}}}", n.focus, n.karma, n.stake) }
            else { format!("focus {}  karma {}  stake {}", n.focus, n.karma, n.stake) }
        }),
        "axons-out" => bbg.state.axons_out.get(&k).map(|v| format!("out {}", v.len())),
        "axons-in" => bbg.state.axons_in.get(&k).map(|v| format!("in {}", v.len())),
        _ => { eprintln!("unknown or unreadable dim: {dim}"); return 3; }
    };
    match line {
        Some(l) => { println!("{l}"); 0 }
        None => { eprintln!("not found"); 1 }
    }
}

fn print_dump(bbg: &Bbg, dim: Option<&str>, _json: bool) {
    let show = |name: &str| dim.is_none() || dim == Some(name);
    if show("particles") {
        for (k, p) in &bbg.state.particles {
            println!("particles {}  energy {}  weight {}", hex(k), p.energy, p.weight);
        }
    }
    if show("neurons") {
        for (k, n) in &bbg.state.neurons {
            println!("neurons {}  focus {}  karma {}  stake {}", hex(k), n.focus, n.karma, n.stake);
        }
    }
    if show("axons") {
        for (aid, (f, t)) in &bbg.state.axon_edges {
            let w = bbg.state.particles.get(aid).map_or(0, |p| p.weight);
            println!("axon {} → {}  weight {}", hex(f), hex(t), w);
        }
    }
    if show("signals") {
        for (step, r) in &bbg.state.signals {
            println!("signal step {}  neuron {}  links {}  height {}", step, hex(&r.neuron), r.link_count, r.block_height);
        }
    }
}

// ── prove ────────────────────────────────────────────────────────────────────

fn cmd_prove(bbg: &Bbg, dim: &str, key: &str, json: bool) -> i32 {
    let Some(d) = dim_from(dim) else { eprintln!("unknown dim: {dim}"); return 3 };
    // time/signals keys are a u64 height, encoded LE into the first 8 bytes.
    let parsed = if matches!(dim, "time" | "signals") { height_key(key) } else { h32(key) };
    let Some(k) = parsed else { eprintln!("bad key"); return 3 };
    let Some(proof) = bbg::bbg_query(&bbg.state, d, &k) else { eprintln!("not found"); return 1 };
    let ok = bbg::verify_query(&proof);
    let value = hex(&proof.value_bytes);
    if json {
        println!("{{\"dim\":\"{dim}\",\"value\":\"{value}\",\"verified\":{ok}}}");
    } else {
        println!("value    {value}");
        println!("verified {}", if ok { "ok" } else { "FAIL" });
        println!("note     proof held in-process; portable proof export awaits lens serde");
    }
    if ok { 0 } else { 2 }
}

fn dim_from(s: &str) -> Option<bbg::Dim> {
    Some(match s {
        "particles" => bbg::Dim::Particles,
        "axons-out" => bbg::Dim::AxonsOut,
        "axons-in" => bbg::Dim::AxonsIn,
        "neurons" => bbg::Dim::Neurons,
        "locations" => bbg::Dim::Locations,
        "coins" => bbg::Dim::Coins,
        "cards" => bbg::Dim::Cards,
        "files" => bbg::Dim::Files,
        "time" => bbg::Dim::Time,
        "signals" => bbg::Dim::Signals,
        _ => return None,
    })
}

// ── hex ──────────────────────────────────────────────────────────────────────

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parse a hex key into 32 bytes. Accepts an optional `0x` and left-pads short
/// input with zeros (dev convenience). Pure hex — no decimal interpretation.
fn h32(s: &str) -> Option<[u8; 32]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.is_empty() || s.len() > 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
    let padded = format!("{s:0>64}");
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Parse a u64 height into a key: little-endian into the first 8 bytes,
/// matching how bbg_query reads the time/signals dimensions.
fn height_key(s: &str) -> Option<[u8; 32]> {
    let n: u64 = s.parse().ok()?;
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&n.to_le_bytes());
    Some(out)
}
