use std::collections::BTreeMap;

use bbg::{
    Bbg, BoxMove, Cyberlink, IntentRecord, NeuronRecord, PruneConfig, Signal, SignalRecord,
    transition::{self, Error, NativeChange},
};

fn graph() -> Bbg {
    let mut graph = Bbg::new();
    graph.state.neurons.insert(
        [1; 32],
        NeuronRecord {
            focus: 1000,
            karma: 4,
            stake: 5,
        },
    );
    graph.state.refresh_root();
    graph.checkpoint = graph.checkpoint.advance(&graph.state);
    graph
}

fn signal(amount: u64) -> Signal {
    Signal {
        neuron: [1; 32],
        links: vec![Cyberlink {
            from: [2; 32],
            to: [3; 32],
            token: [0; 32],
            amount,
            valence: 1,
        }],
        box_moves: vec![BoxMove {
            nullifier: [7; 32],
            commitment: Some(([8; 32], 23)),
        }],
        height: 0,
    }
}

fn header(signal: &Signal) -> SignalRecord {
    SignalRecord {
        neuron: signal.neuron,
        network: [55; 32],
        link_count: signal.links.len() as u32,
        block_height: signal.height,
        proof_hash: [99; 32],
    }
}

fn records(graph: &Bbg) -> BTreeMap<Vec<u8>, Vec<u8>> {
    transition::records(graph)
        .unwrap()
        .map(|r| (r.key, r.value))
        .collect()
}

#[test]
fn dropped_preparation_restores_every_record_and_checkpoint() {
    let mut graph = graph();
    let before = records(&graph);
    let signal = signal(13);
    let old_root = graph.state.root();
    {
        let prepared = graph
            .prepare_native(Some(&signal), Some((256, header(&signal))), None)
            .unwrap();
        assert_eq!(prepared.height(), 1);
        assert_ne!(prepared.root(), old_root);
        assert!(!prepared.changes().is_empty());
    }
    assert_eq!(records(&graph), before);
    assert_eq!(graph.state.root(), old_root);
}

#[test]
fn callback_failure_rolls_back_and_later_retry_succeeds() {
    let mut graph = graph();
    let before = records(&graph);
    let signal = signal(13);
    fn attempt(
        graph: &mut Bbg,
        signal: &Signal,
        commit: impl FnOnce(&[transition::RecordChange]) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        let prepared = graph
            .prepare_native(Some(signal), Some((0, header(signal))), None)
            .unwrap();
        commit(prepared.changes())?;
        prepared.publish();
        Ok(())
    }
    let failed = attempt(&mut graph, &signal, |_| Err("durable transaction rejected"));
    assert!(failed.is_err());
    assert_eq!(records(&graph), before);
    graph
        .prepare_native(Some(&signal), Some((0, header(&signal))), None)
        .unwrap()
        .publish();
    assert_eq!(graph.state.height, 1);
    assert!(graph.state.nullifiers.contains(&[7; 32]));
}

#[test]
fn published_candidate_matches_standard_signal_finalization_and_exact_changes() {
    let mut candidate = graph();
    let mut standard = graph();
    let signal = signal(19);
    let mut disk = records(&candidate);
    standard.insert(&signal).unwrap();
    standard.apply_signal_record(9, header(&signal));
    standard.finalize_block();
    let prepared = candidate
        .prepare_native(Some(&signal), Some((9, header(&signal))), None)
        .unwrap();
    for change in prepared.changes() {
        if let Some(value) = &change.value {
            disk.insert(change.key.clone(), value.clone());
        } else {
            disk.remove(&change.key);
        }
    }
    assert_eq!(prepared.root(), standard.state.root());
    prepared.publish();
    assert_eq!(records(&candidate), disk);
    assert_eq!(records(&candidate), records(&standard));
}

#[test]
fn duplicate_nullifier_inside_one_signal_is_rejected_before_mutation() {
    let mut graph = graph();
    let before = records(&graph);
    let mut signal = signal(2);
    signal.box_moves.push(BoxMove {
        nullifier: [7; 32],
        commitment: None,
    });
    assert!(matches!(
        graph.prepare_native(Some(&signal), Some((0, header(&signal))), None),
        Err(Error::DoubleSpend)
    ));
    assert_eq!(records(&graph), before);
}

