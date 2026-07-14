//! `SettingsRepository` — persistence for the single-row app `Settings`.
//!
//! Not listed in the original port contracts, but the data model defines a `Settings` entity that
//! must survive restarts (threshold, download flag, export mode). Added as a repository port so it
//! slots into the same layer as the other repositories.

use async_trait::async_trait;

use domain::settings::Settings;

use crate::ports::repo_error::RepoError;

/// Loads and stores the app settings.
#[async_trait]
pub trait SettingsRepository: Send + Sync {
    /// Returns the persisted settings, or the defaults if none has been saved yet.
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn load(&self) -> Result<Settings, RepoError>;

    /// Persists `settings` (overwriting the single settings row).
    ///
    /// # Errors
    /// [`RepoError`] on I/O, constraint, or serialization failure.
    async fn save(&self, settings: &Settings) -> Result<(), RepoError>;
}
