import type { RunSummary } from "@/shared/api";

/** Ordered stat tiles derived from a run summary. Keys index into RunSummary. */
const STATS = [
  { key: "total_tracks", label: "Tracks" },
  { key: "crate_count", label: "Crates" },
  { key: "auto_classified", label: "Auto" },
  { key: "in_triage", label: "Triage" },
  { key: "scanned", label: "Unclassified" },
  { key: "manually_decided", label: "Decided" },
] as const satisfies ReadonlyArray<{ key: keyof RunSummary; label: string }>;

/**
 * Renders the library summary as a responsive grid of stat tiles.
 * @param props.summary - the current run summary counts
 */
export function SummaryStats({ summary }: { summary: RunSummary }) {
  return (
    <dl className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
      {STATS.map((stat) => (
        <div key={stat.key} className="rounded-md border bg-card p-3 text-center">
          <dt className="text-xs uppercase tracking-wide text-muted-foreground">{stat.label}</dt>
          <dd className="mt-1 font-mono text-2xl font-bold tabular-nums">{summary[stat.key]}</dd>
        </div>
      ))}
    </dl>
  );
}
