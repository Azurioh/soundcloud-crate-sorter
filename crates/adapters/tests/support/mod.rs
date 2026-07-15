//! Shared port-contract suites (T020). Each suite is one set of behavioral assertions run against
//! BOTH the in-memory fake and the real adapter, proving they honor the same contract
//! (constitution Principle VI). Lives under `tests/support/` so every contract test file can
//! `mod support;` and share it (cargo compiles subdirectory files as modules, not test binaries).
//!
//! Each test binary uses only the suites it needs; the rest are dead code in that binary.
#![allow(dead_code)]

pub mod wav_fixture;

use std::path::Path;

use application::ports::audio_analyzer::AudioAnalyzerPort;
use application::ports::audio_downloader::AudioDownloaderPort;
use application::ports::audit_log::AuditLogPort;
use application::ports::crate_repository::{CrateRepository, CrateSpec};
use application::ports::decision_repository::DecisionRepository;
use application::ports::genre_vibe_classifier::{ClassificationInput, GenreVibeClassifierPort};
use application::ports::likes_source::LikesSourcePort;
use application::ports::repo_error::RepoError;
use application::ports::settings_repository::SettingsRepository;
use application::ports::track_repository::TrackRepository;
use domain::audio::{AudioFeatures, TempoAmbiguity};
use domain::audit::{
    AuditDetail, AuditEvent, AuditEventId, AuditKind, AuditOutcome, NewAuditEvent, PipelineStage,
    RunId,
};
use domain::camelot_key::{CamelotKey, CamelotLetter};
use domain::classification::{
    ClassificationDecision, ClassificationReason, DecisionId, DecisionSource, GenreSuggestion,
    NewDecision,
};
use domain::confidence::{Confidence, ConfidenceThreshold, Energy};
use domain::crate_::{CrateId, CrateOrigin};
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

    // Deferring moves a track off the queue and onto the deferred list — the two are disjoint, so a
    // deferred track neither reappears in the current session nor drops out of the library's
    // unfinished work.
    let deferred = triaged.deferred();
    repo.upsert(&deferred).await.expect("upsert deferred");
    let in_triage = repo.list_in_triage().await.expect("list triage");
    assert!(!in_triage.iter().any(|t| t.id() == deferred.id()));
    let deferred_list = repo.list_deferred().await.expect("list deferred");
    assert!(deferred_list.iter().any(|t| t.id() == deferred.id()));

    // Resuming puts it back on the queue.
    repo.upsert(&deferred.returned_to_triage())
        .await
        .expect("upsert resumed");
    assert!(repo
        .list_in_triage()
        .await
        .expect("list triage")
        .iter()
        .any(|t| t.id() == deferred.id()));
    assert!(repo
        .list_deferred()
        .await
        .expect("list deferred")
        .is_empty());

    // Members of a crate.
    let members = repo
        .list_by_crate(&existing_crate_id)
        .await
        .expect("list by crate");
    assert!(members.iter().any(|t| t.id() == track.id()));

    audio_features_round_trip(repo).await;
}

