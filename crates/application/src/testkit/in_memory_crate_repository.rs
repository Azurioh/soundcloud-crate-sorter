//! In-memory `CrateRepository` fake — dynamic find-or-create keyed on `(genre, energy_role)`.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use domain::crate_::{Crate, CrateId, CrateOrigin, EnergyRole};

use crate::ports::crate_repository::CrateRepository;
use crate::ports::id_provider::IdProvider;
use crate::ports::repo_error::RepoError;

/// A `CrateRepository` backed by an in-memory vector. Mints ids for new crates via the injected
/// `IdProvider` (the one canonical id source), exactly as the SQLite adapter does.
pub struct InMemoryCrateRepository {
    crates: Mutex<Vec<Crate>>,
    ids: Arc<dyn IdProvider>,
}

impl InMemoryCrateRepository {
    /// Builds an empty repository that mints crate ids from `ids`.
    #[must_use]
    pub fn new(ids: Arc<dyn IdProvider>) -> Self {
        Self {
            crates: Mutex::new(Vec::new()),
            ids,
        }
    }
}

#[async_trait]
impl CrateRepository for InMemoryCrateRepository {
    async fn find_or_create(
        &self,
        genre: &str,
        role: Option<EnergyRole>,
    ) -> Result<Crate, RepoError> {
        let mut crates = self.crates.lock().expect("crate repo mutex poisoned");
        if let Some(existing) = crates
            .iter()
            .find(|c| c.genre() == genre && c.energy_role() == role)
        {
            return Ok(existing.clone());
        }
        let created = Crate::new(
            CrateId::from_uuid(self.ids.new_id()),
            genre.to_owned(),
            role,
            CrateOrigin::Auto,
        );
        crates.push(created.clone());
        Ok(created)
    }

    async fn find_by_id(&self, id: &CrateId) -> Result<Option<Crate>, RepoError> {
        let crates = self.crates.lock().expect("crate repo mutex poisoned");
        Ok(crates.iter().find(|c| c.id() == id).cloned())
    }

    async fn list(&self) -> Result<Vec<Crate>, RepoError> {
        let crates = self.crates.lock().expect("crate repo mutex poisoned");
        Ok(crates.clone())
    }
}
