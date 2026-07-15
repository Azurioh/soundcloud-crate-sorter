# Tasks: SoundCloud Crate Sorter

**Input**: Design documents from `specs/001-soundcloud-crate-sorter/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/ports.md

**Tests**: Included — the project **constitution (Principle VI)** mandates use-case isolation tests
(in-memory port fakes) and port contract tests (in-memory AND real adapter). These are non-optional.

**Organization**: Grouped by user story so each is independently implementable and testable.
v1 = US1, US2, US4, US5. **US3 (SoundCloud export) is deferred to v2** — see the Deferred section.

## Format: `[ID] [P?] [Story] Description with file path`

- **[P]**: parallelizable (different files, no incomplete dependencies)
- **[Story]**: US1/US2/US4/US5 (Setup/Foundational/Polish carry no story label)

## Path Conventions

Rust Cargo workspace: `crates/domain`, `crates/application`, `crates/adapters`, `crates/app`.
Frontend: `ui/`. Tests: `tests/contract`, `tests/integration`, plus `#[cfg(test)]` in each crate.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Workspace, tooling, system-lib prerequisites.

- [X] T001 Create Cargo workspace with member crates `domain`, `application`, `adapters`, `app` in root `Cargo.toml` and per-crate `Cargo.toml`
- [X] T002 [P] Pin workspace dependencies (verify each on crates.io first): tauri v2, reqwest, serde/serde_json, rusqlite, tracing, uuid, lofty, symphonia, aubio-rs, libkeyfinder-sys in `Cargo.toml`
- [X] T003 [P] Configure rustfmt + clippy (`-D warnings`) in `rustfmt.toml` and `clippy.toml`
- [X] T004 [P] Write `scripts/check-system-libs.sh` verifying `brew` libkeyfinder/fftw/aubio + `yt-dlp` + `pkg-config --exists libkeyfinder`; document in `README.md`
- [X] T005 Scaffold Tauri v2 app (`crates/app`) + React+TS+Vite frontend (`ui/`) with the invoke bridge
- [X] T006 [P] Load `ANTHROPIC_API_KEY` from env in `crates/app` config; ensure it is never logged or committed

**Checkpoint**: `cargo build` and `cargo tauri dev` run an empty shell.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Entities, port traits, providers, persistence, audit, logging — shared by ALL stories.
**⚠️ No user story can start until this phase is complete.**

### Entities (crates/domain)

- [X] T007 [P] `Track`, `TrackId`, `TrackStatus` in `crates/domain/src/track.rs`
- [X] T008 [P] `Crate`, `CrateId`, `EnergyRole`, `CrateOrigin` in `crates/domain/src/crate_.rs`
- [X] T009 [P] `CamelotKey` value object in `crates/domain/src/camelot_key.rs`
- [X] T010 [P] `Confidence`, `ConfidenceThreshold`, `Energy` newtypes in `crates/domain/src/confidence.rs`
- [X] T011 [P] `ClassificationDecision`, `DecisionSource`, `ClassificationReason` in `crates/domain/src/classification.rs`
- [X] T012 [P] `AuditEvent`, `PipelineStage`, `AuditKind` in `crates/domain/src/audit.rs`

### Port traits (crates/application/src/ports) — per contracts/ports.md

- [X] T013 Define all port traits + typed error enums in `crates/application/src/ports/` (one file each: likes_source, audio_downloader, audio_analyzer, genre_vibe_classifier, tag_writer, track_repository, crate_repository, audit_log, clock, id_provider, playlist_publisher[v2-stub])

### Providers + fakes + persistence

