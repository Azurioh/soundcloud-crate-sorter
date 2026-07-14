# Feature Specification: SoundCloud Crate Sorter

**Feature Branch**: `001-soundcloud-crate-sorter`

**Created**: 2026-07-14

**Status**: Draft

**Input**: User description: "Turn a SoundCloud likes library into organized DJ crates, exportable either to SoundCloud playlists or to local folders ready for Rekordbox/USB. Auto-classify most tracks reliably, route the uncertain ones to a simple, playful manual triage queue."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Import likes and get auto-sorted crates (Priority: P1)

A DJ points the tool at their SoundCloud likes. The tool lists every liked track, removes duplicates, and proposes a crate (genre-based) for each track along with a confidence score. The DJ sees their previously flat pile of likes organized into named crates without any manual effort.

**Why this priority**: This is the core value — turning an unusable flat pile into an organized, browsable structure. Everything else builds on it. On its own it already answers "what's in my likes and how would it group?".

**Independent Test**: Provide a public likes source; run scan + classification; verify each unique track is assigned to exactly one crate with a confidence score, and duplicates are collapsed.

**Acceptance Scenarios**:

1. **Given** a public SoundCloud likes source with hundreds of tracks, **When** the DJ runs a scan, **Then** every unique track is listed with its available metadata (title, artist, genre tag, duration).
2. **Given** a scanned library, **When** classification runs, **Then** each track is assigned to exactly one crate with a confidence score, and crates are created only for genres actually present in the library.
3. **Given** the same track liked twice (or a repost of an original), **When** the library is scanned, **Then** it appears once, not twice.

---

### User Story 2 - Triage uncertain tracks in a playful queue (Priority: P2)

Tracks the tool is not confident about are placed in a manual triage queue instead of being filed automatically. The DJ clears the queue via a fast, playful card interface: preview the track, accept the top suggestion, pick a likely alternative, open a full crate picker, create a new crate on the fly, or defer. A dense table view is available for batch assignment.

**Why this priority**: Keeps control over edge cases without it becoming a chore — the explicit product goal. Depends on P1 producing confidence scores.

**Independent Test**: Feed a set containing low-confidence tracks; verify they appear in the queue (not auto-filed), that each triage action assigns the track to a crate, and that decisions persist across runs.

**Acceptance Scenarios**:

1. **Given** a track with confidence below the threshold, **When** classification completes, **Then** the track appears in the manual triage queue and is not auto-filed.
2. **Given** a track in the queue, **When** the DJ accepts the top suggestion, **Then** the track is assigned to that crate and removed from the queue.
3. **Given** a track in the queue, **When** none of the suggested crates fit, **Then** the DJ can open a searchable picker of all crates or create a new crate and assign the track to it.
4. **Given** a triaged track, **When** the library is re-scanned later, **Then** that track is not presented for triage again.

---

### User Story 3 - Export crates to SoundCloud playlists (Priority: Deferred to v2)

> **Deferred to v2 (scope decision, 2026-07-14)**: writing playlists to SoundCloud requires the official OAuth 2.1 API, whose app registration has been gated/closed since ~2022 and is not realistically obtainable for a personal tool. This story is retained for v2, where a grey-area path (replaying the web client's authenticated `api-v2` calls using the user's own browser session token) may be evaluated. It is **not in v1 scope**. v1 delivers usable output through local/Rekordbox export (User Story 5) instead.

Without downloading anything, the DJ exports their crates back to SoundCloud as playlists — either creating one new playlist per crate or adding tracks to an existing playlist they choose. Tracks already present in the target are skipped so no duplicates are created.

**Why this priority**: Would deliver usable output with zero download, but is blocked in v1 by the SoundCloud write-API gating described above.

**Independent Test**: Given classified crates and an authenticated SoundCloud account, run SoundCloud export; verify playlists are created or filled and contain no duplicate tracks.

**Acceptance Scenarios**:

