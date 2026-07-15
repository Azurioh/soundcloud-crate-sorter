//! Use cases — orchestrate domain entities through ports. Each is testable in isolation against
//! the in-memory fakes in [`crate::testkit`] (constitution Principle VI).

pub mod adjust_threshold;
pub mod apply_manual_decision;
pub mod classify_library;
pub mod classify_track;
pub mod deduplicate_library;
pub mod resume_deferred;
pub mod route_to_triage;
pub mod scan_likes;
