//! Structured, leveled logging (T021, research R7).
//!
//! Secret-redaction is enforced by discipline, not by the subscriber: the API key never reaches a
//! `tracing` field (see [`crate::config::AppConfig`]'s redacting `Debug`, and the adapters which
//! keep the key in HTTP headers only). This module wires the subscriber; keeping secrets out of
//! spans/events is the caller's contract.

use tracing_subscriber::{fmt, EnvFilter};

/// Default log filter when `RUST_LOG` is unset.
const DEFAULT_FILTER: &str = "info";

/// Initializes the global tracing subscriber. Idempotent-safe: a second call is ignored.
pub fn init() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let _ = fmt().with_env_filter(filter).with_target(true).try_init();
}