/// The audio-analysis half of the `TrackRepository` contract (US4): every measured feature must
/// survive a save→load unchanged, including the doubt attached to an uncertain tempo (FR-031).
///
/// Analysis is expensive and deterministic, so a feature that does not round-trip is worse than one
/// never measured: the run reports success and the library quietly holds nothing. The uncertain-tempo
/// flag matters most — losing it would silently promote a BPM the analyzer distrusted into one the
/// exporter tags as fact.
async fn audio_features_round_trip(repo: &dyn TrackRepository) {
    let analyzed = scanned_track(4, "sc:4")
        .downloaded("/tmp/audio/sc-4.mp3".into())
        .analyzed(AudioFeatures {
            bpm: 128,
            tempo_ambiguity: TempoAmbiguity::Confident,
            key: Some(CamelotKey::new(8, CamelotLetter::B).expect("8B is on the wheel")),
            energy: Energy::new(85).expect("85 is in range"),
        });
    repo.upsert(&analyzed).await.expect("upsert analyzed track");

    let reloaded = repo
        .find_by_id(analyzed.id())
        .await
        .expect("reload analyzed")
        .expect("present");
    assert_eq!(reloaded.bpm(), Some(128));
    assert_eq!(
        reloaded.camelot_key().map(|k| k.to_string()),
        Some("8B".into())
    );
    assert_eq!(reloaded.energy().map(Energy::value), Some(85));
    assert_eq!(reloaded.tempo_ambiguity(), Some(TempoAmbiguity::Confident));
    assert!(!reloaded.has_uncertain_tempo());
    assert_eq!(
        reloaded.local_audio_path(),
        Some(&std::path::PathBuf::from("/tmp/audio/sc-4.mp3"))
    );

    // The doubt itself must persist, not just the number it qualifies.
    let uncertain = analyzed.analyzed(AudioFeatures {
        bpm: 70,
        tempo_ambiguity: TempoAmbiguity::HalfOrDoubleTime,
        key: None,
        energy: Energy::new(20).expect("20 is in range"),
    });
    repo.upsert(&uncertain).await.expect("upsert uncertain");

    let reloaded = repo
        .find_by_id(uncertain.id())
        .await
        .expect("reload uncertain")
        .expect("present");
    assert!(
        reloaded.has_uncertain_tempo(),
        "an uncertain tempo must not come back trusted"
    );
    assert_eq!(
        reloaded.camelot_key(),
        None,
        "a track with no detectable key must not gain one on reload"
    );
}

/// Builds a `CrateSpec` for a genre with no energy role, created by the pipeline.
fn auto_spec(genre: &str) -> CrateSpec {
    CrateSpec {
        genre: genre.to_owned(),
        role: None,
        origin: CrateOrigin::Auto,
    }
}

/// `CrateRepository` contract: dynamic find-or-create is idempotent on `(genre, role)`, and the
/// origin is stamped on creation only.
pub async fn crate_repository_suite(repo: &dyn CrateRepository) {
    let house = repo
        .find_or_create(&auto_spec("House"))
        .await
        .expect("create house");
    let house_again = repo
        .find_or_create(&auto_spec("House"))
        .await
        .expect("find house");
    assert_eq!(
        house.id(),
        house_again.id(),
        "same (genre, role) resolves to one crate"
    );

    let techno = repo
        .find_or_create(&auto_spec("Techno"))
        .await
        .expect("create techno");
    assert_ne!(house.id(), techno.id());

    assert_eq!(repo.list().await.expect("list").len(), 2);
    let fetched = repo.find_by_id(house.id()).await.expect("find by id");
    assert_eq!(
        fetched.as_ref().map(|c| c.genre().to_owned()),
        Some("House".to_owned())
    );

    // A crate created in triage is recorded as manual...
    let manual = repo
        .find_or_create(&CrateSpec {
            genre: "Breakbeat".to_owned(),
            role: None,
            origin: CrateOrigin::Manual,
        })
        .await
        .expect("create manual crate");
    assert_eq!(manual.created_by(), CrateOrigin::Manual);

    // ...but resolving an existing crate must never rewrite the origin it was born with, or picking
    // an auto crate in triage would silently rewrite how it came to exist.
    let resolved = repo
        .find_or_create(&CrateSpec {
            genre: "House".to_owned(),
            role: None,
            origin: CrateOrigin::Manual,
        })
        .await
        .expect("resolve existing house");
    assert_eq!(resolved.id(), house.id());
    assert_eq!(
        resolved.created_by(),
        CrateOrigin::Auto,
        "an existing crate keeps its original origin"
    );
}

