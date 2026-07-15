//! Shared port-contract suites (T020). Each suite is one set of behavioral assertions run against
//! BOTH the in-memory fake and the real adapter, proving they honor the same contract
//! (constitution Principle VI). Lives under `tests/support/` so every contract test file can
//! `mod support;` and share it (cargo compiles subdirectory files as modules, not test binaries).
//!
//! Each test binary uses only the suites it needs; the rest are dead code in that binary.
#![allow(dead_code)]

use application::ports::audit_log::AuditLogPort;
use application::ports::crate_repository::CrateRepository;
use application::ports::genre_vibe_classifier::{ClassificationInput, GenreVibeClassifierPort};
use application::ports::likes_source::LikesSourcePort;
use application::ports::repo_error::RepoError;
use application::ports::settings_repository::SettingsRepository;
use application::ports::track_repository::TrackRepository;
use domain::audit::{
    AuditDetail, AuditEvent, AuditEventId, AuditKind, AuditOutcome, NewAuditEvent, PipelineStage,
    RunId,
};
use domain::confidence::{Confidence, ConfidenceThreshold};
use domain::crate_::CrateId;
use domain::settings::{ExportMode, Settings};
use domain::timestamp::Timestamp;
use domain::track::{LikedTrack, Track, TrackId, TrackStatus};
use uuid::Uuid;

/// Builds a scanned track with the given internal id seed and source id.
fn scanned_track(id_seed: u128, source_id: &str) -> Track {
    Track::from_scan(
        TrackId::from_uuid(Uuid::from_u128(id_seed)),
        LikedTrack {
            source_track_id: source_id.to_owned(),
            title: "Title".into(),
            artist: "Artist".into(),
            source_genre: Some("House".into()),
            duration_ms: 240_000,
            permalink_url: "https://soundcloud.com/a/track".into(),
            artwork_url: None,
        },
    )
}

/// Confidence helper for the suites.
fn confidence(value: f32) -> Confidence {
    Confidence::new(value).expect("test confidence is in range")
}

/// `TrackRepository` contract: persistence, dedup by source id, and `ManuallyDecided` preservation.
/// `existing_crate_id` MUST already exist in whatever store `repo` is backed by (FK-safe for SQLite).
pub async fn track_repository_suite(repo: &dyn TrackRepository, existing_crate_id: CrateId) {
    let track = scanned_track(1, "sc:1");
    repo.upsert(&track).await.expect("upsert new track");

    let by_source = repo
        .find_by_source_id("sc:1")
        .await
        .expect("find by source");
    assert_eq!(by_source.as_ref().map(|t| *t.id()), Some(*track.id()));
    let by_id = repo.find_by_id(track.id()).await.expect("find by id");
    assert!(by_id.is_some());
    assert_eq!(repo.list_all().await.expect("list all").len(), 1);

    // Update in place: auto-classify into an existing crate.
    let auto = track.assigned_auto(existing_crate_id, confidence(0.9));
    repo.upsert(&auto).await.expect("upsert auto");
    let reloaded = repo
        .find_by_id(track.id())
        .await
        .expect("reload")
        .expect("present");
    assert_eq!(reloaded.status(), TrackStatus::AutoClassified);

    // A human decision must survive a later downgrade attempt (Principle IV).
    let manual = track.assigned_manual(existing_crate_id);
    repo.upsert(&manual).await.expect("upsert manual");
    let downgrade = track.assigned_auto(existing_crate_id, confidence(0.4));
    repo.upsert(&downgrade).await.expect("upsert downgrade");
    let preserved = repo
        .find_by_id(track.id())
        .await
        .expect("reload")
        .expect("present");
    assert_eq!(preserved.status(), TrackStatus::ManuallyDecided);

    // A second internal id claiming the same source id is a constraint violation.
    let duplicate = scanned_track(2, "sc:1");
    let err = repo
        .upsert(&duplicate)
        .await
        .expect_err("duplicate source id rejected");
    assert!(matches!(err, RepoError::Constraint { .. }));

    // Triage listing.
    let triaged = scanned_track(3, "sc:3").sent_to_triage(confidence(0.2));
    repo.upsert(&triaged).await.expect("upsert triaged");
    let in_triage = repo.list_in_triage().await.expect("list triage");
    assert!(in_triage.iter().any(|t| t.id() == triaged.id()));

    // Members of a crate.
    let members = repo
        .list_by_crate(&existing_crate_id)
        .await
        .expect("list by crate");
    assert!(members.iter().any(|t| t.id() == track.id()));
}

