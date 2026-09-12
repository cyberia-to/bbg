//! Isolated native transitions with bounded undo and exact persistent records.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Bbg, IntentRecord, Particle, Signal, SignalRecord, prune, state};

mod records;
mod undo;
pub use records::{Record, RecordChange, records};

pub const MAX_KEYS: usize = 65_536;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_EVENTS: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Arguments,
    Accumulator,
    DoubleSpend,
    Overflow(&'static str),
    Limit(&'static str),
    PositionExists,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Arguments => {
                f.write_str("native transition requires a signal/header or an intent")
            }
            Self::Accumulator => {
                f.write_str("native persistence does not encode checkpoint accumulators")
            }
            Self::DoubleSpend => f.write_str("duplicate or already spent nullifier"),
            Self::Overflow(field) => write!(f, "native transition overflow: {field}"),
            Self::Limit(limit) => write!(f, "native transition limit: {limit}"),
            Self::PositionExists => f.write_str("native signal position already exists"),
        }
    }
}
impl std::error::Error for Error {}

pub enum NativeChange<'a> {
    Signal {
        signal: &'a Signal,
        position: u64,
        header: &'a SignalRecord,
    },
    Intent(&'a IntentRecord),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub height: u64,
    pub root: Particle,
}

/// The mutable borrow prevents observing or modifying the candidate graph.
#[must_use = "publish only after the shared durable commit succeeds"]
pub struct Prepared<'a> {
    bbg: &'a mut Bbg,
    undo: Option<undo::Undo>,
    changes: Vec<RecordChange>,
    snapshots: Vec<Snapshot>,
}

impl Bbg {
    pub fn prepare_native(
        &mut self,
        signal: Option<&Signal>,
        header: Option<(u64, SignalRecord)>,
        intent: Option<&IntentRecord>,
    ) -> Result<Prepared<'_>, Error> {
        match (signal, header, intent) {
            (Some(signal), Some((position, header)), None) => {
                self.prepare_native_batch(&[NativeChange::Signal {
                    signal,
                    position,
                    header: &header,
                }])
            }
            (None, None, Some(intent)) => {
                self.prepare_native_batch(&[NativeChange::Intent(intent)])
            }
            _ => Err(Error::Arguments),
        }
    }

    pub fn prepare_native_batch(
        &mut self,
        events: &[NativeChange<'_>],
    ) -> Result<Prepared<'_>, Error> {
        if events.is_empty() || events.len() > MAX_EVENTS {
            return Err(Error::Limit("event count"));
        }
        if self.checkpoint.acc.is_some() {
            return Err(Error::Accumulator);
        }
        if self.prune_config.rank_floor_pct > 100 {
            return Err(Error::Limit("pruning rank floor"));
        }
        let undo = undo::Undo::new(self);
        let mut prepared = Prepared {
            bbg: self,
            undo: Some(undo),
            changes: vec![],
            snapshots: vec![],
        };
        prepared.capture(0, &[])?;
        for event in events {
            match event {
                NativeChange::Signal {
                    signal,
                    position,
                    header,
                } => prepared.signal(signal, *position, header)?,
                NativeChange::Intent(intent) => {
                    let key = intent_key(intent);
                    prepared.capture(14, &key)?;
                    prepared.bbg.apply_intent(intent);
                }
            }
            prepared.snapshots.push(Snapshot {
                height: prepared.height(),
                root: prepared.root(),
            });
        }
        prepared.collect_changes()?;
        Ok(prepared)
    }
}

