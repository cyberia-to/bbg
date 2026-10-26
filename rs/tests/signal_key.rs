// ---
// tags: bbg, rust, test
// crystal-type: source
// crystal-domain: cyber
// ---
//! The signals dimension is keyed by `signal_key(neuron, step) = H(neuron ‖ step)`
//! (dimension version 3, specs/state.md rule 7; launch row 9).
//!
//! Before: `signals` was keyed by the bare per-neuron `step`. Every neuron's
//! first signal is step 0, so two neurons' first signals shared one key, the
//! later insert overwrote the earlier record, and which record survived (and
//! so the root) depended on arrival order (bbg#31 demonstrated it, bbg#32
//! proposed the fix). These tests pin the adversarial cases: colliding steps
//! across neurons, every arrival order, the native transition path, replay and
//! recovery.

use std::collections::BTreeMap;

use bbg::transition::{self, Error, NativeChange};
use bbg::{
    Bbg, Cyberlink, Dim, Keyed, NeuronRecord, RootVersion, Signal, SignalRecord, bbg_query,
    signal_key, verify_entity,
};

fn p(seed: u8) -> [u8; 32] {
    [seed; 32]
}

fn record(neuron: [u8; 32], step: u64, network: u8) -> SignalRecord {
    SignalRecord {
        neuron,
        network: p(network),
        link_count: 1,
        block_height: step,
        proof_hash: p(network ^ 0x5A),
        step,
    }
}

fn link_signal(neuron: [u8; 32], to: u8, amount: u64, height: u64) -> Signal {
    Signal {
        neuron,
        links: vec![Cyberlink { from: p(200), to: p(to), token: p(0), amount, valence: 1 }],
        box_moves: vec![],
        height,
    }
}

fn header(signal: &Signal, step: u64) -> SignalRecord {
    SignalRecord {
        neuron: signal.neuron,
        network: p(77),
        link_count: signal.links.len() as u32,
        block_height: signal.height,
        proof_hash: p(78),
        step,
    }
}

fn funded() -> Bbg {
    let mut b = Bbg::new();
    for i in 1..=8u8 {
        b.state.neurons.insert(p(i), NeuronRecord { focus: 1 << 40, karma: 0, stake: 0, nonce: 0 });
    }
    b.state.refresh_root();
    b.checkpoint = b.checkpoint.advance(&b.state);
    b
}

fn records(b: &Bbg) -> BTreeMap<Vec<u8>, Vec<u8>> {
    transition::records(b).unwrap().map(|r| (r.key, r.value)).collect()
}

/// Deterministic permutation of `0..n` (xorshift Fisher–Yates), integers only.
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut x = seed | 1;
    for i in (1..v.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
    v
}

#[test]
fn signal_key_binds_neuron_and_step() {
    // H(neuron ‖ step_le), 40-byte preimage
    let mut preimage = p(1).to_vec();
    preimage.extend_from_slice(&3u64.to_le_bytes());
    let expected: [u8; 32] = hemera::hash(&preimage).as_bytes()[..32].try_into().unwrap();
    assert_eq!(signal_key(&p(1), 3), expected);
    assert_ne!(signal_key(&p(1), 0), signal_key(&p(2), 0), "neurons separate");
    assert_ne!(signal_key(&p(1), 0), signal_key(&p(1), 1), "steps separate");
    // the step is not readable from the key: the old layout (step LE in the
    // first 8 bytes) is not what the dimension is keyed by
    let mut bare = [0u8; 32];
    bare[..8].copy_from_slice(&0u64.to_le_bytes());
    assert_ne!(signal_key(&p(0), 0), bare);
}

