//! Every public root dimension shares its serializer with query proofs.
use super::BbgState;
use crate::{dim::commit_dim, proof::dim_entries, query::Dim, types::Particle};
use lens::Commitment;
use nebu::Goldilocks;
impl BbgState {
    pub(super) fn commit_particles(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Particles))
    }
    pub(super) fn commit_axons_out(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::AxonsOut))
    }
    pub(super) fn commit_axons_in(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::AxonsIn))
    }
    pub(super) fn commit_neurons(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Neurons))
    }
    pub(super) fn commit_locations(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Locations))
    }
    pub(super) fn commit_coins(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Coins))
    }
    pub(super) fn commit_cards(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Cards))
    }
    pub(super) fn commit_files(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Files))
    }
    pub(super) fn commit_time(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Time))
    }
    pub(super) fn commit_signals(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Signals))
    }
    pub(super) fn commit_balances(&self) -> Commitment {
        commit_dim(&dim_entries(self, Dim::Balances))
    }
    pub(super) fn commit_a(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .commitments
            .iter()
            .map(|(k, v)| (*k, vec![*v]))
            .collect();
        commit_dim(&entries)
    }
    pub(super) fn commit_n(&self) -> Commitment {
        let entries: Vec<(Particle, Vec<Goldilocks>)> = self
            .nullifiers
            .iter()
            .map(|k| (*k, vec![Goldilocks::ONE]))
            .collect();
        commit_dim(&entries)
    }
}
