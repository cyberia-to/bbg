use crate::{Bbg, types::*};

use super::Error;

/// Identifies the typed record layout and its commitment semantics together.
/// Root-changing updates must allocate a new version (specs/native-state.md).
pub const NATIVE_RECORD_VERSION: u32 = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum MetadataError {
    Malformed,
    Unsupported(u32),
}

/// Validate the metadata framing before an owning adapter begins replay.
/// Version 1 additionally requires exact replay under the current root contract.
pub fn native_metadata_version(bytes: &[u8]) -> Result<u32, MetadataError> {
    let word = bytes.get(..4).ok_or(MetadataError::Malformed)?;
    let version = u32::from_le_bytes(word.try_into().unwrap());
    if version != 1 && version != NATIVE_RECORD_VERSION {
        return Err(MetadataError::Unsupported(version));
    }
    // The optional diameter is the only variable-width part of this record.
    if !matches!(
        (bytes.get(84), bytes.len()),
        (Some(0), 102) | (Some(1), 110)
    ) {
        return Err(MetadataError::Malformed);
    }
    Ok(version)
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn metadata_version_checks_exact_framing_and_distinguishes_unsupported_readers() {
        let graph = Bbg::new();
        let mut bytes = metadata(&graph);
        assert_eq!(native_metadata_version(&bytes), Ok(2));
        bytes[..4].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(native_metadata_version(&bytes), Ok(1));
        for length in 0..bytes.len() {
            assert!(native_metadata_version(&bytes[..length]).is_err());
        }
        bytes.push(0);
        assert_eq!(
            native_metadata_version(&bytes),
            Err(MetadataError::Malformed)
        );
        assert_eq!(
            native_metadata_version(&9u32.to_le_bytes()),
            Err(MetadataError::Unsupported(9))
        );
        bytes.pop();
        bytes[84] = 2;
        assert_eq!(
            native_metadata_version(&bytes),
            Err(MetadataError::Malformed)
        );
        bytes[84] = 1;
        bytes.splice(85..85, 7u64.to_le_bytes());
        assert_eq!(native_metadata_version(&bytes), Ok(1));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordChange {
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}

pub(crate) fn key(domain: u8, entity: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(1 + entity.len());
    key.push(domain);
    key.extend_from_slice(entity);
    key
}

pub(crate) fn integers(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub(crate) fn particle(p: &ParticleRecord) -> Vec<u8> {
    integers(&[p.energy, p.pi_star, p.weight, p.s_yes, p.s_no, p.meta_score])
}

pub(crate) fn adjacency(entries: &[Particle]) -> Vec<u8> {
    let mut bytes = integers(&[entries.len() as u64]);
    bytes.extend(entries.iter().flatten());
    bytes
}

pub(crate) fn neuron(n: &NeuronRecord) -> Vec<u8> {
    integers(&[n.focus, n.karma, n.stake])
}

pub(crate) fn signal(s: &SignalRecord) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(108);
    bytes.extend(s.neuron);
    bytes.extend(s.network);
    bytes.extend(s.link_count.to_le_bytes());
    bytes.extend(s.block_height.to_le_bytes());
    bytes.extend(s.proof_hash);
    bytes
}

pub(crate) fn intent(i: &IntentRecord) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(136);
    bytes.extend(i.neuron);
    bytes.extend(i.h0.to_le_bytes());
    bytes.extend(i.scope_hash);
    bytes.extend(i.signature);
    bytes
}

pub(crate) fn metadata(bbg: &Bbg) -> Vec<u8> {
    let mut bytes = NATIVE_RECORD_VERSION.to_le_bytes().to_vec();
    bytes.extend(bbg.state.height.to_le_bytes());
    bytes.extend(bbg.state.root());
    bytes.extend(bbg.checkpoint.height.to_le_bytes());
    bytes.extend(bbg.checkpoint.root);
    bytes.push(u8::from(bbg.state.diameter_override.is_some()));
    if let Some(diameter) = bbg.state.diameter_override {
        bytes.extend(diameter.to_le_bytes());
    }
    bytes.extend(bbg.prune_config.max_bytes.to_le_bytes());
    bytes.push(bbg.prune_config.rank_floor_pct);
    bytes.extend(bbg.prune_config.half_life_epochs.to_le_bytes());
    bytes
}

/// Exact persisted state, ordered by byte key. Native profiles have no accumulator.
pub fn records(bbg: &Bbg) -> Result<impl Iterator<Item = Record> + '_, Error> {
    if bbg.checkpoint.acc.is_some() {
        return Err(Error::Accumulator);
    }
    let s = &bbg.state;
    type Stream<'a> = Box<dyn Iterator<Item = Record> + 'a>;
    let record = |d, k: &[u8], value| Record {
        key: key(d, k),
        value,
    };
    let streams: Vec<Stream<'_>> =
        vec![
            Box::new(std::iter::once(record(0, &[], metadata(bbg)))),
            Box::new(
                s.particles
                    .iter()
                    .map(move |(k, v)| record(1, k, particle(v))),
            ),
            Box::new(
                s.axons_out
                    .iter()
                    .map(move |(k, v)| record(2, k, adjacency(v))),
            ),
            Box::new(
                s.axons_in
                    .iter()
                    .map(move |(k, v)| record(3, k, adjacency(v))),
            ),
            Box::new(s.neurons.iter().map(move |(k, v)| record(4, k, neuron(v)))),
            Box::new(s.locations.iter().map(move |(k, v)| {
                record(5, k, [v.lat.to_le_bytes(), v.lon.to_le_bytes()].concat())
            })),
            Box::new(
                s.coins
                    .iter()
                    .map(move |(k, v)| record(6, k, integers(&[v.total_supply]))),
            ),
            Box::new(
                s.cards
                    .iter()
                    .map(move |(k, v)| record(7, k, [v.owner, v.particle].concat())),
            ),
            Box::new(s.files.iter().map(move |(k, v)| {
                let mut bytes = vec![u8::from(v.available)];
                bytes.extend(v.chunk_count.to_le_bytes());
                record(8, k, bytes)
            })),
            Box::new(
                s.time
                    .iter()
                    .map(move |(k, v)| record(9, &k.to_be_bytes(), v.to_vec())),
            ),
            Box::new(
                s.signals
                    .iter()
                    .map(move |(k, v)| record(10, &k.to_be_bytes(), signal(v))),
            ),
            Box::new(
                s.commitments
                    .iter()
                    .map(move |(k, v)| record(11, k, integers(&[v.as_u64()]))),
            ),
            Box::new(s.nullifiers.iter().map(move |k| record(12, k, vec![]))),
            Box::new(
                s.balances
                    .iter()
                    .map(move |(k, v)| record(13, k, integers(&[*v]))),
            ),
            Box::new(s.intents.iter().map(move |(k, v)| record(14, k, intent(v)))),
            Box::new(
                s.axon_edges
                    .iter()
                    .map(move |(k, v)| record(15, k, [v.0, v.1].concat())),
            ),
            Box::new(
                bbg.prune_state
                    .last_touched
                    .iter()
                    .map(move |(k, v)| record(16, k, integers(&[*v]))),
            ),
        ];
    Ok(streams.into_iter().flatten())
}

