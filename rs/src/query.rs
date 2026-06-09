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
//!   namespace = Dim index (0–9; the look pattern rejects > 9 before calling).
//!   key = flat cell index into the dimension polynomial → returns `evals[key]`.
//! This is the index-addressed convention nox's `BrakedownLookProvider` and
//! zheng's `look_openings_from_provider` already use. Entity → (index, record)
//! navigation is composed ABOVE this in inf (locate by reading key cells + `eq`);
//! `bbg_query` / `prove_*` are entity-keyed conveniences for light clients.

use std::sync::Mutex;

use nebu::Goldilocks;
use nox::{CallProvider, LookProvider, Reduction, Order};
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
/// `key` is the flat cell index into the dimension polynomial; `look` returns
/// `evals[key]`. Composing entity/record reads on top is inf's job.
pub struct BbgLookProvider<'a> {
    pub state: &'a BbgState,
}

impl<'a> LookProvider for BbgLookProvider<'a> {
    /// `key` is the flat cell index into dimension `namespace` — the convention
    /// nox's look pattern and zheng's openings assume. Returns `evals[key]`.
    /// Serves the public dimensions only (ns 0..9); private dims (balances) live
    /// in the mutator set and are never exposed through the public look.
    fn look(&self, _commitment: Goldilocks, namespace: Goldilocks, key: Goldilocks) -> Option<Goldilocks> {
        if namespace.as_u64() > 9 {
            return None;
        }
        let dim = Dim::from_u64(namespace.as_u64())?;
        crate::proof::cell_value(self.state, dim, key.as_u64() as usize)
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
    /// `key` is the flat cell index; opens `evals[key]` at its corner and records
    /// the `LookOpening`. `None` (no opening) if `key` is out of range. Public
    /// dimensions only (ns 0..9).
    fn look(&self, _commitment: Goldilocks, namespace: Goldilocks, key: Goldilocks) -> Option<Goldilocks> {
        if namespace.as_u64() > 9 {
            return None;
        }
        let dim = Dim::from_u64(namespace.as_u64())?;
        let proof = crate::proof::open_cell(self.state, dim, key.as_u64() as usize)?;
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
    fn provide(&self, _reduction: &mut Reduction<N>, _tag: Goldilocks, _object: Order) -> Option<Order> {
        None
    }
}

// ── collect_look_openings ──────────────────────────────────────────────────────

/// Generate `LookOpening`s for every look row in `trace` without re-executing.
///
/// Scans for rows where `r[0] == 17` (look tag), reads `r[5]` as namespace and
/// `r[6]` as the flat cell index, and opens that cell. Rows whose index is out
/// of range or whose dimension is ≥ 10 are skipped.
///
/// Post-execution alternative to `ProofLookProvider`: execute with any provider,
/// then derive all openings for `zheng::commit()` from the trace.
pub fn collect_look_openings(state: &BbgState, trace: &[nox::TraceRow]) -> Vec<LookOpening> {
    let mut result = Vec::new();
    for row in trace {
        if row.r()[0] != 17 {
            continue;
        }
        let ns  = Goldilocks::new(row.r()[5]);
        let key = Goldilocks::new(row.r()[6]);
        if ns.as_u64() > 9 {
            continue; // public dimensions only
        }
        let Some(dim) = Dim::from_u64(ns.as_u64()) else { continue };
        let Some(proof) = crate::proof::open_cell(state, dim, key.as_u64() as usize) else { continue };
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
    fn look_provider_reads_cell_by_index() {
        let state = seeded_state(); // time[0] = particle(99); entry = [key(4) | root(4)]
        let prov  = BbgLookProvider { state: &state };
        let ns  = Goldilocks::new(Dim::Time as u64);
        // cell 4 is the first value field = root[0]
        let expect = crate::dim::goldilocks_from_bytes32(&[99u8; 32])[0];
        assert_eq!(prov.look(ct(), ns, Goldilocks::new(4)), Some(expect));
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

    // Cells are addressed by flat index; a neuron's focus is value field 0 = cell 4
    // (after its 4 key cells). Entity → index navigation is composed above, in inf.
    #[test]
    fn look_provider_neuron_focus_cell_by_index() {
        let mut state = BbgState::new();
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(&42u64.to_le_bytes());
        state.neurons.insert(key, crate::types::NeuronRecord { focus: 777, karma: 0, stake: 0 });

        let prov = BbgLookProvider { state: &state };
        let ns   = Goldilocks::new(Dim::Neurons as u64);
        assert_eq!(prov.look(ct(), ns, Goldilocks::new(4)), Some(Goldilocks::new(777)));
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
        // focus is value field 0 = cell 4 (after the 4 key cells)
        let k  = Goldilocks::new(4);

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
        let state = seeded_state(); // time[0] = particle(99); root cell at index 4
        let fast  = BbgLookProvider { state: &state };
        let proof = ProofLookProvider::new(&state);
        let ns  = Goldilocks::new(Dim::Time as u64);
        let key = Goldilocks::new(4);
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
        use nox::{reduce, Reduction, VecTrace, Outcome};

        let state = seeded_state();
        let prov  = ProofLookProvider::new(&state);

        let mut reduction = Reduction::<1024>::new();
        let g = |v: u64| Goldilocks::new(v);

        // Object: [[0|[0|[0|0]]]|0] — 4-limb BBG root, all limbs zero
        let al0  = reduction.atom(g(0)).unwrap();
        let al1  = reduction.atom(g(0)).unwrap();
        let al2  = reduction.atom(g(0)).unwrap();
        let al3  = reduction.atom(g(0)).unwrap();
        let inner2    = reduction.pair(al2, al3).unwrap();
        let mid       = reduction.pair(al1, inner2).unwrap();
        let root_cell = reduction.pair(al0, mid).unwrap();
        let rest      = reduction.atom(g(0)).unwrap();
        let obj       = reduction.pair(root_cell, rest).unwrap();

        // Formula: [17 [[1 8] [1 0]]]
        let t17   = reduction.atom(g(17)).unwrap();
        let t1    = reduction.atom(g(1)).unwrap();
        let vns   = reduction.atom(g(8)).unwrap(); // Dim::Time
        let vkey  = reduction.atom(g(0)).unwrap(); // cell index 0
        let ns_f  = reduction.pair(t1, vns).unwrap();
        let key_f = reduction.pair(t1, vkey).unwrap();
        let body  = reduction.pair(ns_f, key_f).unwrap();
        let f     = reduction.pair(t17, body).unwrap();

        let mut trace = VecTrace::default();
        let outcome = reduce(&mut reduction, obj, f, 1000, &prov, &mut trace);
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
