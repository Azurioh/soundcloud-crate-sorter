// Scan / classify run control + run summary (T036). Drives the metadata-only MVP pipeline:
// enter a public profile URL → Scan → Classify all → see counts.
import { useCallback, useEffect, useState } from "react";
import {
  classifyAll,
  runSummary,
  scan,
  type ClassifyResult,
  type RunSummary,
  type ScanResult,
} from "../../shared/api";
import { ErrorBanner } from "../../shared/ErrorBanner";
import { toMessage } from "../../shared/errors";

interface RunControlProps {
  onLibraryChanged: () => void;
}

export function RunControl({ onLibraryChanged }: RunControlProps) {
  const [profileUrl, setProfileUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scanResult, setScanResult] = useState<ScanResult | null>(null);
  const [classifyResult, setClassifyResult] = useState<ClassifyResult | null>(null);
  const [summary, setSummary] = useState<RunSummary | null>(null);

  const refreshSummary = useCallback(async () => {
    try {
      setSummary(await runSummary());
    } catch (e) {
      setError(toMessage(e));
    }
  }, []);

  useEffect(() => {
    void refreshSummary();
  }, [refreshSummary]);

  const handleScan = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setScanResult(await scan(profileUrl));
      await refreshSummary();
      onLibraryChanged();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setBusy(false);
    }
  }, [profileUrl, refreshSummary, onLibraryChanged]);

  const handleClassify = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setClassifyResult(await classifyAll());
      await refreshSummary();
      onLibraryChanged();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setBusy(false);
    }
  }, [refreshSummary, onLibraryChanged]);

  const canScan = profileUrl.trim().length > 0 && !busy;

  return (
    <section className="panel">
      <h2>Run</h2>
      <ErrorBanner message={error} onDismiss={() => setError(null)} />

      <div className="run-form">
        <label htmlFor="profile-url">Public SoundCloud profile URL</label>
        <div className="run-form__row">
          <input
            id="profile-url"
            type="url"
            inputMode="url"
            placeholder="https://soundcloud.com/your-name"
            value={profileUrl}
            onChange={(e) => setProfileUrl(e.target.value)}
            disabled={busy}
          />
          <button type="button" onClick={handleScan} disabled={!canScan}>
            {busy ? "Working…" : "Scan"}
          </button>
          <button type="button" onClick={handleClassify} disabled={busy}>
            Classify all
          </button>
        </div>
      </div>

      {scanResult && (
        <p className="run-result">
          Scanned {scanResult.liked_total} likes — {scanResult.new_tracks} new,{" "}
          {scanResult.duplicates_collapsed} duplicates collapsed, {scanResult.already_in_library}{" "}
          already in library.
        </p>
      )}
      {classifyResult && (
        <p className="run-result">
          Classified — {classifyResult.auto_classified} auto-sorted, {classifyResult.sent_to_triage}{" "}
          to triage, {classifyResult.skipped} skipped.
        </p>
      )}

      {summary && (
        <dl className="summary-grid">
          <SummaryStat label="Tracks" value={summary.total_tracks} />
          <SummaryStat label="Crates" value={summary.crate_count} />
          <SummaryStat label="Auto" value={summary.auto_classified} />
          <SummaryStat label="Triage" value={summary.in_triage} />
          <SummaryStat label="Unclassified" value={summary.scanned} />
          <SummaryStat label="Decided" value={summary.manually_decided} />
        </dl>
      )}
    </section>
  );
}

function SummaryStat({ label, value }: { label: string; value: number }) {
  return (
    <div className="summary-stat">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
