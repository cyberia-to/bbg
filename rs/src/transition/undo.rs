//! Private decoder for snapshots produced by this module's own encoder.
//! External bytes never enter this decoder.

use std::collections::BTreeMap;

use nebu::Goldilocks;

use crate::{Bbg, types::*};

use super::{Error, MAX_BYTES, MAX_KEYS, records};

pub(super) struct Undo {
    pub values: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    bytes: usize,
    height: u64,
    checkpoint_root: Particle,
    checkpoint_height: u64,
}

impl Undo {
    pub fn new(bbg: &Bbg) -> Self {
        Self {
            values: BTreeMap::new(),
            bytes: 0,
            height: bbg.state.height,
            checkpoint_root: bbg.checkpoint.root,
            checkpoint_height: bbg.checkpoint.height,
        }
    }

    pub fn capture(&mut self, bbg: &Bbg, key: Vec<u8>) -> Result<(), Error> {
        if self.values.contains_key(&key) {
            return Ok(());
        }
        if self.values.len() == MAX_KEYS {
            return Err(Error::Limit("undo keys"));
        }
        let value = bounded_read(bbg, &key)?;
        self.bytes += key.len() + value.as_ref().map_or(0, Vec::len);
        if self.bytes > MAX_BYTES {
            return Err(Error::Limit("undo bytes"));
        }
        self.values.insert(key, value);
        Ok(())
    }

    pub fn restore(&self, bbg: &mut Bbg) {
        for (key, value) in &self.values {
            restore(bbg, key, value.as_deref());
        }
        bbg.state.height = self.height;
        bbg.checkpoint.root = self.checkpoint_root;
        bbg.checkpoint.height = self.checkpoint_height;
        bbg.state.refresh_root();
    }
}

pub(super) fn bounded_read(bbg: &Bbg, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    if key[0] == 2 || key[0] == 3 {
        let entity: Particle = key[1..].try_into().expect("internal adjacency key");
        let adjacency = if key[0] == 2 {
            &bbg.state.axons_out
        } else {
            &bbg.state.axons_in
        };
        if adjacency
            .get(&entity)
            .is_some_and(|v| v.len() > (MAX_BYTES - 8) / 32)
        {
            return Err(Error::Limit("adjacency bytes"));
        }
    }
    Ok(records::read(bbg, key))
}

struct Reader<'a>(&'a [u8]);
impl Reader<'_> {
    fn array<const N: usize>(&mut self) -> [u8; N] {
        let value = self.0[..N].try_into().expect("internal snapshot field");
        self.0 = &self.0[N..];
        value
    }
    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.array())
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.array())
    }
}

fn restore(bbg: &mut Bbg, key: &[u8], value: Option<&[u8]>) {
    let s = &mut bbg.state;
    let entity = || -> Particle { key[1..].try_into().expect("internal entity key") };
    let position = || u64::from_be_bytes(key[1..].try_into().expect("internal position key"));
    let mut r = Reader(value.unwrap_or_default());
    macro_rules! entry {
        ($map:expr, $key:expr, $value:expr) => {
            if value.is_some() {
                $map.insert($key, $value);
            } else {
                $map.remove(&$key);
            }
        };
    }
    match key[0] {
        0 => (), // Height/checkpoint restored from the saved scalar fields.
        1 => entry!(
            s.particles,
            entity(),
            ParticleRecord {
                energy: r.u64(),
                pi_star: r.u64(),
                weight: r.u64(),
                s_yes: r.u64(),
                s_no: r.u64(),
                meta_score: r.u64(),
            }
        ),
        2 | 3 => {
            let map = if key[0] == 2 {
                &mut s.axons_out
            } else {
                &mut s.axons_in
            };
            entry!(map, entity(), (0..r.u64()).map(|_| r.array()).collect());
        }
        4 => entry!(
            s.neurons,
            entity(),
            NeuronRecord {
                focus: r.u64(),
                karma: r.u64(),
                stake: r.u64()
            }
        ),
        9 => entry!(s.time, position(), r.array()),
        10 => entry!(
            s.signals,
            position(),
            SignalRecord {
                neuron: r.array(),
                network: r.array(),
                link_count: r.u32(),
                block_height: r.u64(),
                proof_hash: r.array(),
            }
        ),
        11 => entry!(s.commitments, entity(), Goldilocks::new(r.u64())),
        12 => {
            if value.is_some() {
                s.nullifiers.insert(entity());
            } else {
                s.nullifiers.remove(&entity());
            }
        }
        13 => entry!(s.balances, entity(), r.u64()),
        14 => entry!(
            s.intents,
            entity(),
            IntentRecord {
                neuron: r.array(),
                h0: r.u64(),
                scope_hash: r.array(),
                signature: r.array(),
            }
        ),
        15 => entry!(s.axon_edges, entity(), (r.array(), r.array())),
        16 => entry!(bbg.prune_state.last_touched, entity(), r.u64()),
        _ => unreachable!("internal transition domain"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_limit_counts_unique_keys_and_coalesces_repeated_addresses() {
        let bbg = Bbg::new();
        let mut undo = Undo::new(&bbg);
        for n in 0..MAX_KEYS {
            let mut entity = [0; 32];
            entity[..8].copy_from_slice(&(n as u64).to_le_bytes());
            let key = records::key(13, &entity);
            undo.capture(&bbg, key.clone()).unwrap();
            undo.capture(&bbg, key).unwrap();
        }
        assert_eq!(undo.values.len(), MAX_KEYS);
        assert_eq!(
            undo.capture(&bbg, records::key(13, &[255; 32])),
            Err(Error::Limit("undo keys"))
        );
    }

    #[test]
    fn oversized_adjacency_is_rejected_before_encoding_a_snapshot() {
        let mut bbg = Bbg::new();
        bbg.state
            .axons_out
            .insert([1; 32], vec![[2; 32]; MAX_BYTES / 32]);
        let mut undo = Undo::new(&bbg);
        assert_eq!(
            undo.capture(&bbg, records::key(2, &[1; 32])),
            Err(Error::Limit("adjacency bytes"))
        );
        assert!(undo.values.is_empty());
    }
}
