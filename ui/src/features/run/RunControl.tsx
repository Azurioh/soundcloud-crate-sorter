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
  onLibraryChanged: () => void;
}

/** Run panel: scan a profile, classify all, and show library counts. */
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
      setScanResult(await scan(profileUrl.trim()));
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
              className="min-w-[260px] flex-1 font-mono"
            />
            <Button type="button" onClick={handleScan} disabled={!canScan}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : null}
              {busy ? "Working…" : "Scan"}
            </Button>
            <Button type="button" variant="outline" onClick={handleClassify} disabled={busy}>
              Classify all
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
