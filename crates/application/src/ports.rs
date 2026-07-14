//! Port traits — the application's interface contracts (constitution Principle VI).
//!
//! Every trait here is a seam an adapter implements and a use case depends on. Inputs/outputs are
//! **domain types only**; no vendor/SDK/DB type crosses a trait. Adapters wrap their vendor errors
//! into the port's own typed error enum, carrying the original as `source` (never leaking secrets).

pub mod audio_analyzer;
pub mod audio_downloader;
pub mod audit_log;
pub mod clock;
pub mod crate_repository;
pub mod genre_vibe_classifier;
pub mod id_provider;
pub mod likes_source;
pub mod playlist_publisher;
pub mod repo_error;
pub mod settings_repository;
pub mod tag_writer;
pub mod track_repository;

/// Type-erased wrapper for a wrapped lower-level error. Lets a vendor-free port error carry the
/// original cause (error-design: "keep tracking the original error") without the application layer
/// ever naming the concrete vendor error type.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
