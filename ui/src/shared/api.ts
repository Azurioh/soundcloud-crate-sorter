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
