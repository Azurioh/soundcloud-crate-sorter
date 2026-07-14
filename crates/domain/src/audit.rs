//! The `AuditEvent` entity — the durable, queryable "why is this track in this crate?" trail
//! (constitution Principle VII).

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::timestamp::Timestamp;
use crate::track::TrackId;

/// Correlates every event produced by one pipeline run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(Uuid);

impl RunId {
    /// Wraps a raw UUID minted by the application's `IdProvider`.
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Returns the underlying UUID (for persistence at the adapter edge).
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

/// Stable identity of an `AuditEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuditEventId(Uuid);

impl AuditEventId {
    /// Wraps a raw UUID minted by the application's `IdProvider`.
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Returns the underlying UUID (for persistence at the adapter edge).
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

/// The pipeline stage an event belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    /// Scanning likes from the source.
    Scan,
    /// Deduplicating the library.
    Dedup,
    /// Downloading audio (opt-in).
    Download,
    /// Analyzing audio for BPM/key/energy.
    Analyze,
    /// Classifying a track into a crate.
    Classify,
    /// A manual triage action.
    Triage,
    /// Exporting crates to disk.
    Export,
}

impl PipelineStage {
    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Dedup => "dedup",
            Self::Download => "download",
            Self::Analyze => "analyze",
            Self::Classify => "classify",
            Self::Triage => "triage",
            Self::Export => "export",
        }
    }

    /// Parses a persistence token back into a stage.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "scan" => Some(Self::Scan),
            "dedup" => Some(Self::Dedup),
            "download" => Some(Self::Download),
            "analyze" => Some(Self::Analyze),
            "classify" => Some(Self::Classify),
            "triage" => Some(Self::Triage),
            "export" => Some(Self::Export),
            _ => None,
        }
    }
}

/// The category of an audited event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    /// A run-level lifecycle event (start/finish of a stage).
    RunLifecycle,
    /// A classification decision.
    Classification,
    /// A manual triage action (from → to crate).
    TriageAction,
    /// Result of an audio download attempt.
    DownloadResult,
    /// Result of an audio analysis.
    AnalyzeResult,
    /// A track skipped as a duplicate.
    DedupSkip,
    /// Result of a crate export.
    ExportResult,
}

impl AuditKind {
    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunLifecycle => "run_lifecycle",
            Self::Classification => "classification",
            Self::TriageAction => "triage_action",
            Self::DownloadResult => "download_result",
            Self::AnalyzeResult => "analyze_result",
            Self::DedupSkip => "dedup_skip",
            Self::ExportResult => "export_result",
        }
    }

    /// Parses a persistence token back into a kind.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "run_lifecycle" => Some(Self::RunLifecycle),
            "classification" => Some(Self::Classification),
            "triage_action" => Some(Self::TriageAction),
            "download_result" => Some(Self::DownloadResult),
            "analyze_result" => Some(Self::AnalyzeResult),
            "dedup_skip" => Some(Self::DedupSkip),
            "export_result" => Some(Self::ExportResult),
            _ => None,
        }
    }
}

/// The outcome of an audited event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    /// A stage/run began.
    Started,
    /// A stage/run finished successfully.
    Completed,
    /// A new entity (crate/decision/file) was created.
    Created,
    /// An existing entity was updated.
    Updated,
    /// Skipped because it duplicates an existing track.
    SkippedDuplicate,
    /// Skipped because the track has no local audio (FR-024).
    SkippedNoAudio,
    /// The operation failed (per-item, non-fatal).
    Failed,
}

impl AuditOutcome {
    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Created => "created",
            Self::Updated => "updated",
            Self::SkippedDuplicate => "skipped_duplicate",
            Self::SkippedNoAudio => "skipped_no_audio",
            Self::Failed => "failed",
        }
    }

    /// Parses a persistence token back into an outcome.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "started" => Some(Self::Started),
            "completed" => Some(Self::Completed),
            "created" => Some(Self::Created),
            "updated" => Some(Self::Updated),
            "skipped_duplicate" => Some(Self::SkippedDuplicate),
            "skipped_no_audio" => Some(Self::SkippedNoAudio),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Structured, **secret-free** event detail. A sorted map of string key/values so it serializes
/// deterministically to JSON at the adapter edge without the domain importing a JSON type.
/// Never carries tokens/credentials (constitution Principle VII / error-design).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditDetail(BTreeMap<String, String>);

impl AuditDetail {
    /// An empty detail map.
    #[must_use]
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns a copy with `key` set to `value` (builder-style, immutable).
    #[must_use]
    pub fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.0.insert(key.to_owned(), value.into());
        self
    }

    /// Iterates the sorted key/value pairs (used by the adapter to serialize).
    pub fn entries(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    /// Whether the detail map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A durable, queryable audit record. `events_for_track` over these answers
/// "why is this track in this crate?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    id: AuditEventId,
    run_id: RunId,
    track_id: Option<TrackId>,
    stage: PipelineStage,
    kind: AuditKind,
    outcome: AuditOutcome,
    detail: AuditDetail,
    occurred_at: Timestamp,
}

/// Constructor input for an `AuditEvent` (grouped per the 2+-params convention).
#[derive(Debug, Clone)]
pub struct NewAuditEvent {
    /// Minted event id.
    pub id: AuditEventId,
    /// The run this event belongs to.
    pub run_id: RunId,
    /// The track this event concerns, if any (`None` for run-level events).
    pub track_id: Option<TrackId>,
    /// The pipeline stage.
    pub stage: PipelineStage,
    /// The event category.
    pub kind: AuditKind,
    /// The outcome.
    pub outcome: AuditOutcome,
    /// Structured, secret-free detail.
    pub detail: AuditDetail,
    /// When it occurred (from `ClockPort`).
    pub occurred_at: Timestamp,
}

impl AuditEvent {
    /// Builds an audit event from its grouped fields.
    #[must_use]
    pub fn new(fields: NewAuditEvent) -> Self {
        Self {
            id: fields.id,
            run_id: fields.run_id,
            track_id: fields.track_id,
            stage: fields.stage,
            kind: fields.kind,
            outcome: fields.outcome,
            detail: fields.detail,
            occurred_at: fields.occurred_at,
        }
    }

    /// Event identity.
    #[must_use]
    pub const fn id(&self) -> &AuditEventId {
        &self.id
    }

    /// The correlating run id.
    #[must_use]
    pub const fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// The track this event concerns, if any.
    #[must_use]
    pub const fn track_id(&self) -> Option<&TrackId> {
        self.track_id.as_ref()
    }

    /// The pipeline stage.
    #[must_use]
    pub const fn stage(&self) -> PipelineStage {
        self.stage
    }

    /// The event category.
    #[must_use]
    pub const fn kind(&self) -> AuditKind {
        self.kind
    }

    /// The outcome.
    #[must_use]
    pub const fn outcome(&self) -> AuditOutcome {
        self.outcome
    }

    /// Structured, secret-free detail.
    #[must_use]
    pub const fn detail(&self) -> &AuditDetail {
        &self.detail
    }

    /// When it occurred.
    #[must_use]
    pub const fn occurred_at(&self) -> Timestamp {
        self.occurred_at
    }
}
