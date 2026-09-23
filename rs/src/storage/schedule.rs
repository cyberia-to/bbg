// ---
// tags: bbg, rust
// crystal-type: source
// crystal-domain: cyber
// ---
//! When the archival task runs a checkpoint sweep.
//!
//! `TieredStore::demote`/`archive`/`evict_archived` (specs/storage.md
//! §archival population) are the mechanism; key eligibility by focus
//! threshold is soma's policy, outside bbg. What is still unassigned is
//! the trigger: how often the sweep (demote the eligible keys, archive
//! the batch, evict_archived once sealed) actually runs. `ArchivalSchedule`
//! answers only that question, as a pure counter — it holds no reference
//! to a `TieredStore` and calls none of its methods, so it composes with
//! `demote`/`archive`/`evict_archived` however the caller wires them
//! without needing their implementation to exist yet.
//!
//! Resuming a schedule after a restart from a durable progress marker is
//! out of scope here — that needs a readable last-checkpoint marker on
//! COLD, open on bbg#9's `ShardStore`/`StorageResult` contract.

/// A sweep interval measured in blocks (per-block `commit()` calls),
/// not wall-clock time — the tiered store already counts blocks.
pub struct ArchivalSchedule {
    interval: u64,
    since_last_checkpoint: u64,
}

impl ArchivalSchedule {
    /// `interval` is clamped to at least 1: a zero interval would make
    /// every block due, which is never a checkpoint cadence anyone wants.
    pub fn new(interval: u64) -> Self {
        Self { interval: interval.max(1), since_last_checkpoint: 0 }
    }

    /// Call once per block commit, before checking `due()`.
    pub fn record_block(&mut self) {
        self.since_last_checkpoint += 1;
    }

    /// True once `interval` blocks have passed since the last checkpoint.
    pub fn due(&self) -> bool {
        self.since_last_checkpoint >= self.interval
    }

    /// Call after a sweep's `archive()` returns `Some` (the batch sealed).
    /// Resets the counter regardless of `due()` — an eager, out-of-band
    /// checkpoint is legal and just restarts the interval from here.
    pub fn checkpoint_ran(&mut self) {
        self.since_last_checkpoint = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_due_before_the_interval_elapses() {
        let mut s = ArchivalSchedule::new(3);
        s.record_block();
        s.record_block();
        assert!(!s.due());
    }

    #[test]
    fn due_once_the_interval_elapses() {
        let mut s = ArchivalSchedule::new(3);
        s.record_block();
        s.record_block();
        s.record_block();
        assert!(s.due());
    }

    #[test]
    fn stays_due_past_the_interval_until_checkpointed() {
        let mut s = ArchivalSchedule::new(2);
        for _ in 0..5 {
            s.record_block();
        }
        assert!(s.due());
    }

    #[test]
    fn checkpoint_ran_resets_the_counter() {
        let mut s = ArchivalSchedule::new(2);
        s.record_block();
        s.record_block();
        assert!(s.due());

        s.checkpoint_ran();

        assert!(!s.due());
        s.record_block();
        assert!(!s.due());
        s.record_block();
        assert!(s.due());
    }

    #[test]
    fn a_zero_interval_is_clamped_to_one_not_always_due() {
        let s = ArchivalSchedule::new(0);
        assert!(!s.due());
    }

    #[test]
    fn a_fresh_schedule_is_not_due() {
        let s = ArchivalSchedule::new(10);
        assert!(!s.due());
    }
}