1. **Given** classified crates and an authenticated account, **When** the DJ exports to SoundCloud choosing "new playlist per crate", **Then** one playlist per crate is created containing that crate's tracks.
2. **Given** an existing SoundCloud playlist selected as target, **When** the DJ exports a crate into it, **Then** only tracks not already in the playlist are added.
3. **Given** the DJ is not authenticated, **When** they attempt a SoundCloud export, **Then** the system requires them to authenticate with their own account before writing.

---

### User Story 4 - Enrich classification with audio analysis (Priority: P3)

The DJ opts in to downloading track audio. The tool then computes BPM, musical key (Camelot), and an energy measure for each track, sharpening the crate proposals and adding an energy role (warmup / groove / peak / closing) as a sub-tag.

**Why this priority**: Meaningfully improves auto-classification quality and unlocks DJ-grade organization, but is optional and heavier (requires download). The tool must remain fully usable without it.

**Independent Test**: With download enabled on a small set, verify each downloaded track gains BPM, key (Camelot), and energy values, and that energy roles refine the crate structure.

**Acceptance Scenarios**:

1. **Given** download is enabled, **When** a track's audio is fetched, **Then** the track gains BPM, Camelot key, and an energy value.
2. **Given** audio-derived features exist, **When** classification runs, **Then** crates gain an energy sub-role and confidence for affected tracks improves.
3. **Given** download is disabled or a specific track cannot be downloaded, **When** classification runs, **Then** the track is still classified from metadata alone and nothing blocks on the missing audio features.

---

### User Story 5 - Export crates to local folders for Rekordbox/USB (Priority: P3)

The DJ exports crates to local folders (one folder per crate) containing the downloaded audio files, with genre, BPM, key, energy, and vibe written into each file's tags, plus a playlist file per crate. The result imports cleanly into DJ software and onto a USB key.

**Why this priority**: The full DJ-grade deliverable, but depends on audio having been downloaded (US4). Highest-effort path, so lowest priority for v1.

**Independent Test**: Given classified crates with downloaded audio, run local export; verify per-crate folders, tagged audio files, and per-crate playlist files are produced and import without rework.

**Acceptance Scenarios**:

1. **Given** classified crates with downloaded audio, **When** the DJ runs local export, **Then** one folder per crate is produced containing that crate's audio files and a playlist file.
2. **Given** exported files, **When** they are inspected, **Then** each carries genre, BPM, key, energy, and vibe in its tags.
3. **Given** a crate contains tracks with no downloaded audio, **When** local export runs, **Then** those tracks are clearly skipped (and reported) rather than exported empty.

---

### Edge Cases

- **Private or empty likes**: the source has no publicly readable likes → the run reports nothing to sort rather than failing silently.
- **Missing genre tag**: a track has no genre metadata → classification relies on AI inference and, if still uncertain, routes the track to manual triage.
- **Non-music uploads**: podcasts, full DJ sets, or very long tracks appear among likes → flagged/segregated so they don't pollute mixing crates.
- **Undownloadable tracks**: deleted, private, or geo-blocked audio → track is kept with metadata-only classification and excluded from local export, with a clear report.
- **Ambiguous tempo**: half-/double-time BPM ambiguity → recorded as uncertain so it can be reviewed rather than silently mis-tagged.
- **Tie between crates**: a track scores equally for two crates → treated as low-confidence and sent to triage.
- **Target playlist already populated**: some tracks already exist in the chosen SoundCloud playlist → those are skipped, only the missing ones are added.
- **Auth expires mid-export**: SoundCloud authentication lapses during a write → export pauses and prompts re-authentication; already-written tracks are not duplicated on resume.
- **Threshold changed after triage**: the DJ moves the confidence threshold after triaging some tracks → prior manual decisions are preserved; only not-yet-decided tracks are re-evaluated.
- **Re-run with new likes**: only newly added likes are processed; previously classified or triaged tracks keep their decisions.

## Requirements *(mandatory)*

### Functional Requirements

**Scanning & source**