- [X] T014 [P] `system-clock.provider` + `uuid-id.provider` adapters in `crates/adapters/src/system_clock_provider.rs`, `uuid_id_provider.rs`
- [X] T015 [P] Seeded in-memory fakes for `ClockPort`/`IdProvider` in `crates/application/src/testkit/`
- [X] T016 SQLite schema + migrations (tracks, crates, classification_decisions, audit_events, settings; `tracks.source_track_id` UNIQUE; indexes on audit run_id/track_id) in `crates/adapters/src/sqlite_schema.rs`
- [X] T017 `sqlite-track.repository` + `sqlite-crate.repository` (map domain↔rows at edge) in `crates/adapters/src/sqlite_track_repository.rs`, `sqlite_crate_repository.rs`
- [X] T018 `sqlite-audit.log` adapter (append-only + `events_for_track`) in `crates/adapters/src/sqlite_audit_log.rs`
- [X] T019 [P] In-memory fakes for TrackRepository/CrateRepository/AuditLog in `crates/application/src/testkit/`
- [X] T020 Port contract-test harness that runs one suite against BOTH in-memory and real adapter in `tests/contract/harness.rs`
- [X] T021 [P] `tracing` subscriber (structured, leveled, secret-redacting) in `crates/app/src/observability.rs`
- [X] T022 Composition-root skeleton wiring providers + repos + audit in `crates/app/src/composition_root.rs`
- [X] T023 Tauri command bridge skeleton in `crates/app/src/commands.rs`
- [X] T024 `Settings` entity + `sqlite` settings store (confidence_threshold, download_enabled=false default, export_mode=local) in `crates/domain/src/settings.rs` + `crates/adapters/src/sqlite_settings_repository.rs`

**Checkpoint**: entities, ports, providers, SQLite, audit, logging compile; contract harness runs against fakes.

---

## Phase 3: User Story 1 — Import likes and get auto-sorted crates (Priority: P1) 🎯 MVP

**Goal**: Scan public likes, dedup, auto-classify each into a crate + confidence, no download needed.
**Independent test**: Given a public likes URL → every unique track shows a crate + confidence; dupes collapsed; crates only for genres present.

### Tests (write first)

- [X] T025 [P] [US1] Contract test `LikesSourcePort` (in-memory + real) in `tests/contract/likes_source.rs`
- [X] T026 [P] [US1] Contract test `GenreVibeClassifierPort` in `tests/contract/genre_vibe_classifier.rs`
- [X] T027 [P] [US1] Unit tests for ScanLikes / DeduplicateLibrary / ClassifyTrack / RouteToTriage with fakes in each use-case's `#[cfg(test)]`

### Adapters

- [X] T028 [US1] `internal-api-likes.source`: `/resolve` + `/users/{id}/likes/tracks`, `linked_partitioning` cursor, rotating client_id refresh, **rate-limit backoff/retry** (research R1), map to `LikedTrack` in `crates/adapters/src/internal_api_likes_source.rs`
- [X] T029 [US1] `anthropic-genre-vibe.classifier`: Claude `claude-haiku-4-5-20251001` over HTTP, JSON schema → genre candidates + confidence + vibe (never BPM/key/energy) in `crates/adapters/src/anthropic_genre_vibe_classifier.rs`

### Use cases (crates/application/src/use_cases)

- [X] T030 [US1] `ScanLikes` (resolve → list → upsert new only) in `scan_likes.rs`
- [X] T031 [US1] `DeduplicateLibrary` (collapse by `source_track_id`) in `deduplicate_library.rs`
- [X] T032 [US1] `ClassifyTrack` (genre from source tag; call classifier only if ambiguous/missing; set confidence + `ClassificationReason`; **detect likely non-music/non-mixable uploads by duration/type → dedicated review crate, FR-030**) in `classify_track.rs`
- [X] T033 [US1] `RouteToTriage` (confidence vs threshold; ties → triage) in `route_to_triage.rs`
- [X] T034 [US1] `CrateRepository.find_or_create` dynamic crate creation (genre only in metadata-only mode) in `crates/adapters/src/sqlite_crate_repository.rs`

### Wiring + UI + audit

- [X] T035 [US1] Wire US1 adapters + use cases in composition root; add Tauri commands `scan`, `classify_all`, `list_crates`, `run_summary` in `crates/app/src/commands.rs`
- [X] T036 [P] [US1] UI: scan/run control + progress + run summary (counts) in `ui/src/features/run/`
- [X] T037 [P] [US1] UI: crate browsing view (crates + members + confidence) in `ui/src/features/crates/`
- [X] T038 [US1] Emit audit events for scan/dedup/classify (score + reason) in the US1 use cases

**Checkpoint**: US1 delivers a working MVP — flat likes become confidence-scored crates.

---

## Phase 4: User Story 2 — Triage uncertain tracks (Priority: P2)

**Goal**: Sub-threshold tracks resolved via playful swipe UI + table; decisions persist.
**Independent test**: Low-confidence tracks appear in the queue; each action assigns + removes; re-scan never re-presents decided tracks.

