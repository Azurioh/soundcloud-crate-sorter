//! The `ClassificationDecision` entity — how and why a track reached its crate (Principle VII).

use uuid::Uuid;

use crate::confidence::Confidence;
use crate::crate_::CrateId;
use crate::timestamp::Timestamp;
use crate::track::TrackId;

/// Stable identity of a `ClassificationDecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecisionId(Uuid);

impl DecisionId {
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

/// Whether a decision was made by the system or a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionSource {
    /// Produced by the classification pipeline.
    Auto,
    /// Chosen by the user in triage.
    Manual,
}

impl DecisionSource {
    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }
}

/// The evidence behind a classification — the human-answerable "why".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificationReason {
    /// Genre taken directly from the SoundCloud source tag.
    GenreFromSourceTag,
    /// Genre inferred by the AI classifier (ambiguous/missing source tag).
    GenreFromAi,
    /// Refined using deterministic audio features (BPM/key/energy) — audio-analysis path.
    AudioFeatures,
    /// A human picked the crate in triage.
    ManualPick,
    /// Flagged as likely non-music / non-mixable and routed to a review crate (FR-030).
    LikelyNonMusic,
    /// The AI classifier was unavailable (rate-limited / transport / bad response); the track was
    /// routed to triage without a determined genre. Distinct from `GenreFromAi` so the audit trail
    /// can tell "AI ran, found nothing" apart from "AI never answered" (Principle VII).
    ClassifierUnavailable,
}

impl ClassificationReason {
    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GenreFromSourceTag => "genre_from_source_tag",
            Self::GenreFromAi => "genre_from_ai",
            Self::AudioFeatures => "audio_features",
            Self::ManualPick => "manual_pick",
            Self::LikelyNonMusic => "likely_non_music",
            Self::ClassifierUnavailable => "classifier_unavailable",
        }
    }
}

/// A single, timestamped record of a track being routed to a crate. History is retained; the
/// latest decision is authoritative.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassificationDecision {
    id: DecisionId,
    track_id: TrackId,
    crate_id: CrateId,
    source: DecisionSource,
    confidence: Option<Confidence>,
    reason: ClassificationReason,
    decided_at: Timestamp,
}

/// Constructor input for a `ClassificationDecision` (grouped per the 2+-params convention).
#[derive(Debug, Clone)]
pub struct NewDecision {
    /// Minted decision id.
    pub id: DecisionId,
    /// The track being decided.
    pub track_id: TrackId,
    /// The chosen crate.
    pub crate_id: CrateId,
    /// Auto or manual.
    pub source: DecisionSource,
    /// Confidence (present for auto decisions).
    pub confidence: Option<Confidence>,
    /// Why this crate was chosen.
    pub reason: ClassificationReason,
    /// When the decision was made (from `ClockPort`).
    pub decided_at: Timestamp,
}

impl ClassificationDecision {
    /// Builds a decision from its grouped fields.
    #[must_use]
    pub fn new(fields: NewDecision) -> Self {
        Self {
            id: fields.id,
            track_id: fields.track_id,
            crate_id: fields.crate_id,
            source: fields.source,
            confidence: fields.confidence,
            reason: fields.reason,
            decided_at: fields.decided_at,
        }
    }

    /// Decision identity.
    #[must_use]
    pub const fn id(&self) -> &DecisionId {
        &self.id
    }

    /// The decided track.
    #[must_use]
    pub const fn track_id(&self) -> &TrackId {
        &self.track_id
    }

    /// The chosen crate.
    #[must_use]
    pub const fn crate_id(&self) -> &CrateId {
        &self.crate_id
    }

    /// Auto or manual.
    #[must_use]
    pub const fn source(&self) -> DecisionSource {
        self.source
    }

    /// Confidence, if this was an auto decision.
    #[must_use]
    pub const fn confidence(&self) -> Option<Confidence> {
        self.confidence
    }

    /// The reason/evidence.
    #[must_use]
    pub const fn reason(&self) -> ClassificationReason {
        self.reason
    }

    /// When it was decided.
    #[must_use]
    pub const fn decided_at(&self) -> Timestamp {
        self.decided_at
    }
}
