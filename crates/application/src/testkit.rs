//! In-memory port fakes for use-case isolation tests and contract tests (Principle VI).
//!
//! These live beside the traits so both the use-case `#[cfg(test)]` modules and the adapter
//! contract tests can share one deterministic set of doubles. They never touch network/FS/DB.

pub mod fixed_clock;
pub mod in_memory_audit_log;
pub mod in_memory_crate_repository;
pub mod in_memory_decision_repository;
pub mod in_memory_settings_repository;
pub mod in_memory_track_repository;
pub mod seq_id_provider;
pub mod stub_genre_vibe_classifier;
pub mod stub_likes_source;