### Tests

- [X] T039 [P] [US2] Unit tests for `ApplyManualDecision` + `list_in_triage` with fakes

### Implementation

- [X] T040 [US2] `ApplyManualDecision` use case (accept / pick alt / create crate / defer; persist; audit) in `crates/application/src/use_cases/apply_manual_decision.rs`
- [X] T041 [US2] `TrackRepository.list_in_triage` query in `crates/adapters/src/sqlite_track_repository.rs`
- [X] T042 [US2] Tauri commands: triage actions, create-crate-on-the-fly, adjust threshold (+ live auto/manual split preview) in `crates/app/src/commands.rs`
- [X] T043 [P] [US2] UI swipe cards (`motion` + `@use-gesture/react`): audio preview, top suggestion, alt chips, full searchable picker in `ui/src/features/triage/SwipeDeck.tsx`
- [X] T044 [P] [US2] UI dense assignment table (`@tanstack/react-table` + Virtual) sharing the commit-assignment action in `ui/src/features/triage/AssignmentTable.tsx`
- [X] T045 [US2] Keyboard shortcuts (`react-hotkeys-hook`) → accept / alt-1..3 / picker / defer in `ui/src/features/triage/`
- [X] T046 [US2] Emit audit events for triage actions (from→to crate + timestamp)
- [X] T047 [US2] Threshold re-evaluation preserves `ManuallyDecided` (idempotency, Principle IV) in `RouteToTriage`

### Added during US2 implementation (not in the original breakdown)

FR-015 needs a triage card's top suggestion + alternatives, but a triaged track drops its crate and
nothing persisted the classifier's conclusion — so the queue had nothing to rebuild a card from.

- [X] T047a [US2] Revive `ClassificationDecision`: `DecisionRepository` port + `classification_decisions` table + `sqlite-decision.repository` + in-memory fake + contract suite (both sides). `ClassifyTrack` now persists the decision and keeps the runners-up (`GenreSuggestion`, genre-not-crate so a guessed alternative never creates an empty crate — FR-009)
- [X] T047b [US2] `CrateSpec { genre, role, origin }` on `CrateRepository.find_or_create` so a crate created in triage is recorded as `CrateOrigin::Manual` (data-model); an existing crate keeps its original origin
- [X] T047c [US2] `AdjustThreshold` use case: `preview` (FR-013 split, commits nothing) + `apply` (persist + re-route by replaying stored decisions). Closes the gap where `ClassifyLibrary` only processes `Scanned` tracks, so a threshold change moved nothing
- [X] T047d [US2] `ResumeDeferred` use case + `TrackRepository.list_deferred` + `Track::returned_to_triage` — the data-model's `Deferred → InTriage` edge had no implementation, so a deferred track was unreachable forever
- [X] T047e [US2] Integration test: the US2 slice (classify → triage → decide → re-run) against real SQLite in `crates/adapters/tests/triage_pipeline.rs`

**Checkpoint**: US1 + US2 — full metadata-only workflow, uncertain tracks resolvable and durable.

---

## Phase 5: User Story 4 — Enrich classification with audio analysis (Priority: P3, opt-in)

**Goal**: Opt-in download → deterministic BPM/Camelot-key/energy → energy sub-roles + better confidence.
**Independent test**: With download on, downloaded tracks gain BPM/key/energy (same file → same values); undownloadable tracks stay metadata-only, never blocking.
**⚠️ FFI/build risk (research R4) is isolated to T052/T053 behind `AudioAnalyzerPort`.**

### Tests

- [X] T048 [P] [US4] Contract test `AudioDownloaderPort` (in-memory + real) in `crates/adapters/tests/audio_downloader_contract.rs`
- [X] T049 [P] [US4] Contract test `AudioAnalyzerPort` — determinism (both sides) + known-key/known-tempo fixtures against the real adapter in `crates/adapters/tests/audio_analyzer_contract.rs`
- [X] T050 [P] [US4] Unit tests for `DownloadAudio` / `AnalyzeAudio` with fakes

### Implementation

