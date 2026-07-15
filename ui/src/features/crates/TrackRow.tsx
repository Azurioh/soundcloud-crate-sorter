import { useCallback, useState } from "react";
import { trackAudit, type AuditEvent, type TrackView } from "@/shared/api";
import { toMessage } from "@/shared/errors";
import { statusToBadge } from "@/shared/display/track-status";
import { confidenceTone } from "@/shared/display/confidence-tone";
import { AudioFeatures } from "@/shared/display/AudioFeatures";
import { ToneBadge } from "@/shared/ui/tone-badge";
import { Button } from "@/shared/ui/button";
import { TableCell, TableRow } from "@/shared/ui/table";
import { AuditTrail } from "@/features/crates/AuditTrail";

/** One track row plus its lazily-loaded, expandable audit trail. */
export function TrackRow({ track }: { track: TrackView }) {
  const [open, setOpen] = useState(false);
  const [events, setEvents] = useState<AuditEvent[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [auditError, setAuditError] = useState<string | null>(null);
  const confidence = confidenceTone(track.confidence);
  const status = statusToBadge(track.status);

  const load = useCallback(async () => {
    setLoading(true);
    setAuditError(null);
    try {
      setEvents(await trackAudit(track.id));
    } catch (e) {
      // Leave events null so a retry re-fetches, rather than caching the failure as "no events".
      setAuditError(toMessage(e));
    } finally {
      setLoading(false);
    }
  }, [track.id]);

  const toggle = useCallback(() => {
    const next = !open;
    setOpen(next);
    if (next && events === null && !loading) {
      void load();
    }
  }, [open, events, loading, load]);

  return (
    <>
      <TableRow className={open ? "bg-muted/50" : undefined}>
        <TableCell className="font-medium">{track.title}</TableCell>
        <TableCell className="text-muted-foreground">{track.artist}</TableCell>
        <TableCell>
          <AudioFeatures track={track} />
        </TableCell>
        <TableCell>
          <ToneBadge tone={confidence.tone} className="font-mono tabular-nums">
            {confidence.label}
          </ToneBadge>
        </TableCell>
        <TableCell>
          <ToneBadge tone={status.tone}>{status.label}</ToneBadge>
        </TableCell>
        <TableCell className="text-right">
          <Button type="button" variant="link" size="sm" onClick={toggle} aria-expanded={open} className="h-auto p-0">
            {open ? "Hide" : "Why?"}
          </Button>
        </TableCell>
      </TableRow>
      {open && (
        <TableRow className="bg-muted/50 hover:bg-muted/50">
          <TableCell colSpan={6}>
            <AuditCell loading={loading} auditError={auditError} events={events} onRetry={() => void load()} />
          </TableCell>
        </TableRow>
      )}
    </>
  );
}

/** Renders the audit cell's loading / error / trail branches (module-level: no nested ternary). */
function AuditCell({
  loading,
  auditError,
  events,
  onRetry,
}: {
  loading: boolean;
  auditError: string | null;
  events: AuditEvent[] | null;
  onRetry: () => void;
}) {
  if (loading) {
    return <span className="text-sm text-muted-foreground">Loading trail…</span>;
  }
  if (auditError !== null) {
    return (
      <span className="text-sm text-destructive" role="alert">
        Could not load the audit trail.{" "}
        <Button type="button" variant="link" size="sm" className="h-auto p-0" onClick={onRetry}>
          Retry
        </Button>
      </span>
    );
  }
  return <AuditTrail events={events ?? []} />;
}