impl Prepared<'_> {
    pub fn height(&self) -> u64 {
        self.bbg.state.height
    }
    pub fn root(&self) -> Particle {
        self.bbg.state.root()
    }
    pub fn changes(&self) -> &[RecordChange] {
        &self.changes
    }
    pub fn snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    /// Retain the candidate after its exact changes and receipt are durable.
    pub fn publish(mut self) {
        self.undo = None;
    }

    fn capture(&mut self, domain: u8, key: &[u8]) -> Result<(), Error> {
        self.undo
            .as_mut()
            .expect("active preparation")
            .capture(self.bbg, records::key(domain, key))
    }

    fn signal(
        &mut self,
        signal: &Signal,
        position: u64,
        header: &SignalRecord,
    ) -> Result<(), Error> {
        validate(self.bbg, signal, position, header)?;
        self.capture(4, &signal.neuron)?;
        self.capture(9, &self.bbg.state.height.to_be_bytes())?;
        self.capture(10, &position.to_be_bytes())?;
        for mv in &signal.box_moves {
            self.capture(12, &mv.nullifier)?;
            if let Some((point, _)) = mv.commitment {
                self.capture(11, &point)?;
            }
        }
        for link in &signal.links {
            let axon = state::axon_id(&link.from, &link.to);
            self.capture(1, &axon)?;
            self.capture(1, &link.to)?;
            self.capture(2, &link.from)?;
            self.capture(3, &link.to)?;
            self.capture(13, &state::balance_key(&link.from, &link.token))?;
            self.capture(13, &state::balance_key(&link.to, &link.token))?;
            self.capture(15, &axon)?;
            self.capture(16, &axon)?;
        }
        self.bbg.insert(signal).map_err(|_| Error::DoubleSpend)?;
        self.bbg.apply_signal_record(
            position,
            SignalRecord {
                neuron: header.neuron,
                network: header.network,
                link_count: header.link_count,
                block_height: header.block_height,
                proof_hash: header.proof_hash,
            },
        );
        let height = self.bbg.state.height;
        let root = self.bbg.state.root();
        self.bbg.state.time.insert(height, root);
        self.bbg.state.height += 1; // Checked before mutation.
        if self.bbg.state.height.is_multiple_of(state::EPOCH_BLOCKS) {
            self.prune()?;
        }
        self.bbg.state.refresh_root();
        self.bbg.checkpoint = self.bbg.checkpoint.advance(&self.bbg.state);
        Ok(())
    }

    fn prune(&mut self) -> Result<(), Error> {
        let epoch = self.bbg.state.height / state::EPOCH_BLOCKS;
        let (over_budget, candidates) = prune::candidates(
            &self.bbg.state,
            &self.bbg.prune_state,
            &self.bbg.prune_config,
            epoch,
            MAX_KEYS,
        )
        .map_err(|_| Error::Limit("pruning candidates"))?;
        for axon in candidates {
            if over_budget
                && prune::estimate_bytes(&self.bbg.state) <= self.bbg.prune_config.max_bytes
            {
                break;
            }
            self.capture(1, &axon)?;
            self.capture(15, &axon)?;
            self.capture(16, &axon)?;
            if let Some((from, to)) = self.bbg.state.axon_edges.get(&axon).copied() {
                self.capture(2, &from)?;
                self.capture(3, &to)?;
            }
            prune::remove_axon(&mut self.bbg.state, &mut self.bbg.prune_state, &axon);
        }
        Ok(())
    }

    fn collect_changes(&mut self) -> Result<(), Error> {
        let mut bytes = 0usize;
        for (key, old) in &self.undo.as_ref().expect("active preparation").values {
            let value = undo::bounded_read(self.bbg, key)?;
            if &value != old {
                bytes += key.len() + value.as_ref().map_or(0, Vec::len);
                if bytes > MAX_BYTES {
                    return Err(Error::Limit("write-set bytes"));
                }
                self.changes.push(RecordChange {
                    key: key.clone(),
                    value,
                });
            }
        }
        Ok(())
    }
}

impl Drop for Prepared<'_> {
    fn drop(&mut self) {
        if let Some(undo) = &self.undo {
            undo.restore(self.bbg);
        }
    }
}

fn intent_key(intent: &IntentRecord) -> Particle {
    let bytes = [
        &intent.neuron[..],
        &intent.h0.to_le_bytes(),
        &intent.scope_hash,
    ]
    .concat();
    hemera::hash(&bytes).as_bytes()[..32]
        .try_into()
        .expect("32-byte hemera digest")
}

fn validate(bbg: &Bbg, signal: &Signal, position: u64, header: &SignalRecord) -> Result<(), Error> {
    bbg.state
        .height
        .checked_add(1)
        .ok_or(Error::Overflow("height"))?;
    let count = u32::try_from(signal.links.len()).map_err(|_| Error::Overflow("link count"))?;
    if header.neuron != signal.neuron
        || header.link_count != count
        || header.block_height != signal.height
    {
        return Err(Error::Arguments);
    }
    if bbg.state.signals.contains_key(&position) {
        return Err(Error::PositionExists);
    }
    if signal.links.len().saturating_add(signal.box_moves.len()) > MAX_KEYS {
        return Err(Error::Limit("signal items"));
    }
    let mut nullifiers = BTreeSet::new();
    for mv in &signal.box_moves {
        if bbg.state.nullifiers.contains(&mv.nullifier) || !nullifiers.insert(mv.nullifier) {
            return Err(Error::DoubleSpend);
        }
    }
    let mut balances = BTreeMap::new();
    for link in &signal.links {
        let to = state::balance_key(&link.to, &link.token);
        let from = state::balance_key(&link.from, &link.token);
        let credit = balances
            .entry(to)
            .or_insert_with(|| bbg.state.balances.get(&to).copied().unwrap_or(0));
        *credit = credit
            .checked_add(link.amount)
            .ok_or(Error::Overflow("balance"))?;
        let debit = balances
            .entry(from)
            .or_insert_with(|| bbg.state.balances.get(&from).copied().unwrap_or(0));
        *debit = debit.saturating_sub(link.amount);
    }
    Ok(())
}
