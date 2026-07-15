//! Use cases — orchestrate domain entities through ports. Each is testable in isolation against
//! the in-memory fakes in [`crate::testkit`] (constitution Principle VI).

pub mod classify_library;
pub mod classify_track;
pub mod deduplicate_library;
pub mod route_to_triage;
pub mod scan_likes;
