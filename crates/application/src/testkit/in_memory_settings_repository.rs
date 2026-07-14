//! In-memory `SettingsRepository` fake.

use std::sync::Mutex;

use async_trait::async_trait;

use domain::settings::Settings;

use crate::ports::repo_error::RepoError;
use crate::ports::settings_repository::SettingsRepository;

/// A `SettingsRepository` holding one `Settings` value in memory (defaults until saved).
#[derive(Debug, Default)]
pub struct InMemorySettingsRepository {
    settings: Mutex<Option<Settings>>,
}

impl InMemorySettingsRepository {
    /// Builds a repository seeded with the default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SettingsRepository for InMemorySettingsRepository {
    async fn load(&self) -> Result<Settings, RepoError> {
        let guard = self.settings.lock().expect("settings mutex poisoned");
        Ok(guard.unwrap_or_default())
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepoError> {
        *self.settings.lock().expect("settings mutex poisoned") = Some(*settings);
        Ok(())
    }
}
