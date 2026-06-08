// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! Uniform BBG state query interface.
//!
//! Four components:
//!   1. `Dim` — 11-variant enum mapping namespace indices to BBG dimensions
//!   2. `bbg_query` — dispatch to the appropriate `prove_*` function
//!   3. `BbgLookProvider` — implements `nox::LookProvider` for VM look pattern (fast, no proofs)
//!   4. `ProofLookProvider` — like BbgLookProvider but also accumulates `LookOpening`s
//!      for passing to `zheng::commit()` after execution
//!
//! Key encoding for look providers:
//!   namespace = Dim index (0–9, the look pattern rejects > 9 before calling).
//!   key = first 8 bytes of particle ID as u64 LE.
//!   For Time/Signals the key is the full u64 height/step.
//!   For Balances (index 10): unreachable via look pattern (ns > 9 rejected by nox).

use std::sync::Mutex;

use nebu::Goldilocks;
use nox::{CallProvider, LookProvider, NounId, Order};
use zheng::LookOpening;

use crate::dim::goldilocks_from_bytes32;
use crate::proof::{
    prove_axons_in, prove_axons_out, prove_card, prove_coin, prove_file, prove_location,
    prove_neuron, prove_particle, prove_signal, prove_time, QueryProof,
};
use crate::state::BbgState;

/// BBG dimension index (namespace passed to `LookProvider::look`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum Dim {
    Particles = 0,
    AxonsOut  = 1,
    AxonsIn   = 2,
    Neurons   = 3,
    Locations = 4,
    Coins     = 5,
    Cards     = 6,
    Files     = 7,
    Time      = 8,
    Signals   = 9,
    Balances  = 10,
}

impl Dim {
    pub fn from_u64(v: u64) -> Option<Self> {
        match v {
            0  => Some(Self::Particles),
            1  => Some(Self::AxonsOut),
            2  => Some(Self::AxonsIn),
            3  => Some(Self::Neurons),
            4  => Some(Self::Locations),
            5  => Some(Self::Coins),
            6  => Some(Self::Cards),
            7  => Some(Self::Files),
            8  => Some(Self::Time),
            9  => Some(Self::Signals),
            10 => Some(Self::Balances),
            _  => None,
        }
    }
}

/// Generate a `QueryProof` for dimension `dim` at key `key`.
///
/// `key` is a 32-byte particle/hash. For `Time` and `Signals` the u64 height
/// or step is read from the first 8 bytes (little-endian).
///
/// `Balances` returns `None` — use `prove_balances(state, owner, token)` directly
/// because the key is derived from two inputs.
pub fn bbg_query(state: &BbgState, dim: Dim, key: &[u8; 32]) -> Option<QueryProof> {
    match dim {
        Dim::Particles => prove_particle(state, key),
        Dim::AxonsOut  => prove_axons_out(state, key),
        Dim::AxonsIn   => prove_axons_in(state, key),
        Dim::Neurons   => prove_neuron(state, key),
        Dim::Locations => prove_location(state, key),
        Dim::Coins     => prove_coin(state, key),
        Dim::Cards     => prove_card(state, key),
        Dim::Files     => prove_file(state, key),
        Dim::Time      => {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&key[..8]);
            prove_time(state, u64::from_le_bytes(buf))
        }
        Dim::Signals   => {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&key[..8]);
            prove_signal(state, u64::from_le_bytes(buf))
        }
        Dim::Balances  => None,
    }
}

/// Verify a `QueryProof` produced by `bbg_query`.
///
/// Returns `true` iff the Brakedown opening is valid. Does not re-check
/// which dimension the proof came from — the caller binds that context.
pub fn verify_query(proof: &QueryProof) -> bool {
    use lens::{brakedown::Brakedown, Lens, Transcript as LensTx};
    let value = query_value(proof);
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&proof.commitment, &proof.point, value, &proof.opening, &mut tx)
}

// ── BbgLookProvider ──────────────────────────────────────────────────────────

