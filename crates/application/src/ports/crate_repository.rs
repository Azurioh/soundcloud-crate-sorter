//! `CrateRepository` — persistence for `Crate`, with dynamic find-or-create.

use async_trait::async_trait;

use domain::crate_::{Crate, CrateId, EnergyRole};

use crate::ports::repo_error::RepoError;

/// Stores and queries crates. Crates are created dynamically from the genres/energies present in the
/// library (FR-009); `(genre, energy_role)` is unique.
#[async_trait]
pub trait CrateRepository: Send + Sync {
    /// Returns the crate for `(genre, role)`, creating it if absent. Idempotent: the same pair
    /// always resolves to the same crate.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn find_or_create(
        &self,
        genre: &str,
        role: Option<EnergyRole>,
    ) -> Result<Crate, RepoError>;

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
