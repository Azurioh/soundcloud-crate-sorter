//! Layer 3 — Interface adapters. Each adapter implements an application port and maps vendor/DB
//! types to domain types at the edge (constitution Principle VI). No adapter leaks a vendor type
//! across the port boundary.

pub mod anthropic_genre_vibe_classifier;
pub mod internal_api_likes_source;
pub mod sqlite_audit_log;
pub mod sqlite_crate_repository;
pub mod sqlite_schema;
pub mod sqlite_settings_repository;
pub mod sqlite_support;
pub mod sqlite_track_repository;
pub mod system_clock_provider;
pub mod uuid_id_provider;