/// nox `LookProvider` backed by live `BbgState` — fast, no proofs generated.
///
/// Converts the single-Goldilocks `key` to a 32-byte particle key by writing
/// `key.as_u64()` into the first 8 bytes and zero-filling the rest. This
/// works exactly for `Time` and `Signals` (where the key IS a u64). For
/// particle-keyed dimensions (Particles, Neurons, …) it matches entries whose
/// first 8 bytes equal the key — programmes must use the particle's first limb
/// as the lookup key or call `bbg_query` directly for full 32-byte resolution.
pub struct BbgLookProvider<'a> {
    pub state: &'a BbgState,
}

impl<'a> LookProvider for BbgLookProvider<'a> {
    fn look(&self, _commitment: Goldilocks, namespace: Goldilocks, key: Goldilocks) -> Option<Goldilocks> {
        let dim = Dim::from_u64(namespace.as_u64())?;
        let mut key_bytes = [0u8; 32];
        key_bytes[..8].copy_from_slice(&key.as_u64().to_le_bytes());
        look_scalar(self.state, dim, &key_bytes)
    }
}

// ── ProofLookProvider ─────────────────────────────────────────────────────────

/// nox `LookProvider` that generates `LookOpening` proofs alongside the value.
///
/// During `look()`, calls `bbg_query()` to produce a full Brakedown opening
/// and appends a `LookOpening` to the internal accumulator. After execution,
/// call `take_look_openings()` to retrieve the ordered list for `zheng::commit()`.
///
/// Uses `Mutex` (not `RefCell`) so the provider is `Sync` — satisfying the
/// `CallProvider<N>` bound when wrapped in a `ProofCallProvider`.
pub struct ProofLookProvider<'a> {
    pub state: &'a BbgState,
    openings: Mutex<Vec<LookOpening>>,
}

impl<'a> ProofLookProvider<'a> {
    pub fn new(state: &'a BbgState) -> Self {
        Self { state, openings: Mutex::new(Vec::new()) }
    }

    /// Drain and return all accumulated `LookOpening`s in insertion order.
    ///
    /// Pass the result directly to `zheng::commit()` as `look_openings`.
    /// The internal buffer is emptied; subsequent calls return an empty vec.
    pub fn take_look_openings(&self) -> Vec<LookOpening> {
        self.openings.lock().unwrap().drain(..).collect()
    }
}

impl<'a> LookProvider for ProofLookProvider<'a> {
    fn look(&self, _commitment: Goldilocks, namespace: Goldilocks, key: Goldilocks) -> Option<Goldilocks> {
        let dim = Dim::from_u64(namespace.as_u64())?;
        let mut key_bytes = [0u8; 32];
        key_bytes[..8].copy_from_slice(&key.as_u64().to_le_bytes());

        // Gate proof generation on BTreeMap presence: the polynomial evaluates at
        // any point (returning Some), but we only want proofs for keys that exist.
        look_scalar(self.state, dim, &key_bytes)?;

        let proof = bbg_query(self.state, dim, &key_bytes)?;
        let value = query_value(&proof);
        let opening = LookOpening {
            commitment:      proof.commitment,
            point:           proof.point,
            value,
            opening:         proof.opening,
            transcript_seed: b"bbg-dim-open".to_vec(),
            bbg_root:        goldilocks_from_bytes32(&self.state.root),
            namespace,
            a_commit:        None,
        };
        self.openings.lock().unwrap().push(opening);
        Some(value)
    }
}

/// `ProofLookProvider` can be used directly with `nox::reduce()`.
///
/// `provide()` always returns `None` — only the look side is implemented.
/// The `Sync` bound is satisfied because `Mutex` is `Sync` when its content
/// is `Send`, and `Vec<LookOpening>` is `Send`.
impl<'a, const N: usize> CallProvider<N> for ProofLookProvider<'a> {
    fn provide(&self, _order: &mut Order<N>, _tag: Goldilocks, _object: NounId) -> Option<NounId> {
        None
    }
}

// ── collect_look_openings ──────────────────────────────────────────────────────

