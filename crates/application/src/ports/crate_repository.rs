//! `CrateRepository` — persistence for `Crate`, with dynamic find-or-create.

use async_trait::async_trait;

use domain::crate_::{Crate, CrateId, CrateOrigin, EnergyRole};

use crate::ports::repo_error::RepoError;

/// What identifies a crate to resolve, and how to stamp it if it has to be created (grouped per the
/// 2+-params convention).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateSpec {
    /// The primary genre axis.
    pub genre: String,
    /// The optional energy sub-role.
    pub role: Option<EnergyRole>,
    /// Origin stamped on the crate **only when this call creates it** — an existing crate keeps the
    /// origin it was born with, so a human picking a pre-existing auto crate in triage does not
    /// rewrite its history.
    pub origin: CrateOrigin,
}

/// Stores and queries crates. Crates are created dynamically from the genres/energies present in the
/// library (FR-009); `(genre, energy_role)` is unique.
#[async_trait]
pub trait CrateRepository: Send + Sync {
    /// Returns the crate for `(genre, role)`, creating it if absent. Idempotent: the same pair
    /// always resolves to the same crate, and `spec.origin` applies only to a fresh creation.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn find_or_create(&self, spec: &CrateSpec) -> Result<Crate, RepoError>;

    /// Looks up a crate by id.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn find_by_id(&self, id: &CrateId) -> Result<Option<Crate>, RepoError>;

    /// Returns every crate.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn list(&self) -> Result<Vec<Crate>, RepoError>;
}
