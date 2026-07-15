//! US2 checkpoint: the triage slice driven end-to-end against the REAL SQLite adapters.
//!
//! The use-case unit tests prove each step in isolation against fakes; this proves the wired slice —
//! classify → route → triage → decide → re-run — behaves against real persistence, foreign keys and
//! all. It is the spec's "Independent Test" for User Story 2, executed:
//!
//! > Low-confidence tracks appear in the queue (not auto-filed); each triage action assigns the
//! > track to a crate and removes it from the queue; a re-scan never re-presents a decided track.

use std::sync::Arc;

use adapters::sqlite_audit_log::SqliteAuditLog;
use adapters::sqlite_crate_repository::SqliteCrateRepository;
use adapters::sqlite_decision_repository::SqliteDecisionRepository;
use adapters::sqlite_schema::SqliteDatabase;
use adapters::sqlite_settings_repository::SqliteSettingsRepository;
use adapters::sqlite_track_repository::SqliteTrackRepository;
use adapters::uuid_id_provider::UuidIdProvider;
use application::audit_recorder::AuditRecorder;
use application::ports::audit_log::AuditLogPort;
use application::ports::crate_repository::CrateRepository;
use application::ports::decision_repository::DecisionRepository;
use application::ports::id_provider::IdProvider;
use application::ports::settings_repository::SettingsRepository;
use application::ports::track_repository::TrackRepository;
use application::testkit::fixed_clock::FixedClock;
use application::testkit::stub_genre_vibe_classifier::StubGenreVibeClassifier;
use application::use_cases::apply_manual_decision::{
    ApplyManualDecision, ApplyManualDecisionPorts, TriageAction,
};
use application::use_cases::classify_library::ClassifyLibrary;
use application::use_cases::classify_track::{ClassifyTrack, ClassifyTrackPorts};
use application::use_cases::route_to_triage::RouteToTriage;
use domain::audit::{AuditKind, RunId};
use domain::confidence::Confidence;
use domain::track::{LikedTrack, Track, TrackId, TrackStatus};
use uuid::Uuid;

use application::ports::genre_vibe_classifier::{GenreCandidate, GenreVibeSuggestion};

/// The wired slice over one real SQLite database.
struct Pipeline {
    classify_library: ClassifyLibrary,
    apply: ApplyManualDecision,
    tracks: Arc<dyn TrackRepository>,
    crates: Arc<dyn CrateRepository>,
    decisions: Arc<dyn DecisionRepository>,
    audit: Arc<dyn AuditLogPort>,
}

/// Wires the real adapters exactly as the composition root does, but with a stubbed classifier
/// (no network) and a fixed clock (deterministic timestamps).
fn pipeline(suggestion: GenreVibeSuggestion) -> Pipeline {
    let db = SqliteDatabase::open_in_memory().expect("open db");
    let ids: Arc<dyn IdProvider> = Arc::new(UuidIdProvider::new());
    let clock = Arc::new(FixedClock::at_millis(1_000));

    let tracks: Arc<dyn TrackRepository> = Arc::new(SqliteTrackRepository::new(db.connection()));
    let crates: Arc<dyn CrateRepository> =
        Arc::new(SqliteCrateRepository::new(db.connection(), ids.clone()));
    let decisions: Arc<dyn DecisionRepository> =
        Arc::new(SqliteDecisionRepository::new(db.connection()));
    let audit: Arc<dyn AuditLogPort> = Arc::new(SqliteAuditLog::new(db.connection()));
    let settings: Arc<dyn SettingsRepository> =
        Arc::new(SqliteSettingsRepository::new(db.connection()));
    let recorder = Arc::new(AuditRecorder::new(
        audit.clone(),
        clock.clone(),
        ids.clone(),
    ));

    let classify = Arc::new(ClassifyTrack::new(ClassifyTrackPorts {
        classifier: Arc::new(StubGenreVibeClassifier::always(suggestion)),
        crates: crates.clone(),
        decisions: decisions.clone(),
        audit: recorder.clone(),
        clock: clock.clone(),
        ids: ids.clone(),
    }));
    let route = Arc::new(RouteToTriage::new(tracks.clone(), recorder.clone()));
    let classify_library =
        ClassifyLibrary::new(tracks.clone(), settings, classify, route, ids.clone());
    let apply = ApplyManualDecision::new(ApplyManualDecisionPorts {
        tracks: tracks.clone(),
        crates: crates.clone(),
        decisions: decisions.clone(),
        audit: recorder,
        clock,
        ids,
    });

    Pipeline {
        classify_library,
        apply,
        tracks,
        crates,
        decisions,
        audit,
    }
}

/// A genre-less track, so classification must go through the (stubbed) AI and land on its score.
fn untagged_track(seed: u128) -> Track {
    Track::from_scan(
        TrackId::from_uuid(Uuid::from_u128(seed)),
        LikedTrack {
            source_track_id: format!("sc:{seed}"),
            title: "Night Drive".to_owned(),
            artist: "Artist".to_owned(),
            source_genre: None,
            duration_ms: 240_000,
            permalink_url: "https://soundcloud.com/a/night-drive".to_owned(),
            artwork_url: None,
        },
    )
}

