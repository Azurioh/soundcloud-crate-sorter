# Data Model: SoundCloud Crate Sorter

**Phase 1 output.** Domain entities (Rust `crates/domain`) and their persistent shape (SQLite,
`crates/adapters/sqlite_*`). Domain types are vendor-free; the SQLite columns below are the adapter's
mapping, not the domain definition.

## Entities

### Track

A unique liked item, enriched as the pipeline progresses.

| Field | Type | Notes |
|---|---|---|
| `id` | `TrackId` (uuid) | Internal id from `IdProvider`. |
| `source_track_id` | string | SoundCloud's stable track id — the dedup key. |
| `title` | string | From metadata. |
| `artist` | string | Uploader / `user.username`. |
| `source_genre` | string? | SoundCloud genre tag; may be absent. |
| `duration_ms` | u64 | From metadata. |
| `permalink_url` | string | Used for download. |
| `artwork_url` | string? | Optional. |
| `bpm` | u16? | Present only after audio analysis (deterministic). |
| `camelot_key` | `CamelotKey`? | Present only after audio analysis. |
| `energy` | `Energy` (0–100)? | Present only after audio analysis. |
| `vibe_tags` | list\<string\> | Optional, AI-inferred; never required. |
| `crate_id` | `CrateId`? | Assigned crate (null while unclassified/in-triage). |
| `confidence` | `Confidence` (0.0–1.0)? | Latest auto-classification score. |
| `status` | `TrackStatus` | See state machine below. |
| `local_audio_path` | path? | Set once downloaded; required for local export. |

**Validation**: `source_track_id` unique across the library (dedup). `bpm`/`camelot_key`/`energy`
MUST be produced by deterministic analysis, never AI (Principle I). `energy` normalized 0–100.

### Crate

An organizing bucket = genre × optional energy role.

| Field | Type | Notes |
|---|---|---|
| `id` | `CrateId` (uuid) | |
| `name` | string | Display name (e.g. "Deep House · Peak"). |
| `genre` | string | Primary axis. |
| `energy_role` | `EnergyRole`? | warmup / groove / peak / closing. |
| `created_by` | `CrateOrigin` | `auto` (discovered from library) or `manual` (created in triage). |

**Validation**: crates are created dynamically from genres/energies present in the library
(FR-009) — no fixed global taxonomy. `(genre, energy_role)` unique.

### ClassificationDecision

How and why a track reached its crate — the auditable heart (Principle VII).

| Field | Type | Notes |
|---|---|---|
| `id` | uuid | |
| `track_id` | `TrackId` | |
| `crate_id` | `CrateId` | Chosen crate. |
| `source` | `DecisionSource` | `auto` or `manual`. |
| `confidence` | `Confidence`? | For auto decisions. |
| `reason` | `ClassificationReason` | e.g. `GenreFromSourceTag`, `GenreFromAi`, `AudioFeatures`, `ManualPick`. |
| `decided_at` | timestamp | From `ClockPort`. |

### AuditEvent

Durable, queryable trail; superset of decisions plus operational outcomes.

| Field | Type | Notes |
|---|---|---|
| `id` | uuid | |
| `run_id` | uuid | Correlates one pipeline run. |
| `track_id` | `TrackId`? | Null for run-level events. |
| `stage` | `PipelineStage` | scan / dedup / download / analyze / classify / triage / export. |
| `kind` | `AuditKind` | classification, triage_action, download_result, dedup_skip, export_result. |
| `outcome` | string | e.g. `created`, `skipped_duplicate`, `skipped_no_audio`, `failed`. |
| `detail` | json | Structured, secret-free (never tokens). For triage: `from_crate` → `to_crate`. |
| `occurred_at` | timestamp | From `ClockPort`. |

### Settings (single row / app state)

| Field | Type | Notes |
|---|---|---|
| `confidence_threshold` | `ConfidenceThreshold` (0.0–1.0) | Default targets ~85/15 auto/manual split. |
| `download_enabled` | bool | Default **false** (opt-in, Principle III/V). |
| `export_mode` | `ExportMode` | v1: `local` only (SoundCloud/both = v2). |

### Value objects

- **CamelotKey**: wheel position 1–12 + letter A/B. Built by the analyzer adapter from libKeyFinder's
  `key_t` via the static table in research.md.
- **EnergyRole**: `Warmup | Groove | Peak | Closing`.
- **Confidence** / **ConfidenceThreshold**: `f32` in [0.0, 1.0], newtype-guarded.
- **TrackStatus**: `Scanned | AutoClassified | InTriage | ManuallyDecided | Deferred`.

## State machine — TrackStatus

```text
Scanned ──classify──> AutoClassified        (confidence >= threshold)
Scanned ──classify──> InTriage              (confidence < threshold, or tie/ambiguous)
InTriage ──accept/pick/create──> ManuallyDecided
InTriage ──defer──> Deferred
Deferred ──(next triage session)──> InTriage
AutoClassified ──user override──> ManuallyDecided
```

Once `ManuallyDecided`, a re-scan MUST NOT re-present the track (FR-018, Principle IV). Threshold
changes re-evaluate only tracks not yet `ManuallyDecided`.

## Relationships

- `Track *→1 Crate` (a track belongs to at most one crate).
- `Crate 1→* Track`.
- `Track 1→* ClassificationDecision` (history; latest is authoritative).
- `AuditEvent *→1 run`, optionally `*→1 Track`.

## Persistence notes (SQLite / rusqlite)

- Tables: `tracks`, `crates`, `classification_decisions`, `audit_events`, `settings`.
- `tracks.source_track_id` UNIQUE (dedup + idempotent incremental scans).
- `audit_events` indexed by `run_id` and `track_id` so "why is this track in this crate?" is a simple
  query (Principle VII).
- Value objects serialize to primitive columns; the domain never imports rusqlite types (mapping in
  the sqlite adapter only).
