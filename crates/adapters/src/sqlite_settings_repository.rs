//! `SqliteSettingsRepository` — SQLite-backed single-row `SettingsRepository` (T024).

use application::ports::repo_error::RepoError;
use application::ports::settings_repository::SettingsRepository;
use async_trait::async_trait;
use domain::confidence::ConfidenceThreshold;
use domain::settings::{ExportMode, Settings};
use rusqlite::{params, OptionalExtension};

use crate::sqlite_support::{to_repo_error, SharedConnection};

/// The fixed primary key of the single settings row.
const SETTINGS_ROW_ID: i64 = 1;

/// SQLite-backed settings store (one row).
pub struct SqliteSettingsRepository {
    connection: SharedConnection,
}

impl SqliteSettingsRepository {
    /// Builds the repository over a shared connection.
    #[must_use]
    pub fn new(connection: SharedConnection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl SettingsRepository for SqliteSettingsRepository {
    async fn load(&self) -> Result<Settings, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let row = conn
            .query_row(
                "SELECT confidence_threshold, download_enabled, export_mode FROM settings WHERE id = ?1",
                [SETTINGS_ROW_ID],
                |r| {
                    let threshold: f64 = r.get(0)?;
                    let download_enabled: i64 = r.get(1)?;
                    let export_mode: String = r.get(2)?;
                    Ok((threshold, download_enabled, export_mode))
                },
            )
            .optional()
            .map_err(to_repo_error)?;

        match row {
            None => Ok(Settings::default()),
            Some((threshold, download_enabled, export_mode)) => {
                let threshold = ConfidenceThreshold::new(threshold as f32).map_err(|e| {
                    RepoError::Serialization {
                        source: Box::new(e),
                    }
                })?;
                let export_mode = ExportMode::from_token(&export_mode).ok_or_else(|| {
                    RepoError::Serialization {
                        source: "unknown export_mode token".into(),
                    }
                })?;
                Ok(Settings::new(threshold, download_enabled != 0, export_mode))
            }
        }
    }

    async fn save(&self, settings: &Settings) -> Result<(), RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO settings (id, confidence_threshold, download_enabled, export_mode) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(id) DO UPDATE SET \
                 confidence_threshold = excluded.confidence_threshold, \
                 download_enabled = excluded.download_enabled, \
                 export_mode = excluded.export_mode",
            params![
                SETTINGS_ROW_ID,
                f64::from(settings.confidence_threshold().value()),
                i64::from(settings.download_enabled()),
                settings.export_mode().as_str(),
            ],
        )
        .map_err(to_repo_error)?;
        Ok(())
    }
}
