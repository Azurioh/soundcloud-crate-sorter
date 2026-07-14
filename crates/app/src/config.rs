//! Application configuration (T006). The Anthropic API key is loaded from the environment and is
//! never logged, committed, or printed — the custom `Debug` impl redacts it (error-design: secrets
//! never on the wire or in logs).

use std::fmt;
use std::path::PathBuf;

/// Environment variable holding the Claude API key.
const API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
/// Environment variable overriding the SQLite database path.
const DB_PATH_ENV: &str = "SCS_DB_PATH";
/// Default database filename under the current working directory.
const DEFAULT_DB_FILE: &str = "crate-sorter.sqlite";

/// Runtime configuration for the app.
pub struct AppConfig {
    /// Claude API key for the genre/vibe classifier (absent → AI classification degrades to
    /// low-confidence fallback, routing ambiguous tracks to triage).
    api_key: Option<String>,
    /// Path to the local SQLite database file.
    database_path: PathBuf,
}

impl AppConfig {
    /// Loads configuration from the process environment.
    #[must_use]
    pub fn from_env() -> Self {
        let api_key = std::env::var(API_KEY_ENV)
            .ok()
            .filter(|k| !k.trim().is_empty());
        let database_path = std::env::var(DB_PATH_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_DB_FILE));
        Self {
            api_key,
            database_path,
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
    pub fn database_path(&self) -> &PathBuf {
        &self.database_path
    }
}

impl fmt::Debug for AppConfig {
    /// Redacts the API key so it can never leak through a `{:?}` / tracing field.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("database_path", &self.database_path)
            .finish()
    }
}