#[test]
fn overflow_is_rejected_even_when_credit_and_debit_keys_alias() {
    let mut graph = graph();
    let mut signal = signal(1);
    signal.links[0].from = signal.links[0].to;
    graph
        .state
        .balances
        .insert(bbg::balance_key(&[3; 32], &[0; 32]), u64::MAX);
    graph.state.refresh_root();
    let before = records(&graph);
    assert!(matches!(
        graph.prepare_native(Some(&signal), Some((0, header(&signal))), None),
        Err(Error::Overflow("balance"))
    ));
    assert_eq!(records(&graph), before);
    graph.state.height = u64::MAX;
    assert!(matches!(
        graph.prepare_native(Some(&signal), Some((0, header(&signal))), None),
        Err(Error::Overflow("height"))
    ));
    assert_eq!(graph.state.height, u64::MAX);
}

#[test]
fn failure_in_later_batch_event_rolls_back_earlier_signal_and_intent() {
    let mut graph = graph();
    let before = records(&graph);
    let first = signal(2);
    let second = signal(3); // Same nullifier already consumed by the first candidate.
    let first_header = header(&first);
    let second_header = header(&second);
    let intent = IntentRecord {
        neuron: [1; 32],
        h0: 0,
        scope_hash: [5; 32],
        signature: [6; 64],
    };
    let batch = [
        NativeChange::Signal {
            signal: &first,
            position: 0,
            header: &first_header,
        },
        NativeChange::Intent(&intent),
        NativeChange::Signal {
            signal: &second,
            position: 1,
            header: &second_header,
        },
    ];
    assert!(matches!(
        graph.prepare_native_batch(&batch),
        Err(Error::DoubleSpend)
    ));
    assert_eq!(records(&graph), before);
}

#[test]
fn epoch_pruning_restores_removed_records_and_adjacency_on_drop() {
    let mut graph = graph().with_prune_config(PruneConfig {
        max_bytes: 0,
        rank_floor_pct: 0,
        half_life_epochs: 0,
    });
    let mut old_signal = signal(9);
    old_signal.box_moves.clear();
    graph.insert(&old_signal).unwrap();
    graph.state.height = 99;
    graph.state.refresh_root();
    graph.checkpoint = graph.checkpoint.advance(&graph.state);
    let before = records(&graph);
    let next = signal(3);
    let prepared = graph
        .prepare_native(Some(&next), Some((7, header(&next))), None)
        .unwrap();
    assert_eq!(prepared.height(), 100);
    assert!(
        prepared
            .changes()
            .iter()
            .any(|change| change.value.is_none())
    );
    drop(prepared);
    assert_eq!(records(&graph), before);
    graph
        .prepare_native(Some(&next), Some((7, header(&next))), None)
        .unwrap()
        .publish();
    assert!(graph.state.particles.is_empty());
    assert!(graph.state.axons_out.is_empty());
    assert_eq!(graph.state.root(), graph.state.compute_root());
}

#[test]
fn intent_only_persists_exact_record_and_retains_current_root_and_height() {
    let mut graph = graph();
    let before = graph.state.root();
    let intent = IntentRecord {
        neuron: [1; 32],
        h0: u64::MAX,
        scope_hash: [3; 32],
        signature: [4; 64],
    };
    let prepared = graph.prepare_native(None, None, Some(&intent)).unwrap();
    assert_eq!(prepared.height(), 0);
    assert_eq!(prepared.root(), before);
    assert_eq!(prepared.changes().len(), 1);
    assert_eq!(prepared.changes()[0].key[0], 14);
    assert_eq!(
        &prepared.changes()[0].value.as_ref().unwrap()[32..40],
        &u64::MAX.to_le_bytes()
    );
    prepared.publish();
    assert_eq!(graph.state.intents.len(), 1);
}

#[test]
fn typed_records_preserve_raw_integer_bytes_and_sort_numeric_keys() {
    let mut graph = graph();
    graph.state.time.insert(255, [1; 32]);
    graph.state.time.insert(256, [2; 32]);
    graph.state.balances.insert([0xff; 32], u64::MAX);
    let records: Vec<_> = transition::records(&graph).unwrap().collect();
    assert!(records.windows(2).all(|pair| pair[0].key < pair[1].key));
    let balance = records
        .iter()
        .find(|r| r.key == [vec![13], vec![0xff; 32]].concat())
        .unwrap();
    assert_eq!(balance.value, u64::MAX.to_le_bytes());
    let time: Vec<_> = records.iter().filter(|r| r.key[0] == 9).collect();
    assert_eq!(time[0].value, [1; 32]);
    assert_eq!(time[1].value, [2; 32]);
}

