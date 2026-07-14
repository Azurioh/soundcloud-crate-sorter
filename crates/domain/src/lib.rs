//! Layer 1 — Entities. Vendor-free domain types (constitution Principle VI).
//!
//! This crate depends on nothing app-specific: only `uuid` (a value type) and `thiserror`
//! (error derives). It never imports a DB, HTTP, UI, or vendor SDK — the Dependency Rule is a
//! compile-time guarantee.

pub mod audit;
pub mod camelot_key;
pub mod classification;
pub mod confidence;
pub mod crate_;
pub mod settings;
pub mod timestamp;
pub mod track;
pub mod validation;