- **FR-001**: System MUST list all tracks in a user's SoundCloud likes from a publicly readable profile without requiring the user to log in.
- **FR-002**: System MUST capture, per track, the available metadata: title, artist/uploader, genre tag, duration, artwork reference, permalink, and a stable track identifier.
- **FR-003**: System MUST detect and collapse duplicate tracks within the source (same track liked more than once, or a repost of an original) so each unique track is considered exactly once.
- **FR-004**: System MUST support incremental runs: a re-scan brings in only newly added likes and preserves all prior classification and triage decisions.

**Audio (optional)**

- **FR-005**: System MUST let the user choose whether to download track audio; downloading MUST be opt-in and off by default.
- **FR-006**: When a track's audio is available, System MUST compute its BPM, musical key in Camelot notation, and an energy measure using deterministic audio analysis (never AI-guessed).
- **FR-007**: When audio is unavailable for a track, System MUST classify it from metadata alone and MUST NOT block the run on missing audio-derived features.

**Classification**

- **FR-008**: System MUST assign each track to exactly one crate, where a crate is defined by a genre and, when energy is known, an energy role (warmup / groove / peak / closing).
- **FR-009**: System MUST create crates dynamically from the genres and energies present in the user's own library, without imposing a fixed global taxonomy.
- **FR-010**: System MUST use AI assistance only where it adds value — inferring an ambiguous or missing genre, and inferring vibe/mood — and MUST NOT use AI to produce BPM, key, or energy.
- **FR-011**: System MUST produce a confidence score for every auto-classification.
- **FR-012**: System MUST route any track whose confidence is below an adjustable threshold to the manual triage queue instead of auto-filing it.
- **FR-013**: Users MUST be able to adjust the confidence threshold (aggressive ↔ cautious) and see the resulting auto-versus-manual split before committing.
- **FR-014**: System MUST attach optional vibe/mood tags to tracks; a track MUST be classifiable and exportable without any vibe tag.

**Manual triage**

- **FR-015**: System MUST present uncertain tracks in a card-based triage interface offering audio preview, the top suggested crate, and a small set of likely alternative crates.
- **FR-016**: For each queued track, users MUST be able to accept the top suggestion, choose an alternative, open a searchable picker of all crates, create a new crate on the fly, or defer the track.
- **FR-017**: System MUST also provide a dense table view for fast batch assignment of the same queue.
- **FR-018**: System MUST persist every manual decision so a decided track is never re-presented for triage on later runs.

**Export — SoundCloud** *(Deferred to v2 — blocked by SoundCloud write-API gating; see User Story 3)*

- **FR-019** *(v2)*: Users MUST be able to export crates as SoundCloud playlists, choosing either to create a new playlist per crate or to add a crate's tracks to an existing playlist they select.
- **FR-020** *(v2)*: System MUST require the user to authenticate with their own SoundCloud account before performing any write to SoundCloud.
- **FR-021** *(v2)*: When adding tracks to a playlist, System MUST skip tracks already present so no duplicates are created.

**Export — Local / Rekordbox**

- **FR-022**: Users MUST be able to export crates to local folders, one folder per crate, containing the downloaded audio files.
- **FR-023**: For local export, System MUST write genre, BPM, key, energy, and vibe into each audio file's tags and generate a playlist file per crate.
- **FR-024**: Local export MUST require downloaded audio; tracks without local audio MUST be skipped and reported, not exported empty.
- **FR-025**: Users MUST be able to choose the export destination. In v1 the only destination is local/Rekordbox export; SoundCloud export and the "both" option are enabled in v2 (see FR-019–021).

**General**

- **FR-026**: System MUST operate as a single local user tool with no accounts or multi-user separation.
- **FR-027**: System MUST present a run summary with counts: total tracks, auto-classified, sent to triage, exported, and duplicates skipped.
- **FR-028**: System MUST require explicit user opt-in before downloading audio and surface the ToS/legal consideration that downloading is for personal use.

**Classification refinement & edge handling**