#[test]
fn preparation_touches_only_signal_neighborhood_in_large_graph() {
    let mut graph = graph();
    for n in 0..1000u64 {
        let mut key = [0; 32];
        key[..8].copy_from_slice(&n.to_le_bytes());
        graph.state.balances.insert(key, n);
    }
    graph.state.refresh_root();
    let signal = signal(2);
    let prepared = graph
        .prepare_native(Some(&signal), Some((0, header(&signal))), None)
        .unwrap();
    assert!(prepared.changes().len() < 20);
}

#[test]
fn repeated_positions_and_inconsistent_headers_are_rejected() {
    let mut graph = graph();
    let mut signal = signal(1);
    signal.box_moves.clear();
    graph
        .prepare_native(Some(&signal), Some((123, header(&signal))), None)
        .unwrap()
        .publish();
    let before = records(&graph);
    assert!(matches!(
        graph.prepare_native(Some(&signal), Some((123, header(&signal))), None),
        Err(Error::PositionExists)
    ));
    let mut wrong = header(&signal);
    wrong.neuron = [0; 32];
    assert!(matches!(
        graph.prepare_native(Some(&signal), Some((124, wrong)), None),
        Err(Error::Arguments)
    ));
    assert_eq!(records(&graph), before);
}

#[test]
fn successful_batch_coalesces_overlapping_keys_and_keeps_each_snapshot() {
    let mut graph = graph();
    let mut baseline = self::graph();
    let mut disk = records(&graph);
    let first = signal(3);
    let mut second = signal(5);
    second.box_moves[0].nullifier = [9; 32];
    let first_header = header(&first);
    let second_header = header(&second);
    let intent = IntentRecord {
        neuron: [1; 32],
        h0: 1,
        scope_hash: [5; 32],
        signature: [6; 64],
    };
    baseline.insert(&first).unwrap();
    baseline.apply_signal_record(0, header(&first));
    baseline.finalize_block();
    let first_root = baseline.state.root();
    baseline.apply_intent(&intent);
    baseline.insert(&second).unwrap();
    baseline.apply_signal_record(1, header(&second));
    baseline.finalize_block();
    let prepared = graph
        .prepare_native_batch(&[
            NativeChange::Signal {
                signal: &first,
                position: 0,
                header: &first_header,
            },
            NativeChange::Intent(&intent),
            NativeChange::Signal {
                signal: &second,
                position: 1,
                header: &second_header,
            },
        ])
        .unwrap();
    assert_eq!(
        prepared
            .snapshots()
            .iter()
            .map(|s| s.height)
            .collect::<Vec<_>>(),
        vec![1, 1, 2]
    );
    assert_eq!(prepared.snapshots()[0].root, first_root);
    assert_eq!(prepared.snapshots()[1].root, first_root);
    assert_eq!(prepared.snapshots()[2].root, baseline.state.root());
    for change in prepared.changes() {
        if let Some(value) = &change.value {
            disk.insert(change.key.clone(), value.clone());
        } else {
            disk.remove(&change.key);
        }
    }
    prepared.publish();
    assert_eq!(records(&graph), disk);
    assert_eq!(records(&graph), records(&baseline));
}

#[test]
fn empty_and_oversized_batches_fail_without_changing_state() {
    let mut graph = graph();
    let before = records(&graph);
    assert!(matches!(
        graph.prepare_native_batch(&[]),
        Err(Error::Limit("event count"))
    ));
    let intent = IntentRecord {
        neuron: [1; 32],
        h0: 0,
        scope_hash: [5; 32],
        signature: [6; 64],
    };
    let batch: Vec<_> = (0..65).map(|_| NativeChange::Intent(&intent)).collect();
    assert!(matches!(
        graph.prepare_native_batch(&batch),
        Err(Error::Limit("event count"))
    ));
    assert_eq!(records(&graph), before);
}