#[test]
fn two_neurons_first_signals_both_persist() {
    for version in [RootVersion::V1, RootVersion::V2] {
        let mut b = Bbg::new().with_root_version(version);
        let k1 = b.apply_signal_record(record(p(1), 0, 10));
        let k2 = b.apply_signal_record(record(p(2), 0, 20));
        assert_ne!(k1, k2);
        assert_eq!(b.state.signals.len(), 2, "{version:?}: the step-0 collision is gone");
        let r1 = b.state.signals.get(&k1).unwrap();
        let r2 = b.state.signals.get(&k2).unwrap();
        assert_eq!((r1.neuron, r1.step, r1.network), (p(1), 0, p(10)));
        assert_eq!((r2.neuron, r2.step, r2.network), (p(2), 0, p(20)));
        assert_eq!(b.state.statistics().relation_sizes[Dim::Signals as usize], 2);
    }
}

#[test]
fn arrival_order_does_not_move_the_root() {
    // 8 neurons × 6 steps: 8 records share every step value
    let all: Vec<SignalRecord> = (1..=8u8)
        .flat_map(|n| (0..6u64).map(move |s| record(p(n), s, n * 16 + s as u8)))
        .collect();
    for version in [RootVersion::V1, RootVersion::V2] {
        let build = |order: &[usize]| {
            let mut b = Bbg::new().with_root_version(version);
            for &i in order {
                b.apply_signal_record(SignalRecord { ..all[i] });
            }
            b
        };
        let in_order: Vec<usize> = (0..all.len()).collect();
        let reference = build(&in_order);
        assert_eq!(reference.state.signals.len(), all.len());
        for seed in [1u64, 7, 0xDEAD_BEEF, 1 << 40] {
            let other = build(&shuffled(all.len(), seed));
            assert_eq!(other.state.root(), reference.state.root(), "{version:?} seed {seed}");
            assert_eq!(other.state.root_leaves().dims[9], reference.state.root_leaves().dims[9]);
        }
        // reversed: the exact scenario of bbg#31 at scale
        let reversed: Vec<usize> = (0..all.len()).rev().collect();
        assert_eq!(build(&reversed).state.root(), reference.state.root(), "{version:?}");
    }
}

#[test]
fn every_record_is_under_the_root_and_provable() {
    let mut v2 = Bbg::new();
    let mut v1 = Bbg::new().with_root_version(RootVersion::V1);
    for b in [&mut v1, &mut v2] {
        b.apply_signal_record(record(p(1), 0, 10));
        b.apply_signal_record(record(p(2), 0, 20));
        b.state.refresh_root();
    }
    // v2: a trie path to each record, under the signals dimension leaf
    let leaves = v2.state.root_leaves();
    for (neuron, net) in [(p(1), 10), (p(2), 20)] {
        let key = signal_key(&neuron, 0);
        let leaf = Keyed::<[u8; 32], SignalRecord>::leaf(&key, &record(neuron, 0, net));
        assert!(v2.state.signals.prove(&key).verify(&leaves.dims[9], &key, Some(&leaf)));
    }
    // v1: an authenticated entity read by (neuron, step)
    let root = v1.state.root();
    let proof = v1.prove_signal(&p(2), 0).unwrap();
    assert!(verify_entity(&proof, &root, Dim::Signals, &signal_key(&p(2), 0)));
    assert!(!verify_entity(&proof, &root, Dim::Signals, &signal_key(&p(1), 0)));
    assert!(bbg_query(&v1.state, Dim::Signals, &signal_key(&p(3), 0)).is_none());
    // the record's step is committed: the same record at another step is another root
    let mut moved = Bbg::new();
    moved.apply_signal_record(record(p(1), 1, 10));
    moved.apply_signal_record(record(p(2), 0, 20));
    assert_ne!(moved.state.root(), v2.state.root());
}

