# Implementation Plan: SoundCloud Crate Sorter

**Branch**: `001-soundcloud-crate-sorter` | **Date**: 2026-07-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-soundcloud-crate-sorter/spec.md`

## Summary

Turn a SoundCloud "likes" library into organized DJ crates. The tool scans a user's public likes
(no login), deduplicates them, and auto-classifies each track into one crate (genre × energy role)
with a confidence score; tracks below an adjustable confidence threshold go to a playful manual
triage UI. Opting into audio download unlocks deterministic BPM / Camelot-key / energy analysis and
the local/Rekordbox export (per-crate folders + ID3-tagged files + M3U8 playlists).

Technical approach: a **Rust** core built as strict clean architecture (traits = ports), packaged as
a **Tauri v2** desktop app with a **React + TypeScript** webview for the triage/preview UI. Key
detection uses **libKeyFinder** (FFI), BPM uses **aubio**, storage + audit trail use **SQLite**
(rusqlite). **SoundCloud playlist write is deferred to v2** (official write API is gated); v1's usable
output is the local/Rekordbox path.

## Technical Context

**Language/Version**: Rust (stable, edition 2021+); TypeScript for the Tauri webview.

**Primary Dependencies**: tauri v2.9.x · reqwest + serde/serde_json · libkeyfinder-sys 0.1.0 (GPL-3.0,
`cxx` FFI) · aubio-rs (native aubio) · symphonia (PCM decode) · lofty (ID3/M3U8) · rusqlite (SQLite) ·
tracing · uuid. Frontend: React + Vite, motion, @use-gesture/react, @tanstack/react-table,
react-hotkeys-hook. External binary: `yt-dlp`. AI: Claude API (`claude-haiku-4-5-20251001`).

**Storage**: Local SQLite file (tracks, crates, classification decisions, audit log). Downloaded audio
+ exported crate folders on the local filesystem.

**Testing**: `cargo test` — use-case unit tests with in-memory port fakes; port contract tests run
against BOTH in-memory and real adapters. Frontend: component tests for the triage interactions.

**Target Platform**: macOS (Apple Silicon) desktop first (Homebrew-provided libKeyFinder/fftw/aubio).
Single local user.

**Project Type**: Desktop app (Tauri) with a Rust core library + React frontend.

**Performance Goals**: Handle hundreds–low thousands of tracks. Scan + metadata classify a
1,000-track library in a few minutes; audio analysis batched in the background with resumable
progress. Triage UI responsive (<100 ms per card action).

**Constraints**: Reading likes = no login (public only); SoundCloud write deferred (v2). Download is
opt-in, off by default; missing audio never blocks a run. Non-destructive & idempotent; dedup
everywhere. GPL-3.0 (libKeyFinder) → personal, non-distributed build.

**Scale/Scope**: v1 = User Stories 1, 2, 4, 5 (scan+classify, triage, audio analysis, local export).
User Story 3 (SoundCloud export) deferred to v2.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Compliance in this plan |
|---|---|
| **I. Reliability over coverage** | Confidence score per track; sub-threshold → triage (FR-011/012). BPM/key/energy deterministic (libKeyFinder/aubio/RMS); AI limited to genre/vibe (R4, R8). ✅ |
| **II. Local-first, single-user** | Tauri desktop, local SQLite, no cloud/accounts. SoundCloud auth would only be for v2 write. ✅ |
| **III. Graceful degradation** | Metadata-only classification works with no download; download opt-in/off by default; missing audio never blocks (R2, R4). ✅ |
| **IV. Non-destructive & idempotent** | Never write Rekordbox DB (export files only); dedup at source + export; incremental runs preserve decisions via SQLite state (R5, R6). ✅ |
| **V. User keeps control** | Explicit opt-in gate before download; triage lets the human override every auto decision; adjustable threshold (FR-013/016/028). ✅ |
| **VI. Clean Architecture (STRICT)** | Traits = ports; entities depend on nothing; use cases depend only on entities + ports; adapters map vendor types at the edge; `[technology]-[concept]` naming; in-memory fakes + contract tests. See [contracts/](./contracts/) and data-model. ✅ |
| **VII. Observability — audit & logs** | `tracing` structured logs per stage; SQLite audit log records every classification (score + reason), triage action, download, dedup skip, export result. ✅ |

**Result**: PASS. No violations to justify — Complexity Tracking left empty. The one licensing note
(GPL-3.0 via libKeyFinder) is a documented, accepted constraint under Principle II, not a violation.

## Project Structure

### Documentation (this feature)

```text
specs/001-soundcloud-crate-sorter/
├── plan.md              # This file
├── spec.md              # Feature spec (US3 deferred to v2)
├── research.md          # Phase 0 decisions
├── data-model.md        # Phase 1 entities
├── quickstart.md        # Phase 1 validation guide
├── contracts/           # Phase 1 port contracts
│   └── ports.md
└── checklists/
    └── requirements.md
