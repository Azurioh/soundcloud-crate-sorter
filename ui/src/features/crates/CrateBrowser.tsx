// Crate browsing view (T037): crates with their members, per-track confidence, and an expandable
// audit trail per track ("why is this track in this crate?" — Principle VII / V6).
import { useCallback, useEffect, useState } from "react";
import {
  listCrates,
  trackAudit,
  type AuditEvent,
  type CrateView,
  type TrackView,
} from "../../shared/api";
import { ErrorBanner } from "../../shared/ErrorBanner";
import { toMessage } from "../../shared/errors";

interface CrateBrowserProps {
  // Bumped by the run controls whenever the library changes, to trigger a reload.
  reloadKey: number;
}

export function CrateBrowser({ reloadKey }: CrateBrowserProps) {
  const [crates, setCrates] = useState<CrateView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setCrates(await listCrates());
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload, reloadKey]);

  return (
    <section className="panel">
      <div className="panel__header">
        <h2>Crates</h2>
        <button type="button" onClick={reload} disabled={loading}>
          {loading ? "Loading…" : "Refresh"}
        </button>
      </div>
      <ErrorBanner message={error} onDismiss={() => setError(null)} />

      {crates.length === 0 && !loading ? (
        <p className="empty">No crates yet. Scan a profile and classify to build crates.</p>
      ) : (
        <ul className="crate-list">
          {crates.map((crate) => (
            <CrateCard key={crate.id} crate={crate} />
          ))}
        </ul>
      )}
    </section>
  );
}

function CrateCard({ crate }: { crate: CrateView }) {
  return (
    <li className="crate-card">
      <header className="crate-card__header">
        <h3>{crate.name}</h3>
        <span className="crate-card__count">{crate.members.length} tracks</span>
      </header>
      <div className="table-scroll">
        <table className="track-table">
          <thead>
            <tr>
              <th scope="col">Title</th>
              <th scope="col">Artist</th>
              <th scope="col">Confidence</th>
              <th scope="col">Status</th>
              <th scope="col" aria-label="Audit trail" />
            </tr>
          </thead>
          <tbody>
            {crate.members.map((track) => (
              <TrackRow key={track.id} track={track} />
            ))}
          </tbody>
        </table>
      </div>
    </li>
  );
}

function TrackRow({ track }: { track: TrackView }) {
  const [open, setOpen] = useState(false);
  const [events, setEvents] = useState<AuditEvent[] | null>(null);
  const [loading, setLoading] = useState(false);
  const confidence = track.confidence === null ? "—" : `${Math.round(track.confidence * 100)}%`;

  const toggle = useCallback(async () => {
    if (open) {
      setOpen(false);
      return;
    }
    setOpen(true);
    if (events === null) {
      setLoading(true);
      try {
        setEvents(await trackAudit(track.id));
      } catch {
        setEvents([]);
      } finally {
        setLoading(false);
      }
    }
  }, [open, events, track.id]);

  return (
    <>
      <tr>
        <td>{track.title}</td>
        <td>{track.artist}</td>
        <td>{confidence}</td>
        <td>{track.status.replace(/_/g, " ")}</td>
        <td>
          <button type="button" className="link-button" onClick={toggle} aria-expanded={open}>
            {open ? "Hide" : "Why?"}
          </button>
        </td>
      </tr>
      {open && (
        <tr className="audit-row">
          <td colSpan={5}>
            {loading ? <span className="muted">Loading trail…</span> : <AuditTrail events={events ?? []} />}
          </td>
        </tr>
      )}
    </>
  );
}

function AuditTrail({ events }: { events: AuditEvent[] }) {
  if (events.length === 0) {
    return <span className="muted">No audit events recorded for this track yet.</span>;
  }
  return (
    <ol className="audit-trail">
      {events.map((event, index) => (
        <li key={index}>
          <span className="audit-trail__stage">{event.stage}</span>
          <span className="audit-trail__outcome">
            {event.kind} · {event.outcome}
          </span>
          <span className="audit-trail__detail">
            {Object.entries(event.detail)
              .map(([key, value]) => `${key}=${value}`)
              .join("  ")}
          </span>
        </li>
      ))}
    </ol>
  );
}