/// `CrateRepository` contract: dynamic find-or-create is idempotent on `(genre, role)`.
pub async fn crate_repository_suite(repo: &dyn CrateRepository) {
    let house = repo
        .find_or_create("House", None)
        .await
        .expect("create house");
    let house_again = repo
        .find_or_create("House", None)
        .await
        .expect("find house");
    assert_eq!(
        house.id(),
        house_again.id(),
        "same (genre, role) resolves to one crate"
    );

    let techno = repo
        .find_or_create("Techno", None)
        .await
        .expect("create techno");
    assert_ne!(house.id(), techno.id());

    assert_eq!(repo.list().await.expect("list").len(), 2);
    let fetched = repo.find_by_id(house.id()).await.expect("find by id");
    assert_eq!(
        fetched.as_ref().map(|c| c.genre().to_owned()),
        Some("House".to_owned())
    );
}

/// `AuditLogPort` contract: append-only, ordered by time, with secret-free detail round-tripping.
pub async fn audit_log_suite(log: &dyn AuditLogPort) {
    let track_id = TrackId::from_uuid(Uuid::from_u128(10));
    let run_id = RunId::from_uuid(Uuid::from_u128(11));

    let first = AuditEvent::new(NewAuditEvent {
        id: AuditEventId::from_uuid(Uuid::from_u128(100)),
        run_id,
        track_id: Some(track_id),
        stage: PipelineStage::Classify,
        kind: AuditKind::Classification,
        outcome: AuditOutcome::Created,
        detail: AuditDetail::new().with("genre", "House"),
        occurred_at: Timestamp::from_millis(10),
    });
    let second = AuditEvent::new(NewAuditEvent {
        id: AuditEventId::from_uuid(Uuid::from_u128(101)),
        run_id,
        track_id: Some(track_id),
        stage: PipelineStage::Classify,
        kind: AuditKind::Classification,
        outcome: AuditOutcome::Completed,
        detail: AuditDetail::new().with("status", "auto_classified"),
        occurred_at: Timestamp::from_millis(20),
    });

    // Insert newest-first so a correct implementation must re-order to oldest-first — an insertion-
    // order query would fail this, unlike a pre-sorted fixture.
    log.record(second).await.expect("record second");
    log.record(first).await.expect("record first");

    let events = log
        .events_for_track(&track_id)
        .await
        .expect("events for track");
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].occurred_at().as_millis(),
        10,
        "oldest event must come first regardless of insertion order"
    );
    assert!(events[0].occurred_at().as_millis() < events[1].occurred_at().as_millis());
    let detail: Vec<(String, String)> = events[0]
        .detail()
        .entries()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(detail, vec![("genre".to_string(), "House".to_string())]);
}

/// `LikesSourcePort` contract: resolve then list yields tracks with non-empty source ids.
pub async fn likes_source_suite(source: &dyn LikesSourcePort, profile_url: &str) {
    let user = source
        .resolve_user(profile_url)
        .await
        .expect("resolve user");
    let likes = source.list_likes(&user).await.expect("list likes");
    assert!(!likes.is_empty(), "expected at least one liked track");
    assert!(likes.iter().all(|t| !t.source_track_id.is_empty()));
}

/// `SettingsRepository` contract: an empty store loads the defaults; a saved non-default `Settings`
/// round-trips exactly (including the `export_mode` token) through save→load.
pub async fn settings_repository_suite(repo: &dyn SettingsRepository) {
    // Nothing saved yet → the documented defaults.
    let loaded_default = repo.load().await.expect("load default");
    assert_eq!(loaded_default, Settings::default());

    // A non-default value must survive a save→load round-trip unchanged.
    let threshold = ConfidenceThreshold::new(0.42).expect("in-range threshold");
    let saved = Settings::new(threshold, true, ExportMode::SoundCloud);
    repo.save(&saved).await.expect("save settings");

    let reloaded = repo.load().await.expect("load saved");
    assert_eq!(reloaded, saved);
    assert_eq!(reloaded.export_mode(), ExportMode::SoundCloud);
    assert!(reloaded.download_enabled());
    assert!((reloaded.confidence_threshold().value() - 0.42).abs() < f32::EPSILON);
}

/// `GenreVibeClassifierPort` contract: returns at least one candidate for a plausible track.
pub async fn genre_vibe_classifier_suite(classifier: &dyn GenreVibeClassifierPort) {
    let input = ClassificationInput {
        title: "Midnight Groove".into(),
        artist: "Some Artist".into(),
        source_genre: None,
        description: None,
    };
    let suggestion = classifier.classify(&input).await.expect("classify");
    assert!(
        !suggestion.candidates.is_empty(),
        "expected at least one genre candidate"
    );
}
