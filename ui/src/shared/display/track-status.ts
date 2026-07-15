import type { Tone } from "@/shared/ui/tone-badge";

/** Track statuses emitted by the core (mirrors the `status` string on TrackView). */
export type KnownStatus =
  | "auto_classified"
  | "in_triage"
  | "scanned"
  | "manually_decided"
  | "deferred";

interface StatusBadge {
  label: string;
  tone: Tone;
}

/**
 * Presentation for every known status. `satisfies` makes an unhandled
 * status a compile error rather than a silent gap.
 */
const STATUS_BADGES = {
  auto_classified: { label: "Auto", tone: "success" },
  in_triage: { label: "Triage", tone: "warning" },
  scanned: { label: "Scanned", tone: "neutral" },
  manually_decided: { label: "Decided", tone: "info" },
  deferred: { label: "Deferred", tone: "neutral" },
} satisfies Record<KnownStatus, StatusBadge>;

/** Humanizes an unknown status token for display (never throws — this is presentation). */
function humanize(status: string): string {
  return status.replace(/_/g, " ");
}

/**
 * Maps a track status string to its badge label and tone.
 * @param status - the raw status string from the core
 * @returns label + semantic tone; unknown statuses render neutral + humanized
 */
export function statusToBadge(status: string): StatusBadge {
  if (status in STATUS_BADGES) {
    return STATUS_BADGES[status as KnownStatus];
  }
  return { label: humanize(status), tone: "neutral" };
}
