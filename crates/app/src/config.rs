//! Application configuration (T006). The Anthropic API key is loaded from the environment and is
//! never logged, committed, or printed — the custom `Debug` impl redacts it (error-design: secrets
//! never on the wire or in logs).

use std::fmt;
use std::path::{Path, PathBuf};

/// Environment variable holding the Claude API key.
pub(crate) const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
/// Environment variable overriding the SQLite database path.
const DB_PATH_ENV: &str = "SCS_DB_PATH";
/// Database filename, placed inside the OS application-data directory.
const DB_FILE: &str = "crate-sorter.sqlite";
/// Environment variable overriding where opt-in downloaded audio is stored.
const DOWNLOAD_DIR_ENV: &str = "SCS_DOWNLOAD_DIR";
/// Directory name for downloaded audio, inside the OS application-data directory.
const DOWNLOAD_DIR: &str = "audio";

/// Runtime configuration for the app.
pub struct AppConfig {
    /// Claude API key for the genre/vibe classifier (absent → AI classification degrades to
    /// low-confidence fallback, routing ambiguous tracks to triage).
    api_key: Option<String>,
    /// Path to the local SQLite database file.
    database_path: PathBuf,
    /// Directory holding opt-in downloaded audio (User Story 4).
    download_dir: PathBuf,
}

impl AppConfig {
    /// Loads configuration from the process environment, storing the database inside `data_dir`
    /// unless `SCS_DB_PATH` overrides it.
    ///
    /// `data_dir` is resolved by the caller from the OS application-data location rather than the
    /// working directory: a bundled app inherits its launcher's CWD (`/` when opened from Finder),
    /// where the database file cannot be created.
    ///
    /// # Parameters
    /// - `data_dir`: directory that holds the app's persistent state; must already exist.
    #[must_use]
    pub fn from_env(data_dir: &Path) -> Self {
        let api_key = std::env::var(API_KEY_ENV)
            .ok()
            .filter(|k| !k.trim().is_empty());
        let database_path = std::env::var(DB_PATH_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join(DB_FILE));
        let download_dir = std::env::var(DOWNLOAD_DIR_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join(DOWNLOAD_DIR));
        Self {
            api_key,
            database_path,
            download_dir,
        }
    }

    /// The API key, if configured.
    #[must_use]
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Whether an API key is present (safe to log — reveals presence, not the value).
    #[must_use]
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// The SQLite database path.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Where opt-in downloaded audio is stored. Nothing is written here unless the user turns
    /// downloading on (Principle V) — the directory is created by the downloader on first use.
    #[must_use]
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }
}

impl fmt::Debug for AppConfig {
    /// Redacts the API key so it can never leak through a `{:?}` / tracing field.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("database_path", &self.database_path)
            .field("download_dir", &self.download_dir)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key value that would be unmistakable if it ever leaked into rendered output.
    const SECRET: &str = "sk-ant-must-never-appear";

    fn config_with(api_key: Option<&str>) -> AppConfig {
        AppConfig {
            api_key: api_key.map(str::to_owned),
            database_path: PathBuf::from("/tmp/crate-sorter.sqlite"),
            download_dir: PathBuf::from("/tmp/audio"),
        }
    }

    #[test]
    fn debug_redacts_the_api_key() {
        let rendered = format!("{:?}", config_with(Some(SECRET)));
        assert!(
            !rendered.contains(SECRET),
            "the API key leaked into Debug output: {rendered}"
        );
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn debug_keeps_the_non_secret_fields_readable() {
        let rendered = format!("{:?}", config_with(None));
        assert!(rendered.contains("crate-sorter.sqlite"));
        assert!(rendered.contains("None"));
    }

    #[test]
    fn has_api_key_reflects_presence_without_exposing_the_value() {
        assert!(config_with(Some(SECRET)).has_api_key());
        assert!(!config_with(None).has_api_key());
    }
}
