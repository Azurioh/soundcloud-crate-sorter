import type { AuditEvent } from "@/shared/api";

/** Renders a track's audit events as a compact timeline ("why is this track here?"). */
export function AuditTrail({ events }: { events: AuditEvent[] }) {
  if (events.length === 0) {
    return <span className="text-sm text-muted-foreground">No audit events recorded for this track yet.</span>;
  }
  return (
    <ol className="flex flex-col gap-2 py-1">
      {events.map((event, index) => (
        <li key={index} className="flex flex-wrap items-baseline gap-2 text-sm">
          <span className="text-xs font-bold uppercase tracking-wide text-primary">{event.stage}</span>
          <span className="text-foreground">
            {event.kind} · {event.outcome}
          </span>
          <span className="font-mono text-xs text-muted-foreground">
            {Object.entries(event.detail)
              .map(([key, value]) => `${key}=${value}`)
              .join("  ")}
          </span>
        </li>
      ))}
    </ol>
  );
}