- **FR-029**: When audio analysis later adds an energy role to a track previously filed by genre only, the System MUST refine the track into the matching energy sub-crate **only if it is still auto-classified**; a `ManuallyDecided` track MUST NOT be moved without explicit user confirmation (preserves FR-018 and Principle IV).
- **FR-030**: System MUST detect likely non-music or non-mixable uploads (e.g., by duration/type heuristics — long DJ sets, podcasts) and route them to a dedicated review crate instead of the mixing crates.
- **FR-031**: When tempo detection is ambiguous (half-/double-time), the System MUST mark the track's BPM as uncertain and route it to the manual triage queue rather than silently tagging a possibly-wrong value.

### Key Entities *(include if feature involves data)*

- **Track**: a unique liked item. Attributes: stable identifier, title, artist, genre tag, duration, artwork/permalink, optional audio-derived features (BPM, Camelot key, energy), inferred vibe tags, assigned crate, confidence score, status (auto-classified / in-triage / manually-decided / deferred).
- **Crate**: an organizing bucket. Attributes: name, genre, optional energy role, member tracks.
- **Source**: where tracks come from (v1: the user's likes; future: reposts, listening history, existing playlists).
- **Classification Decision**: how a track reached its crate. Attributes: automatic vs manual, chosen crate, confidence, timestamp.
- **Export Target**: a destination. SoundCloud playlist (new or an existing one selected by the user) or a local folder. Carries a per-track result (created / skipped-as-duplicate / skipped-no-audio / failed).
- **Settings**: confidence threshold, download on/off, export mode (SoundCloud / local / both), SoundCloud authentication state.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a library of hundreds to a few thousand likes, at least 85% of tracks are auto-classified into a crate the DJ accepts without moving it during a review pass.
- **SC-002**: The DJ can decide each track in the manual triage queue in under 10 seconds on average, and clear a typical queue in a single focused session.
- **SC-003**: 100% of exported targets contain zero duplicate tracks (v1: local crate folders; v2: SoundCloud playlists).
- **SC-004**: Exported crates are usable without rework — in v1, local crate folders import into DJ software with 0 corrupted or misplaced entries (v2 adds: SoundCloud playlists appear correctly populated).
- **SC-005**: A first-time user goes from a flat likes library to their first exported, usable crate in under 15 minutes for an initial batch.
- **SC-006**: On a re-run after adding new likes, only the new or not-yet-decided tracks are processed; previously decided tracks are never re-sorted or re-triaged.
- **SC-007**: Total hands-on sorting time is reduced by at least 70% compared with organizing the same library entirely by hand.

## Assumptions

- v1 targets the **likes** source only; reposts, listening history, and existing playlists as *input* sources are future scope (existing playlists are supported as *export* targets, not inputs).
- This is a **personal, single-user, local** tool — no cloud, no multi-user, no accounts beyond the user's own SoundCloud authentication for writing.
- The user's likes are **publicly readable**; private likes are out of scope for v1.
- **Downloading audio is off by default and opt-in.** The user is responsible for the legal/ToS implications; downloading is assumed to be for personal use only.
- "Correctly classified" (SC-001) is measured as the share of auto-filed tracks the DJ does not move during a review pass.
- Musical key uses **Camelot** notation; energy is a normalized scale mapped to the four energy roles (warmup / groove / peak / closing).
- **Rekordbox import is performed by the user** importing the generated files and playlist files; the tool never writes to Rekordbox's own database.
- The genre taxonomy is **derived from the user's own library**, not a fixed global list.
- The default confidence threshold targets roughly an **85/15 auto/manual split** and is adjustable at any time.
- **Reading likes requires no login; writing to SoundCloud requires the user's own account authentication** and is the more fragile path (subject to SoundCloud's terms and interface stability).
- **SoundCloud playlist export (User Story 3, FR-019–021) is deferred to v2.** The official write API requires a gated OAuth app registration that a personal tool cannot realistically obtain. v1 delivers usable output via local/Rekordbox export only; because local export requires downloaded audio, **v1's usable output path requires opting into download** (classification itself still degrades gracefully to metadata-only without download).
- **Key detection uses the libKeyFinder library (GPL-3.0).** The tool is a personal, non-distributed build (Principle II), so GPL is acceptable; this forecloses a future closed-source distribution unless the key-detection adapter is swapped (the port isolates it).