/// Generate `LookOpening`s for every look row in `trace` without re-executing.
///
/// Scans for rows where `r[0] == 17` (look tag), reads `r[5]` as namespace and
/// `r[6]` as key, and calls `bbg_query()` to produce the opening. Rows whose
/// lookup fails (key absent or dimension ≥ 10) are skipped.
///
/// This is the post-execution alternative to `ProofLookProvider`: execute with
/// any provider, then call this function on the resulting trace to derive all
/// necessary openings for `zheng::commit()`.
pub fn collect_look_openings(state: &BbgState, trace: &[nox::TraceRow]) -> Vec<LookOpening> {
    let mut result = Vec::new();
    for row in trace {
        if row.r()[0] != 17 {
            continue;
        }
        let ns  = Goldilocks::new(row.r()[5]);
        let key = Goldilocks::new(row.r()[6]);
        let Some(dim) = Dim::from_u64(ns.as_u64()) else { continue };
        let mut key_bytes = [0u8; 32];
        key_bytes[..8].copy_from_slice(&key.as_u64().to_le_bytes());
        // Gate proof generation on BTreeMap presence (same as ProofLookProvider).
        if look_scalar(state, dim, &key_bytes).is_none() { continue }
        let Some(proof) = bbg_query(state, dim, &key_bytes) else { continue };
        let value = query_value(&proof);
        result.push(LookOpening {
            commitment:      proof.commitment,
            point:           proof.point,
            value,
            opening:         proof.opening,
            transcript_seed: b"bbg-dim-open".to_vec(),
            bbg_root:        goldilocks_from_bytes32(&state.root),
            namespace:       ns,
            a_commit:        None,
        });
    }
    result
}

/// Fast scalar lookup (no proof generated).
///
/// Returns the primary u64 field of the record as a Goldilocks element, or
/// `None` if the key is not present.
fn look_scalar(state: &BbgState, dim: Dim, key: &[u8; 32]) -> Option<Goldilocks> {
    match dim {
        Dim::Particles => state.particles.get(key).map(|p| Goldilocks::new(p.energy)),
        Dim::AxonsOut  => state.axons_out.get(key).map(|v| Goldilocks::new(v.len() as u64)),
        Dim::AxonsIn   => state.axons_in.get(key).map(|v| Goldilocks::new(v.len() as u64)),
        Dim::Neurons   => state.neurons.get(key).map(|n| Goldilocks::new(n.focus)),
        Dim::Locations => state.locations.get(key).map(|l| Goldilocks::new(l.lat as u32 as u64)),
        Dim::Coins     => state.coins.get(key).map(|c| Goldilocks::new(c.total_supply)),
        Dim::Cards     => state.cards.get(key).map(|c| goldilocks_from_bytes32(&c.owner)[0]),
        Dim::Files     => state.files.get(key).map(|f| Goldilocks::new(f.available as u64)),
        Dim::Time      => {
            let h = u64::from_le_bytes(key[..8].try_into().ok()?);
            state.time.get(&h).map(|p| goldilocks_from_bytes32(p)[0])
        }
        Dim::Signals   => {
            let step = u64::from_le_bytes(key[..8].try_into().ok()?);
            state.signals.get(&step).map(|s| Goldilocks::new(s.link_count as u64))
        }
        Dim::Balances  => None,
    }
}

fn query_value(proof: &QueryProof) -> Goldilocks {
    if proof.value_bytes.len() < 8 {
        return Goldilocks::ZERO;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&proof.value_bytes[..8]);
    Goldilocks::new(u64::from_le_bytes(buf))
}