- [X] T051 [US4] `ytdlp-audio.downloader` adapter (subprocess; per-track failure = typed reported skip) in `crates/adapters/src/ytdlp_audio_downloader.rs`
- [X] T052 [US4] `libkeyfinder-aubio-audio.analyzer`: symphonia → f32 PCM; aubio BPM (**flag half-/double-time ambiguity as uncertain → triage, FR-031**); libKeyFinder FFI key; RMS energy; `key_t`→Camelot table (SILENCE→None) in `crates/adapters/src/libkeyfinder_aubio_audio_analyzer.rs`
- [X] T053 [US4] Vendor/fork `libkeyfinder-sys`; wire `build.rs` (pkg-config, fftw) + document Apple-Silicon build in `crates/adapters/vendor/libkeyfinder-sys/`
- [X] T054 [US4] `DownloadAudio` use case with explicit opt-in gate in `crates/application/src/use_cases/download_audio.rs`
- [X] T055 [US4] `AnalyzeAudio` use case (writes bpm/key/energy; the energy → `EnergyRole` mapping lives in the domain and is applied by `ClassifyTrack` — T056)
- [X] T056 [US4] Extend `ClassifyTrack`/crate creation to add energy sub-role crates + confidence uplift when audio features present; **refine genre-only auto-classified tracks into the energy sub-crate, but NEVER move a `ManuallyDecided` track without explicit confirmation (FR-029, Principle IV)**
- [X] T057 [US4] Tauri commands (`set_download_enabled`, `analyze_library`) + UI opt-in download gate (ToS/personal-use notice) + analysis progress (resumable) in `ui/src/features/run/AudioAnalysisPanel.tsx`
- [X] T058 [US4] Emit audit events for download + analysis results

### Added during US4 implementation (not in the original breakdown)

The audio path needed four things the breakdown did not name. Each is recorded here for the same
reason T047a–e were: a later reader should find the gap explained, not the code unexplained.

- [X] T058a [US4] `domain::audio` (`AudioFeatures`, `TempoAmbiguity`) + `Track::downloaded`/`analyzed`/`has_uncertain_tempo` + `EnergyRole::for_energy`. FR-031 requires a track to *remember* that its BPM is doubtful — a bool the data-model never had — so it needed a domain type, a `tracks.tempo_ambiguity` column, and an additive `ALTER TABLE` migration (`CREATE TABLE IF NOT EXISTS` never alters an existing library)
- [X] T058b [US4] FR-031 routing in `RouteToTriage`: an uncertain tempo sends a track to triage regardless of genre confidence. Placed there because every path into a crate already runs through it, so no caller can route around the rule
- [X] T058c [US4] `AnalyzeLibrary` orchestrator (`analyze_library.rs`) — the library-level download→analyze→refine pass the UI drives as one action; the equivalent of `ClassifyLibrary` for the audio path. Halts the run on an environmental failure (download off, `yt-dlp` missing) instead of repeating it per track
- [X] T058d [US4] Energy→role banding decided with the user: `Closing` (0–19) < `Warmup` (20–44) < `Groove` (45–69) < `Peak` (70–100). Energy is one scalar but the roles encode set position, so warmup and closing are not separable by measurement; the band order is a documented judgement, overridable in triage

**Checkpoint**: DJ-grade metadata (BPM/key/energy) attached; crates gain energy roles. ✅ Reached —
`cargo test --workspace` (148) green offline; `cargo test -p adapters --features real-adapters
--test audio_analyzer_contract` (7) green against the real FFI: a C-major triad reads as Camelot 8B,
a 120 BPM click track as ~120 confident, silence as `Silence`, and analysis is deterministic.

---

## Phase 6: User Story 5 — Export crates to local folders for Rekordbox/USB (Priority: P3)

**Goal**: Per-crate folders + ID3-tagged files + M3U8, importable into Rekordbox. Requires downloaded audio.
**Independent test**: Classified crates with downloaded audio → folder per crate + tagged files + M3U8; tracks without audio skipped + reported; imports into Rekordbox with no rework.

### Tests

- [ ] T059 [P] [US5] Contract test `TagWriterPort` (in-memory + real: assert ID3 frames + M3U8 content) in `tests/contract/tag_writer.rs`
- [ ] T060 [P] [US5] Unit tests `ExportToLocal` incl. skip-no-audio path with fakes

### Implementation

