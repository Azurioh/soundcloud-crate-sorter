// Typed wrappers over the Tauri command bridge. Components depend on these functions, never on
// `invoke` directly — the one canonical path to the Rust core (matches the repository pattern).
//
// Argument keys are camelCase: Tauri v2's command macro defaults to `rename_all = "camelCase"`, so
// a Rust parameter `profile_url` is bound from the IPC key `profileUrl`. Sending snake_case keys
// would leave the argument unbound and the command would reject before running.
import { invoke } from "@tauri-apps/api/core";

export interface ScanResult {
  liked_total: number;
  duplicates_collapsed: number;
  new_tracks: number;
  already_in_library: number;
}

export interface ClassifyResult {
  auto_classified: number;
  sent_to_triage: number;
  skipped: number;
}

export interface TrackView {
  id: string;
  title: string;
  artist: string;
  source_genre: string | null;
  confidence: number | null;
  status: string;
  bpm: number | null;
  camelot_key: string | null;
  energy: number | null;
}

export interface CrateView {
  id: string;
  name: string;
  genre: string;
  energy_role: string | null;
  members: TrackView[];
}

export interface RunSummary {
  total_tracks: number;
  scanned: number;
  auto_classified: number;
  in_triage: number;
  manually_decided: number;
  deferred: number;
  crate_count: number;
}

export interface AuditEvent {
  stage: string;
  kind: string;
  outcome: string;
  detail: Record<string, string>;
  occurred_at: number;
}

/** A crate as an assignable option (the triage picker and resolved suggestions). */
export interface CrateOption {
  id: string;
  name: string;
}

/**
 * A runner-up genre on a triage card. Carries a genre, not a crate id — the crate is created only
 * if the user picks the chip, so a merely-suggested genre never becomes an empty crate.
 */
export interface GenreSuggestion {
  genre: string;
  confidence: number;
}

/** One card in the triage queue: the track, its preview, the suggestion, and the alternatives. */
export interface TriageCard {
  track: TrackView;
  permalink_url: string;
  artwork_url: string | null;
  suggestion: CrateOption | null;
  alternatives: GenreSuggestion[];
}

/** The outcome of one triage action. */
export interface TriageResult {
  status: string;
  crate_id: string | null;
}

/** How a candidate threshold would divide the library. */
export interface ThresholdSplit {
  auto: number;
  manual: number;
  preserved: number;
}

/** The user-adjustable settings. */
export interface Settings {
  confidence_threshold: number;
  download_enabled: boolean;
}

/**
 * A triage action, mirroring the Rust `TriageActionInput` tagged enum. The `kind` discriminant is
 * what serde matches on, so these strings are a wire contract, not a display concern.
 */
export type TriageAction =
  | { kind: "accept_suggestion" }
  | { kind: "assign_to_crate"; crate_id: string }
  | { kind: "create_crate"; genre: string }
  | { kind: "defer" };

/** Scans a public SoundCloud profile's likes into the library. */
export function scan(profileUrl: string): Promise<ScanResult> {
  return invoke<ScanResult>("scan", { profileUrl });
}

/** Classifies and routes every scanned track. */
export function classifyAll(): Promise<ClassifyResult> {
  return invoke<ClassifyResult>("classify_all");
}

/** Lists crates with their member tracks. */
export function listCrates(): Promise<CrateView[]> {
  return invoke<CrateView[]>("list_crates");
}

/** Returns library counts by status plus the crate count. */
export function runSummary(): Promise<RunSummary> {
  return invoke<RunSummary>("run_summary");
}

/** Returns a track's audit trail ("why is this track in this crate?"). */
export function trackAudit(trackId: string): Promise<AuditEvent[]> {
  return invoke<AuditEvent[]>("track_audit", { trackId });
}

/** Lists the manual triage queue, each card with its suggestion and alternatives. */
export function listTriageQueue(): Promise<TriageCard[]> {
  return invoke<TriageCard[]>("list_triage_queue");
}

/** Applies one triage action to one track. */
export function applyTriageAction(trackId: string, action: TriageAction): Promise<TriageResult> {
  return invoke<TriageResult>("apply_triage_action", { trackId, action });
}

/** Lists every crate as an assignable option (the triage picker). */
export function listCrateOptions(): Promise<CrateOption[]> {
  return invoke<CrateOption[]>("list_crate_options");
}

/** Returns how many tracks are deferred, awaiting a later triage session. */
export function countDeferred(): Promise<number> {
  return invoke<number>("count_deferred");
}

/** Puts every deferred track back on the triage queue; resolves to how many returned. */
export function resumeDeferred(): Promise<number> {
  return invoke<number>("resume_deferred");
}

/** Returns the current settings (the threshold the slider starts from). */
export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/** Previews the auto-versus-manual split for a candidate threshold, committing nothing. */
export function previewThreshold(threshold: number): Promise<ThresholdSplit> {
  return invoke<ThresholdSplit>("preview_threshold", { threshold });
}

/** Commits a threshold and re-routes the library, preserving manual decisions. */
export function updateThreshold(threshold: number): Promise<ThresholdSplit> {
  return invoke<ThresholdSplit>("update_threshold", { threshold });
}