/// Verify a `LookOpening` produced by `ProofLookProvider` or `collect_look_openings`.
pub fn verify_opening(lo: &LookOpening) -> bool {
    use lens::{brakedown::Brakedown, Lens, Transcript as LensTx};
    let mut tx = LensTx::new(b"bbg-dim-open");
    Brakedown::verify(&lo.commitment, &lo.point, lo.value, &lo.opening, &mut tx)
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{Cyberlink, Signal};
    use crate::state::EPOCH_BLOCKS;
    use crate::types::NeuronRecord;

    fn particle(seed: u8) -> [u8; 32] { [seed; 32] }

    fn seeded_state() -> BbgState {
        let mut s = BbgState::new();
        s.neurons.insert(particle(1), NeuronRecord { focus: 50_000, karma: 0, stake: 0 });
        let sig = Signal {
            neuron: particle(1),
            links: vec![Cyberlink {
                from:    particle(2),
                to:      particle(3),
                token:   particle(0),
                amount:  100,
                valence: 1,
            }],
            box_moves: vec![],
            height: 0,
        };
        s.insert(&sig).unwrap();
        // add a time snapshot
        s.time.insert(0, particle(99));
        s
    }

    #[test]
    fn dim_roundtrip() {
        for i in 0u64..=10 {
            assert_eq!(Dim::from_u64(i).unwrap() as u64, i);
        }
        assert!(Dim::from_u64(11).is_none());
    }

    #[test]
    fn bbg_query_particles_returns_proof() {
        let state = seeded_state();
        let axon  = crate::state::axon_id(&particle(2), &particle(3));
        let proof = bbg_query(&state, Dim::Particles, &axon);
        assert!(proof.is_some());
    }

    #[test]
    fn bbg_query_neurons_returns_proof() {
        let state = seeded_state();
        let proof = bbg_query(&state, Dim::Neurons, &particle(1));
        assert!(proof.is_some());
    }

    #[test]
    fn bbg_query_time_uses_first_8_bytes_as_u64() {
        let state = seeded_state();
        let mut key = [0u8; 32];
        // height 0 in LE
        key[..8].copy_from_slice(&0u64.to_le_bytes());
        let proof = bbg_query(&state, Dim::Time, &key);
        assert!(proof.is_some());
    }

    #[test]
    fn bbg_query_balances_returns_none() {
        let state = seeded_state();
        let proof = bbg_query(&state, Dim::Balances, &particle(42));
        assert!(proof.is_none());
    }

    #[test]
    fn verify_query_valid_proof() {
        let state = seeded_state();
        let axon  = crate::state::axon_id(&particle(2), &particle(3));
        let proof = bbg_query(&state, Dim::Particles, &axon).unwrap();
        assert!(verify_query(&proof));
    }

    // ── look_provider: Time and Signals are the primary natural use cases ─────

    fn ct() -> Goldilocks { Goldilocks::ZERO } // commitment arg not checked by BbgLookProvider

    #[test]
    fn look_provider_time_by_height() {
        let state = seeded_state();
        let prov  = BbgLookProvider { state: &state };
        let ns  = Goldilocks::new(Dim::Time as u64);
        let key = Goldilocks::new(0); // height = 0
        assert!(prov.look(ct(), ns, key).is_some());
    }

    #[test]
    fn look_provider_missing_returns_none() {
        let state = seeded_state();
        let prov  = BbgLookProvider { state: &state };
        let ns  = Goldilocks::new(Dim::Time as u64);
        let key = Goldilocks::new(9999); // height not in state
        assert!(prov.look(ct(), ns, key).is_none());
    }

    #[test]
    fn look_provider_invalid_namespace_returns_none() {
        let state = seeded_state();
        let prov = BbgLookProvider { state: &state };
        assert!(prov.look(ct(), Goldilocks::new(99), Goldilocks::ZERO).is_none());
    }

    #[test]
    fn look_provider_balances_returns_none() {
        let state = seeded_state();
        let prov = BbgLookProvider { state: &state };
        assert!(prov.look(ct(), Goldilocks::new(Dim::Balances as u64), Goldilocks::ZERO).is_none());
    }

    // look_provider for particle-keyed dimensions matches on first 8 bytes only.
    // Neurons keyed by u64-compatible IDs (bytes 8..32 = 0) work correctly.
    #[test]
    fn look_provider_neuron_u64_compatible_key() {
        let mut state = BbgState::new();
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&42u64.to_le_bytes());
        state.neurons.insert(key, crate::types::NeuronRecord { focus: 777, karma: 0, stake: 0 });

        let prov = BbgLookProvider { state: &state };
        let ns   = Goldilocks::new(Dim::Neurons as u64);
        let k    = Goldilocks::new(42);
        assert_eq!(prov.look(ct(), ns, k), Some(Goldilocks::new(777)));
    }

    // full 32-byte particle ID (non-zero bytes 8..32) is NOT addressable via single u64 key
    #[test]
    fn look_provider_full_particle_key_not_addressable() {
        let state = seeded_state(); // neurons stored as particle(1) = [1;32]
        let prov  = BbgLookProvider { state: &state };
        let ns    = Goldilocks::new(Dim::Neurons as u64);
        let key   = Goldilocks::new(u64::from_le_bytes([1; 8]));
        assert!(prov.look(ct(), ns, key).is_none()); // [1,1,1,1,1,1,1,1,0...0] ≠ [1;32]
    }

    // ── ProofLookProvider ─────────────────────────────────────────────────────

    #[test]
    fn proof_look_provider_returns_value_and_accumulates_opening() {
        let state = seeded_state();
        let prov  = ProofLookProvider::new(&state);
        let ns    = Goldilocks::new(Dim::Time as u64);
        let key   = Goldilocks::new(0); // height 0 is present

        let val = prov.look(ct(), ns, key);
        assert!(val.is_some(), "should return the time root value");

        let openings = prov.take_look_openings();
        assert_eq!(openings.len(), 1);
        // verify the opening is sound
        assert!(crate::query::verify_opening(&openings[0]));
    }

    #[test]
    fn proof_look_provider_missing_key_returns_none_no_opening() {
        let state = seeded_state();
        let prov  = ProofLookProvider::new(&state);
        let ns    = Goldilocks::new(Dim::Time as u64);
        let key   = Goldilocks::new(9999); // not in state

        let val = prov.look(ct(), ns, key);
        assert!(val.is_none());
        assert!(prov.take_look_openings().is_empty());
    }

    #[test]
    fn proof_look_provider_accumulates_multiple_openings() {
        let state = seeded_state();
        let prov  = ProofLookProvider::new(&state);
        let ns    = Goldilocks::new(Dim::Time as u64);

        prov.look(ct(), ns, Goldilocks::new(0));       // present
        prov.look(ct(), ns, Goldilocks::new(9999));    // absent — no opening
        prov.look(ct(), ns, Goldilocks::new(0));       // present again

        let openings = prov.take_look_openings();
        assert_eq!(openings.len(), 2, "only successful lookups accumulate openings");
    }

    #[test]
    fn take_look_openings_drains_the_buffer() {
        let state = seeded_state();
        let prov  = ProofLookProvider::new(&state);
        let ns    = Goldilocks::new(Dim::Time as u64);
        prov.look(ct(), ns, Goldilocks::new(0));

        let first  = prov.take_look_openings();
        let second = prov.take_look_openings();
        assert_eq!(first.len(), 1);
        assert!(second.is_empty(), "take drains the buffer");
    }

    // ── proof-path value semantics (corner opening, L0 fix) ───────────────────

    // Before the L0 fix, ProofLookProvider returned poly.evaluate(key_point) — an
    // off-corner MLE fingerprint that disagreed with the real column. After the fix
    // it opens the value cell at its hypercube corner and returns the real cell, so
    // the proof path and the fast path must agree.
    #[test]
    fn proof_provider_value_matches_fast_provider_neuron() {
        let mut state = BbgState::new();
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&42u64.to_le_bytes());
        state.neurons.insert(key, NeuronRecord { focus: 777, karma: 0, stake: 0 });

        let fast  = BbgLookProvider { state: &state };
        let proof = ProofLookProvider::new(&state);
        let ns = Goldilocks::new(Dim::Neurons as u64);
        let k  = Goldilocks::new(42);

        // both return the REAL focus (777), not a fingerprint
        assert_eq!(fast.look(ct(), ns, k), Some(Goldilocks::new(777)));
        assert_eq!(
            proof.look(ct(), ns, k),
            Some(Goldilocks::new(777)),
            "proof provider must return the real cell, matching the fast provider"
        );

        // and the opening it accumulated verifies
        let openings = proof.take_look_openings();
        assert_eq!(openings.len(), 1);
        assert_eq!(openings[0].value, Goldilocks::new(777));
        assert!(verify_opening(&openings[0]), "corner opening must verify");
    }

    #[test]
    fn proof_provider_value_matches_fast_provider_time() {
        let state = seeded_state(); // time[0] = particle(99)
        let fast  = BbgLookProvider { state: &state };
        let proof = ProofLookProvider::new(&state);
        let ns  = Goldilocks::new(Dim::Time as u64);
        let key = Goldilocks::new(0);
        let fast_v = fast.look(ct(), ns, key);
        assert!(fast_v.is_some());
        assert_eq!(proof.look(ct(), ns, key), fast_v, "proof and fast providers must agree");
    }

    // ── collect_look_openings ─────────────────────────────────────────────────

    #[test]
    fn collect_look_openings_empty_trace() {
        let state = seeded_state();
        assert!(collect_look_openings(&state, &[]).is_empty());
    }

    #[test]
    fn collect_look_openings_non_look_row_ignored() {
        use nox::TraceRow;
        let state = seeded_state();
        let row   = TraceRow::default(); // r[0] = 0 (axis tag, not 17)
        let ops   = collect_look_openings(&state, &[row]);
        assert!(ops.is_empty());
    }

    /// Execute a real look via nox::reduce(), then verify collect_look_openings
    /// produces the same openings as ProofLookProvider accumulated during execution.
    ///
    /// Formula: [17 [[1 8] [1 0]]] — look dim=Time(8), key=0 (height in state)
    /// Object:  [[l0|[l1|[l2|l3]]]|rest] — 4-limb BBG root layout
    #[test]
    fn collect_look_openings_from_real_trace() {
        use nox::{reduce, Order, Tag, VecTrace, Outcome};

        let state = seeded_state();
        let prov  = ProofLookProvider::new(&state);

        let mut order = Order::<1024>::new();
        let g = |v: u64| Goldilocks::new(v);

        // Object: [[0|[0|[0|0]]]|0] — 4-limb BBG root, all limbs zero
        let al0  = order.atom(g(0), Tag::Field).unwrap();
        let al1  = order.atom(g(0), Tag::Field).unwrap();
        let al2  = order.atom(g(0), Tag::Field).unwrap();
        let al3  = order.atom(g(0), Tag::Field).unwrap();
        let inner2    = order.cell(al2, al3).unwrap();
        let mid       = order.cell(al1, inner2).unwrap();
        let root_cell = order.cell(al0, mid).unwrap();
        let rest      = order.atom(g(0), Tag::Field).unwrap();
        let obj       = order.cell(root_cell, rest).unwrap();

        // Formula: [17 [[1 8] [1 0]]]
        let t17   = order.atom(g(17), Tag::Field).unwrap();
        let t1    = order.atom(g(1),  Tag::Field).unwrap();
        let vns   = order.atom(g(8),  Tag::Field).unwrap(); // Dim::Time
        let vkey  = order.atom(g(0),  Tag::Field).unwrap(); // height 0
        let ns_f  = order.cell(t1, vns).unwrap();
        let key_f = order.cell(t1, vkey).unwrap();
        let body  = order.cell(ns_f, key_f).unwrap();
        let f     = order.cell(t17, body).unwrap();

        let mut trace = VecTrace::default();
        let outcome = reduce(&mut order, obj, f, 1000, &prov, &mut trace);
        assert!(matches!(outcome, Outcome::Ok(_, _)), "look must succeed with ProofLookProvider");

        // ProofLookProvider accumulated 1 opening during execution.
        let prov_openings = prov.take_look_openings();
        assert_eq!(prov_openings.len(), 1);

        // collect_look_openings on the same trace must produce the same count.
        let col_openings = collect_look_openings(&state, &trace.0);
        assert_eq!(col_openings.len(), 1, "collect_look_openings should find 1 look row");

        // Both openings must be valid.
        assert!(verify_opening(&prov_openings[0]));
        assert!(verify_opening(&col_openings[0]));
    }

    // Confirm EPOCH_BLOCKS is used in signal fixture only
    const _: u64 = EPOCH_BLOCKS;
}