```

### Source Code (repository root)

```text
crates/
├── domain/                     # Layer 1 — Entities (depend on nothing)
│   └── src/
│       ├── track.rs            # Track, TrackId
│       ├── crate_.rs           # Crate, CrateId, EnergyRole
│       ├── camelot_key.rs      # CamelotKey (+ libKeyFinder→Camelot table lives in the adapter)
│       ├── confidence.rs       # Confidence, ConfidenceThreshold
│       ├── classification.rs   # ClassificationDecision, DecisionSource
│       └── audit.rs            # AuditEvent (domain-level event types)
│
├── application/                # Layer 2 — Use cases + port traits
│   └── src/
│       ├── ports/              # Trait definitions (the seams)
│       │   ├── likes_source.rs         # LikesSourcePort
│       │   ├── playlist_publisher.rs   # PlaylistPublisherPort (defined, no v1 adapter)
│       │   ├── audio_downloader.rs     # AudioDownloaderPort
│       │   ├── audio_analyzer.rs       # AudioAnalyzerPort
│       │   ├── genre_vibe_classifier.rs# GenreVibeClassifierPort
│       │   ├── tag_writer.rs           # TagWriterPort
│       │   ├── track_repository.rs     # TrackRepository
│       │   ├── crate_repository.rs     # CrateRepository
│       │   ├── audit_log.rs            # AuditLogPort
│       │   ├── clock.rs                # ClockPort
│       │   └── id_provider.rs          # IdProvider
│       └── use_cases/
│           ├── scan_likes.rs
│           ├── deduplicate_library.rs
│           ├── classify_track.rs
│           ├── route_to_triage.rs
│           ├── apply_manual_decision.rs
│           ├── download_audio.rs
│           ├── analyze_audio.rs
│           └── export_to_local.rs      # export_to_soundcloud.rs deferred to v2
│
├── adapters/                   # Layer 3 — Interface adapters (map vendor types at the edge)
│   └── src/
│       ├── internal_api_likes_source.rs        # reqwest + serde → api-v2
│       ├── ytdlp_audio_downloader.rs           # subprocess yt-dlp
│       ├── libkeyfinder_aubio_audio_analyzer.rs# symphonia + libKeyFinder FFI + aubio
│       ├── anthropic_genre_vibe_classifier.rs  # Claude API over HTTP
│       ├── lofty_tag_writer.rs                 # ID3 + M3U8
│       ├── sqlite_track_repository.rs
│       ├── sqlite_crate_repository.rs
│       ├── sqlite_audit_log.rs
│       ├── system_clock_provider.rs
│       └── uuid_id_provider.rs
│
└── app/                        # Layer 4 — Frameworks & drivers (composition root)
    └── src/
        ├── main.rs             # wires adapters → use cases; Tauri setup
        ├── composition_root.rs # the ONLY place that knows concrete types
        └── commands.rs         # #[tauri::command] handlers calling use cases

tests/
├── contract/                   # port contract tests (in-memory AND real adapter)
└── integration/                # end-to-end pipeline slices

ui/                             # React + TypeScript (Tauri webview)
├── src/
│   ├── features/
│   │   ├── triage/             # swipe cards + assignment table
│   │   ├── crates/             # crate browsing
│   │   └── run/                # scan/analyze/export run control + summary
│   ├── shared/
│   └── main.tsx
└── index.html
```

**Structure Decision**: A Rust **Cargo workspace** with one crate per architectural layer
(`domain`, `application`, `adapters`, `app`) makes the Dependency Rule (Principle VI) a
compile-time guarantee: `domain` has no dependencies, `application` depends only on `domain`,
`adapters` depend on `application` + vendor crates, and `app` (the composition root + Tauri) is the
only crate that wires concretes. In-memory port fakes live beside the traits for use-case tests;
real adapters get contract tests. The React app under `ui/` is the Tauri webview and talks to the
core only through `#[tauri::command]` handlers.

## Complexity Tracking

> No constitution violations to justify — section intentionally empty.