#[test]
fn native_batch_keeps_both_step_zero_signals_and_rejects_a_repeat() {
    let mut b = funded();
    let before = records(&b);
    let (a, c) = (link_signal(p(1), 30, 5, 0), link_signal(p(2), 31, 6, 0));
    let (ha, hc) = (header(&a, 0), header(&c, 0));
    let batch = [
        NativeChange::Signal { signal: &a, header: &ha },
        NativeChange::Signal { signal: &c, header: &hc },
    ];
    {
        let prepared = b.prepare_native_batch(&batch).unwrap();
        let written: Vec<_> = prepared.changes().iter().filter(|ch| ch.key[0] == 10).collect();
        assert_eq!(written.len(), 2, "two signal records, not one overwritten");
        for ch in &written {
            let value = ch.value.as_ref().unwrap();
            assert_eq!(value.len(), 116);
            assert_eq!(&value[108..], &0u64.to_le_bytes(), "the step is in the record");
        }
    } // dropped: rolled back
    assert_eq!(records(&b), before);
    b.prepare_native_batch(&batch).unwrap().publish();
    assert_eq!(b.state.signals.len(), 2);
    for (n, key) in [(p(1), signal_key(&p(1), 0)), (p(2), signal_key(&p(2), 0))] {
        assert_eq!(b.state.signals.get(&key).unwrap().neuron, n);
        assert!(records(&b).contains_key(&[&[10u8][..], &key].concat()));
    }
    // the same (neuron, step) again is rejected, state untouched
    let after = records(&b);
    let again = link_signal(p(1), 32, 1, 1);
    assert!(matches!(
        b.prepare_native(Some(&again), Some(header(&again, 0)), None),
        Err(Error::SignalExists)
    ));
    assert_eq!(records(&b), after);
    // its next step is accepted
    b.prepare_native(Some(&again), Some(header(&again, 1)), None).unwrap().publish();
    assert_eq!(b.state.signals.len(), 3);
}

/// A fixed script of native batches: 8 neurons, each with its own chain
/// starting at step 0, interleaved, some batches carrying two signals.
fn script() -> Vec<Vec<(Signal, SignalRecord)>> {
    let mut steps = [0u64; 9];
    let mut x = 0x5157_u64;
    let mut batches = Vec::new();
    for h in 0..40u64 {
        let mut batch = Vec::new();
        for _ in 0..(1 + h % 2) {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let n = 1 + (x % 8) as u8;
            let s = link_signal(p(n), 40 + (x >> 8) as u8 % 16, (x >> 16) % 4, h);
            let r = header(&s, steps[n as usize]);
            steps[n as usize] += 1;
            batch.push((s, r));
        }
        batches.push(batch);
    }
    batches
}

fn run(b: &mut Bbg, disk: &mut BTreeMap<Vec<u8>, Vec<u8>>) -> Vec<transition::Snapshot> {
    let mut snapshots = Vec::new();
    for batch in script() {
        let changes: Vec<_> = batch
            .iter()
            .map(|(signal, header)| NativeChange::Signal { signal, header })
            .collect();
        let prepared = b.prepare_native_batch(&changes).unwrap();
        for ch in prepared.changes() {
            match &ch.value {
                Some(v) => disk.insert(ch.key.clone(), v.clone()),
                None => disk.remove(&ch.key),
            };
        }
        snapshots.extend_from_slice(prepared.snapshots());
        prepared.publish();
    }
    snapshots
}

#[test]
fn replay_is_deterministic_and_recovery_matches_every_stored_root() {
    let mut live = funded();
    let mut disk = records(&live);
    let snapshots = run(&mut live, &mut disk);
    let total: usize = script().iter().map(Vec::len).sum();
    assert_eq!(live.state.signals.len(), total, "no signal lost to a shared step");
    assert_eq!(records(&live), disk, "the persisted change stream is the state");

    // recovery: replay the same log from genesis, check every stored root
    let mut recovered = funded();
    let mut recovered_disk = records(&recovered);
    assert_eq!(run(&mut recovered, &mut recovered_disk), snapshots);
    assert_eq!(live.state.time.len(), recovered.state.time.len());
    for (h, root) in live.state.time.iter() {
        assert_eq!(recovered.state.time.get(h), Some(root), "stored root at {h}");
    }
    assert_eq!(recovered.state.root(), live.state.root());
    assert_eq!(recovered_disk, disk);
}