/// Reads only addresses selected by the preparation, without a graph scan.
pub(crate) fn read(bbg: &Bbg, key: &[u8]) -> Option<Vec<u8>> {
    let s = &bbg.state;
    let entity = || -> [u8; 32] { key[1..].try_into().expect("internal entity key") };
    let position = || u64::from_be_bytes(key[1..].try_into().expect("internal position key"));
    match key[0] {
        0 => Some(metadata(bbg)),
        1 => s.particles.get(&entity()).map(particle),
        2 => s.axons_out.get(&entity()).map(|v| adjacency(v)),
        3 => s.axons_in.get(&entity()).map(|v| adjacency(v)),
        4 => s.neurons.get(&entity()).map(neuron),
        9 => s.time.get(&position()).map(|v| v.to_vec()),
        10 => s.signals.get(&position()).map(signal),
        11 => s
            .commitments
            .get(&entity())
            .map(|v| integers(&[v.as_u64()])),
        12 => s.nullifiers.contains(&entity()).then(Vec::new),
        13 => s.balances.get(&entity()).map(|v| integers(&[*v])),
        14 => s.intents.get(&entity()).map(intent),
        15 => s.axon_edges.get(&entity()).map(|v| [v.0, v.1].concat()),
        16 => bbg
            .prune_state
            .last_touched
            .get(&entity())
            .map(|v| integers(&[*v])),
        _ => unreachable!("internal transition domain"),
    }
}