- [ ] T061 [US5] `lofty-tag.writer` adapter: write `TCON`/`TBPM`/`TKEY`/`COMM`/`TXXX` + emit M3U8 per crate in `crates/adapters/src/lofty_tag_writer.rs`
- [ ] T062 [US5] `ExportToLocal` use case: folder-per-crate layout; tag files; skip + report no-audio (FR-024) in `crates/application/src/use_cases/export_to_local.rs`
- [ ] T063 [US5] Tauri command + UI export control + result summary (created / skipped-no-audio) in `ui/src/features/run/`
- [ ] T064 [US5] Emit audit events for export results (created / skipped / failed)

**Checkpoint**: End-to-end v1 complete — likes → organized, tagged crates on disk for Rekordbox.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [ ] T065 [P] Integration test: full slice scan→classify→triage→analyze→export on a small fixture in `tests/integration/pipeline.rs`
- [ ] T066 [P] Integration test: idempotent re-run (new like only; prior decisions preserved; no dupes) in `tests/integration/incremental.rs`
- [ ] T067 [P] Test: audit answerability — "why is this track in this crate?" via `events_for_track` in `tests/integration/audit.rs`
- [ ] T068 [P] Secret-safety test: assert no tokens/secrets in logs or audit detail
- [ ] T069 [P] Performance smoke: 1000-track metadata classify within target; analysis batched + resumable in `tests/integration/perf.rs`
- [ ] T070 [P] Error UX: persistent blocking-error messages; per-track failures surfaced, never fatal (spec edge cases) in `ui/src/shared/`
- [ ] T071 [P] Validate `quickstart.md` scenarios V1–V6 end-to-end; finalize `README.md` + `scripts/check-system-libs.sh`
- [ ] T072 Clean-build gate: `cargo clippy -D warnings`, `cargo fmt --check`, frontend typecheck — zero errors/warnings
- [ ] T073 [P] Classification accuracy eval (measures **SC-001 ≥85%**): run classification over a hand-labeled sample of likes and report auto-precision (share of auto-filed tracks a human does not move) in `tests/integration/classification_accuracy.rs` + a small labeled fixture

---

## Deferred to v2 (NOT in this task set)

**User Story 3 — SoundCloud playlist export** (blocked by gated write API, research R3). When v2 starts:
`PlaylistPublisherPort` adapter (authenticated write, create/fill, dedup), `ExportToSoundCloud` use
case, auth flow, and `export_mode` = SoundCloud/both. The port trait already exists (T013) so it slots
in without redesign.

---

## Dependencies & Execution Order

- **Setup (P1)** → **Foundational (P2)** → user stories. Foundational blocks everything.
- **US1 (P3)** depends only on Foundational — the MVP.
- **US2 (P4)** depends on US1 (needs crates + confidence + triage routing).
- **US4 (P5)** depends on Foundational; integrates with US1's ClassifyTrack (T056). Independent of US2.
- **US5 (P6)** depends on US4 (needs downloaded audio + features to tag/export).
- **Polish (P7)** last.

Story order: US1 → US2 → US4 → US5. (US4 could be built in parallel with US2 by a second track once
Foundational is done — they touch different files — but US5 must follow US4.)

## Parallel Execution Examples

- **Foundational entities**: T007–T012 all `[P]` (separate files).
- **US1 tests**: T025, T026, T027 in parallel before adapters.
- **US1 UI**: T036, T037 in parallel with each other and with backend wiring.
- **US2 UI**: T043 (swipe) and T044 (table) in parallel.
- **US4 tests**: T048, T049, T050 in parallel before adapters.
- **Polish**: T065–T071 all `[P]`.

## Implementation Strategy

- **MVP = Phase 1 + 2 + Phase 3 (US1)** — a working "flat likes → confidence-scored crates" tool,
  no download, no key. Ship/validate this first.
- **Increment 2 = US2** — make it usable end-to-end on metadata alone (triage resolves the tail).
- **Increment 3 = US4 + US5** — add the audio path and the Rekordbox-ready local export (the DJ payoff).
- Each increment is independently testable per its checkpoint and the `quickstart.md` scenarios.

## Task Summary

- **Total**: 82 tasks. Setup 6 · Foundational 18 · US1 14 · US2 14 · US4 15 · US5 6 · Polish 9.
- **Test tasks**: contract + unit + integration throughout (constitution Principle VI).
- **MVP scope**: T001–T038 (Setup + Foundational + US1).
