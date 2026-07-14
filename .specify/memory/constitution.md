<!--
SYNC IMPACT REPORT
==================
Version change: (template, unversioned) → 1.0.0
Bump rationale: Initial ratification of the project constitution (MAJOR baseline).

Modified principles: n/a (first ratification)
Added principles:
  I.   Reliability Over Coverage
  II.  Local-First, Personal, Single-User
  III. Graceful Degradation
  IV.  Non-Destructive & Idempotent
  V.   User Keeps Control
  VI.  Clean Architecture (STRICT — the Dependency Rule)
  VII. Observability — Audit & Logs (NON-NEGOTIABLE)
Added sections:
  - Additional Constraints (Data, Access & Legal)
  - Development Workflow & Quality Gates
  - Governance

Templates reviewed for consistency:
  - .specify/templates/plan-template.md .......... ✅ generic Constitution Check gate; no edit required
  - .specify/templates/spec-template.md .......... ✅ aligned; spec.md already reflects principles I–V
  - .specify/templates/tasks-template.md ......... ✅ supports observability/testing task types; no edit required
  - .claude/skills/speckit-*/SKILL.md ............ ✅ generic guidance; no agent-specific references to fix

Deferred TODOs: none.
-->

# SoundCloud Crate Sorter Constitution

A personal, local, single-user tool that turns a SoundCloud "likes" library into organized
DJ crates, exportable to SoundCloud playlists or to local folders ready for Rekordbox/USB.

## Core Principles

### I. Reliability Over Coverage

The system MUST NOT misfile a track silently. When a track's classification confidence falls
below the adjustable threshold, it MUST be routed to the manual triage queue rather than
auto-filed. Audio-derived measures (BPM, Camelot key, energy) MUST be produced by deterministic
audio analysis and MUST NEVER be AI-guessed. AI assistance is permitted ONLY where it adds
value that deterministic signals cannot provide: inferring an ambiguous or missing genre, and
inferring vibe/mood.

**Rationale**: A wrong-but-confident crate assignment costs the DJ more than an honest "I'm not
sure" — trust in the auto-sort collapses the first time a track is silently misplaced.

### II. Local-First, Personal, Single-User

The tool MUST run as a single local user's tool: no cloud, no accounts, no multi-user
separation. The ONLY external authentication is the user's own SoundCloud account, and ONLY when
writing (creating or filling playlists). Reading publicly readable likes MUST require no login.

**Rationale**: Scope discipline. The problem is one DJ organizing their own pile; every cloud or
multi-user affordance is unbounded complexity that does not serve that goal.

### III. Graceful Degradation

Every capability MUST work with less. No audio downloaded → classification proceeds on metadata
alone. No SoundCloud authentication → local export MUST still be available. Downloading audio
MUST be opt-in and OFF by default. Missing audio-derived features MUST NEVER block a run.

**Rationale**: The lightest path (read public likes, classify on metadata, export to SoundCloud)
must always be usable; richer paths are additive, never gatekeepers.

### IV. Non-Destructive & Idempotent

The system MUST NEVER write to Rekordbox's own database — it only produces importable files and
playlists. Deduplication MUST apply everywhere: source likes (same track liked twice, reposts)
and target playlists. Re-runs MUST be incremental and MUST preserve all prior classification and
manual decisions. Exports MUST skip tracks already present in the target.

**Rationale**: The user's library and DJ software are irreplaceable state. Running the tool
twice must be safe and must never undo a human decision or corrupt an external system.

### V. User Keeps Control

The system MUST obtain explicit user opt-in before downloading audio, surfacing the
terms-of-service / personal-use consideration at that moment. A human decision MUST always
override a machine decision. Resolving edge cases MUST remain quick and low-friction — never a
chore.

**Rationale**: The product exists to keep the DJ in control of the limit cases; automation
serves the human, and the human is the final authority.

### VI. Clean Architecture (STRICT — the Dependency Rule)

Source-code dependencies MUST point inward only. Four layers:

- **Entities** — `Track`, `Crate`, `CamelotKey`, `EnergyRole`, `Confidence`,
  `ClassificationDecision`. Depend on nothing.
- **Use cases** — `ScanLikes`, `DeduplicateLibrary`, `DownloadAudio`, `AnalyzeAudio`,
  `ClassifyTrack`, `RouteToTriage`, `ApplyManualDecision`, `ExportToSoundCloud`, `ExportToLocal`.
  Depend ONLY on entities and ports.
- **Interface adapters** — implement the ports; map vendor/SDK/DB types to domain types AT THE
  EDGE.