/// A suggestion below the 0.6 default threshold, with a runner-up to become a triage chip.
fn low_confidence_suggestion() -> GenreVibeSuggestion {
    GenreVibeSuggestion {
        candidates: vec![
            GenreCandidate {
                genre: "Deep House".to_owned(),
                confidence: Confidence::new(0.35).expect("in range"),
            },
            GenreCandidate {
                genre: "Techno".to_owned(),
                confidence: Confidence::new(0.3).expect("in range"),
            },
        ],
        vibe_tags: vec!["dark".to_owned()],
    }
}

#[tokio::test]
async fn a_low_confidence_track_reaches_triage_is_decided_once_and_never_returns() {
    let pipeline = pipeline(low_confidence_suggestion());
    let track = untagged_track(1);
    pipeline.tracks.upsert(&track).await.expect("seed track");

    // Classify: 0.35 is under the 0.6 default threshold, so the track must NOT be auto-filed.
    let summary = pipeline.classify_library.execute().await.expect("classify");
    assert_eq!(summary.auto_classified, 0);
    assert_eq!(summary.sent_to_triage, 1);

    let queue = pipeline.tracks.list_in_triage().await.expect("queue");
    assert_eq!(queue.len(), 1, "the uncertain track is queued, not filed");

    // The card can be rebuilt: the decision survived even though the track dropped its crate.
    let decision = pipeline
        .decisions
        .find_latest_for_track(track.id())
        .await
        .expect("decision")
        .expect("classify recorded one");
    let suggested = *decision.crate_id();
    let suggested_crate = pipeline
        .crates
        .find_by_id(&suggested)
        .await
        .expect("crate")
        .expect("present");
    assert_eq!(suggested_crate.genre(), "Deep House");
    assert_eq!(
        decision.alternatives().first().map(|a| a.genre.as_str()),
        Some("Techno"),
        "the runner-up survives as the card's alternative chip"
    );

    // Accept the suggestion.
    let result = pipeline
        .apply
        .execute(
            RunId::from_uuid(Uuid::from_u128(77)),
            track.id(),
            TriageAction::AcceptSuggestion,
        )
        .await
        .expect("accept");
    assert_eq!(result.crate_id, Some(suggested));

    // The decided track leaves the queue and joins its crate.
    assert!(
        pipeline
            .tracks
            .list_in_triage()
            .await
            .expect("queue")
            .is_empty(),
        "an accepted track leaves the queue"
    );
    let members = pipeline
        .tracks
        .list_by_crate(&suggested)
        .await
        .expect("members");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].status(), TrackStatus::ManuallyDecided);

    // Re-running classification must not re-present it (FR-018, Principle IV).
    let rerun = pipeline
        .classify_library
        .execute()
        .await
        .expect("re-classify");
    assert_eq!(rerun.sent_to_triage, 0);
    assert_eq!(
        rerun.skipped, 1,
        "the decided track is skipped, not re-routed"
    );
    assert!(
        pipeline
            .tracks
            .list_in_triage()
            .await
            .expect("queue")
            .is_empty(),
        "a decided track is never re-presented"
    );

    // Principle VII: the trail answers "why is this track in this crate?" — the classifier's call
    // and the human's override are both there, in order.
    let events = pipeline
        .audit
        .events_for_track(track.id())
        .await
        .expect("audit");
    let kinds: Vec<AuditKind> = events.iter().map(|e| e.kind()).collect();
    assert!(kinds.contains(&AuditKind::Classification));
    assert!(kinds.contains(&AuditKind::TriageAction));
    let triage = events
        .iter()
        .rev()
        .find(|e| e.kind() == AuditKind::TriageAction)
        .expect("triage event");
    let detail: Vec<(String, String)> = triage
        .detail()
        .entries()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert!(detail.contains(&("action".to_owned(), "accept_suggestion".to_owned())));
    assert!(detail.contains(&("to_crate".to_owned(), suggested.to_string())));
}

/// Deferring must park the track without deciding it, and it must come back on request — otherwise
/// "later" quietly means "never".
#[tokio::test]
async fn a_deferred_track_leaves_the_queue_and_returns_on_request() {
    let pipeline = pipeline(low_confidence_suggestion());
    let track = untagged_track(2);
    pipeline.tracks.upsert(&track).await.expect("seed track");
    pipeline.classify_library.execute().await.expect("classify");

    pipeline
        .apply
        .execute(
            RunId::from_uuid(Uuid::from_u128(78)),
            track.id(),
            TriageAction::Defer,
        )
        .await
        .expect("defer");

    assert!(
        pipeline
            .tracks
            .list_in_triage()
            .await
            .expect("queue")
            .is_empty(),
        "a deferred track leaves the current session's queue"
    );
    let deferred = pipeline.tracks.list_deferred().await.expect("deferred");
    assert_eq!(deferred.len(), 1, "but it stays visible as unfinished work");

    pipeline
        .tracks
        .upsert(&deferred[0].returned_to_triage())
        .await
        .expect("resume");
    assert_eq!(
        pipeline.tracks.list_in_triage().await.expect("queue").len(),
        1
    );
}
