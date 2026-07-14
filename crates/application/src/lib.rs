//! Layer 2 — Use cases and port traits (the seams). Depends only on `domain`.
//!
//! Ports (`ports`) are the interface contracts adapters implement. Use cases (`use_cases`)
//! orchestrate domain entities through those ports. In-memory fakes for the ports live in
//! `testkit`, beside the traits, for use-case isolation tests (constitution Principle VI).

pub mod audit_recorder;
pub mod ports;
pub mod testkit;
pub mod use_cases;