/// `DecisionRepository` contract: append-only history, latest-wins per track, alternatives and the
/// `Manual`/no-confidence shape round-tripping intact.
/// `track_id` / `crate_id` MUST already exist in whatever store `repo` is backed by (FK-safe for
/// SQLite).
pub async fn decision_repository_suite(
    repo: &dyn DecisionRepository,
    track_id: TrackId,
    crate_id: CrateId,
) {
    // A track that was never classified has no decision — absence is not an error.
    let missing = repo
        .find_latest_for_track(&track_id)
        .await
        .expect("query unknown track");
    assert!(missing.is_none());

    let auto = ClassificationDecision::new(NewDecision {
        id: DecisionId::from_uuid(Uuid::from_u128(900)),
        track_id,
        crate_id,
        source: DecisionSource::Auto,
        confidence: Some(confidence(0.42)),
        reason: ClassificationReason::GenreFromAi,
        alternatives: vec![
            GenreSuggestion {
                genre: "Techno".to_owned(),
                confidence: confidence(0.31),
            },
            GenreSuggestion {
                genre: "Trance".to_owned(),
                confidence: confidence(0.2),
            },
        ],
        decided_at: Timestamp::from_millis(1_000),
    });
    repo.record(&auto).await.expect("record auto decision");

    let loaded = repo
        .find_latest_for_track(&track_id)
        .await
        .expect("load decision")
        .expect("present");
    assert_eq!(loaded.crate_id(), &crate_id);
    assert_eq!(loaded.source(), DecisionSource::Auto);
    assert_eq!(loaded.reason(), ClassificationReason::GenreFromAi);
    assert_eq!(
        loaded.confidence().map(Confidence::value),
        Some(0.42),
        "confidence must survive the round-trip"
    );
    let alternatives: Vec<(&str, f32)> = loaded
        .alternatives()
        .iter()
        .map(|a| (a.genre.as_str(), a.confidence.value()))
        .collect();
    assert_eq!(
        alternatives,
        vec![("Techno", 0.31), ("Trance", 0.2)],
        "alternatives round-trip in order — they are the triage card's chips"
    );

    // A later manual decision supersedes the auto one: history is kept, latest wins.
    let manual = ClassificationDecision::new(NewDecision {
        id: DecisionId::from_uuid(Uuid::from_u128(901)),
        track_id,
        crate_id,
        source: DecisionSource::Manual,
        confidence: None,
        reason: ClassificationReason::ManualPick,
        alternatives: Vec::new(),
        decided_at: Timestamp::from_millis(2_000),
    });
    repo.record(&manual).await.expect("record manual decision");

    let latest = repo
        .find_latest_for_track(&track_id)
        .await
        .expect("load latest")
        .expect("present");
    assert_eq!(latest.source(), DecisionSource::Manual);
    assert_eq!(latest.reason(), ClassificationReason::ManualPick);
    assert_eq!(
        latest.confidence(),
        None,
        "a manual pick carries no confidence, and None must not become 0.0"
    );
    assert!(latest.alternatives().is_empty());
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

/// `AudioDownloaderPort` contract: a successful download leaves a real file inside `dest_dir` and
/// returns its path.
///
/// The contract is deliberately about the *file*, not its contents: what a downloader owes its
/// caller is "audio is now on disk, here". Whether those bytes decode is `AudioAnalyzerPort`'s
/// business — which is why the in-memory fake can honor this suite without pretending to be an
/// encoder.
pub async fn audio_downloader_suite(
    downloader: &dyn AudioDownloaderPort,
    track: &Track,
    dest_dir: &Path,
) {
    let audio = downloader
        .download(track, dest_dir)
        .await
        .expect("download the track");

    assert!(
        audio.path.is_file(),
        "the returned path must point at a file that exists"
    );
    assert!(
        audio.path.starts_with(dest_dir),
        "a download must stay inside the destination it was given"
    );
    assert!(
        audio.path.metadata().expect("file metadata").len() > 0,
        "an empty file is not a download"
    );
}

/// `AudioAnalyzerPort` contract: analysis is **deterministic** and its output is in range.
///
/// Determinism is the whole reason BPM/key/energy are allowed to be trusted at all (Principle I), so
/// it is the contract both sides must honor. Correctness against known-key audio is asserted
/// separately, against the real adapter only: an in-memory fake cannot know what is in a file, and a
/// suite that demanded it would be testing the fake's hard-coded answer, not the port.
pub fn audio_analyzer_suite(analyzer: &dyn AudioAnalyzerPort, audio_path: &Path) {
    let first = analyzer.analyze(audio_path).expect("analyze the fixture");
    let second = analyzer
        .analyze(audio_path)
        .expect("analyze the same fixture again");

    assert_eq!(
        first, second,
        "the same file must always analyze to the same features"
    );
    assert!(first.bpm > 0, "a reported tempo must be a real one");
    assert!(
        first.energy.value() <= 100,
        "energy must stay on the normalized scale"
    );
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