- **Frameworks & drivers** — HTTP client, download binary, audio-analysis library, AI SDK,
  filesystem, database, UI toolkit. Glue only, wired at the composition root.

Ports MUST be declared by the inner layer (the use case declares the need in its own vocabulary;
an adapter implements it; injection happens at the composition root). Required ports:
`LikesSourcePort` (SoundCloud read), `PlaylistPublisherPort` (SoundCloud authenticated write),
`AudioDownloaderPort`, `AudioAnalyzerPort`, `GenreVibeClassifierPort` (AI), `TagWriterPort`
(ID3 tags + M3U8), `TrackRepository` and `CrateRepository` (persistence), `ClockPort`,
`IdProvider`, `AuditLogPort`.

Hard prohibitions:

- No vendor/SDK/DB type may cross inward into a use case or an entity.
- No `new Date()` or randomness outside the dedicated Clock / Id providers.
- Adapter files and classes MUST be named `[technology]-[concept]` (e.g.
  `internal-api-likes.source`, `ytdlp-audio.downloader`).

Testability: every use case MUST be unit-testable in isolation using in-memory fakes of ALL its
ports (no network, no filesystem, no AI, no database). Repository/port contract tests MUST run
against BOTH the in-memory adapter and the real adapter.

**Rationale**: SoundCloud's unofficial interface, the download tool, and the audio/AI libraries
are the most volatile parts of the system. Isolating them behind ports means they can be swapped
or repaired without touching the rules that define what the app does — and everything stays
testable and safe to refactor.

### VII. Observability — Audit & Logs (NON-NEGOTIABLE)

The system MUST emit structured logs at every pipeline stage, carrying at minimum the run id,
track id, stage, and outcome. Print-style debugging is forbidden; log levels MUST be used; auth
tokens and secrets MUST NEVER appear in logs.

The system MUST maintain an audit trail of every decision:

- classification — confidence score plus the reason (e.g. genre from SoundCloud tag vs AI
  inference);
- manual triage action — track, from-crate → to-crate, timestamp;
- download result; deduplication skip; export result (created / skipped-duplicate /
  skipped-no-audio / failed).

"Why is this track in this crate?" MUST be answerable from the audit trail. The trail MUST be
stored locally, be queryable, and persist across runs (supporting Principle IV).

**Rationale**: An automated classifier the user cannot interrogate is a black box they will stop
trusting. A durable, queryable audit trail turns every surprising result into an explainable one
and makes debugging tractable.

## Additional Constraints (Data, Access & Legal)

- **Reading vs. writing**: reading public likes requires no login; writing to SoundCloud
  requires the user's own authentication and is the more fragile, terms-of-service-sensitive
  path. The system MUST treat write failures and expired auth as recoverable, non-duplicating
  events.
- **Personal use**: audio download is for the user's personal use; the opt-in gate (Principle V)
  is the point where this is surfaced.
- **Genre taxonomy** is derived from the user's own library, never a fixed global list.
- **Key notation** is Camelot; **energy** is a normalized scale mapped to the roles
  warmup / groove / peak / closing.

## Development Workflow & Quality Gates

- **Spec-driven**: features flow through spec → plan → tasks → implement. Plans MUST include a
  Constitution Check that verifies compliance with Principles I–VII before implementation.
- **Test discipline**: use cases ship with isolation unit tests; ports ship with contract tests
  run against in-memory AND real adapters (Principle VI). A build is not "done" with failing
  tests or unresolved warnings.
- **Observability is a deliverable, not an afterthought**: any new pipeline stage or decision
  point MUST add its structured logging and audit-trail entries in the same change (Principle
  VII).
- **Graceful-degradation checks**: any new capability MUST define its behavior when its
  precondition (audio, auth, network) is absent (Principle III).

## Governance

This constitution supersedes ad-hoc practices. All plan, tasks, and implementation work MUST
comply; deviations MUST be justified in writing or the work MUST be brought into compliance.

Amendment procedure: proposed changes are recorded with rationale, versioned per semantic
versioning, and propagated to dependent templates (plan, spec, tasks) in the same change.

Versioning policy:

- **MAJOR** — backward-incompatible governance/principle removal or redefinition.
- **MINOR** — a new principle/section or materially expanded guidance.
- **PATCH** — clarifications, wording, and non-semantic refinements.

Compliance review: each plan's Constitution Check is the enforcement point; reviewers verify the
seven principles are honored before code is written and before work is called complete.

**Version**: 1.0.0 | **Ratified**: 2026-07-14 | **Last Amended**: 2026-07-14
