// Scan / classify run control + run summary (T036). Drives the metadata-only MVP pipeline:
// enter a public profile URL → Scan → Classify all → see counts.
import { useCallback, useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import {
  classifyAll,
  runSummary,
  scan,
  type ClassifyResult,
  type RunSummary,
  type ScanResult,
} from "@/shared/api";
import { ErrorBanner } from "@/shared/ErrorBanner";
import { toMessage } from "@/shared/errors";
import { Button } from "@/shared/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/ui/card";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { SummaryStats } from "@/features/run/SummaryStats";

interface RunControlProps {
  /** Bumped whenever anything changed the library, so the counts do not go stale. */
  reloadKey: number;
  onLibraryChanged: () => void;
}

/** Which long-running action is in flight, if any — drives per-button spinner + shared disable state. */
type RunAction = "scan" | "classify";

/**
 * Run panel: scan a profile, classify all, and show library counts.
 * @param props.reloadKey - re-reads the counts when another panel changed the library
 * @param props.onLibraryChanged - announces that a run changed the library
 */
export function RunControl({ reloadKey, onLibraryChanged }: RunControlProps) {
  const [profileUrl, setProfileUrl] = useState("");
  const [running, setRunning] = useState<RunAction | null>(null);
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

  // The counts describe the whole library, so a triage decision elsewhere invalidates them just as
  // much as a scan does — without `reloadKey` the summary keeps reporting the tracks it saw at
  // mount, contradicting the triage panel right below it.
  useEffect(() => {
    void refreshSummary();
  }, [refreshSummary, reloadKey]);

  const handleScan = useCallback(async () => {
    setRunning("scan");
    setError(null);
    try {
      setScanResult(await scan(profileUrl.trim()));
      await refreshSummary();
      onLibraryChanged();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setRunning(null);
    }
  }, [profileUrl, refreshSummary, onLibraryChanged]);

  const handleClassify = useCallback(async () => {
    setRunning("classify");
    setError(null);
    try {
      setClassifyResult(await classifyAll());
      await refreshSummary();
      onLibraryChanged();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setRunning(null);
    }
  }, [refreshSummary, onLibraryChanged]);

  const busy = running !== null;
  const canScan = profileUrl.trim().length > 0 && !busy;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Run</CardTitle>
      </CardHeader>
      <CardContent>
        <ErrorBanner message={error} onDismiss={() => setError(null)} />

        <div className="space-y-2">
          <Label htmlFor="profile-url">Public SoundCloud profile URL</Label>
          <div className="flex flex-wrap gap-2">
            <Input
              id="profile-url"
              type="url"
              inputMode="url"
              placeholder="https://soundcloud.com/your-name"
              value={profileUrl}
              onChange={(e) => setProfileUrl(e.target.value)}
              disabled={busy}
              // min-w keeps a typical SoundCloud profile URL readable before the row wraps.
              className="min-w-[260px] flex-1 font-mono"
            />
            <Button type="button" onClick={handleScan} disabled={!canScan}>
              {running === "scan" ? <Loader2 className="size-4 animate-spin" /> : null}
              {running === "scan" ? "Working…" : "Scan"}
            </Button>
            <Button type="button" variant="outline" onClick={handleClassify} disabled={busy}>
              {running === "classify" ? <Loader2 className="size-4 animate-spin" /> : null}
              {running === "classify" ? "Working…" : "Classify all"}
            </Button>
          </div>
        </div>

        {scanResult && (
          <p className="mt-3 text-sm text-muted-foreground">
            Scanned {scanResult.liked_total} likes — {scanResult.new_tracks} new,{" "}
            {scanResult.duplicates_collapsed} duplicates collapsed, {scanResult.already_in_library}{" "}
            already in library.
          </p>
        )}
        {classifyResult && (
          <p className="mt-3 text-sm text-muted-foreground">
            Classified — {classifyResult.auto_classified} auto-sorted, {classifyResult.sent_to_triage}{" "}
            to triage, {classifyResult.skipped} skipped.
          </p>
        )}

        {summary && <SummaryStats summary={summary} />}
      </CardContent>
    </Card>
  );
}
