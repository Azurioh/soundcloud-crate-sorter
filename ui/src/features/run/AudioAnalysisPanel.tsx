// Opt-in audio download gate + analysis run (T057, US4).
//
// The gate is the UI half of constitution Principle V: downloading is off by default and the
// terms-of-service / personal-use consideration is surfaced *at the moment of opting in*, not buried
// in a settings page the user never opens. Enabling therefore takes two deliberate steps — reveal
// the notice, then confirm — rather than one stray click on a toggle.
//
// The gate here is a courtesy, not the enforcement: `DownloadAudio` re-reads the setting on every
// track, so the rule holds even if this component is bypassed entirely.
import { useCallback, useEffect, useState } from "react";
import { AudioLines, Loader2, ShieldAlert } from "lucide-react";
import {
  analyzeLibrary,
  getSettings,
  setDownloadEnabled,
  type AnalyzeResult,
} from "@/shared/api";
import { ErrorBanner } from "@/shared/ErrorBanner";
import { toMessage } from "@/shared/errors";
import { Alert, AlertDescription, AlertTitle } from "@/shared/ui/alert";
import { Button } from "@/shared/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/ui/card";

interface AudioAnalysisPanelProps {
  /** Bumped whenever anything changed the library, so the gate's state does not go stale. */
  reloadKey: number;
  onLibraryChanged: () => void;
}

/** Ordered result tiles. Keys index into AnalyzeResult. */
const RESULT_STATS = [
  { key: "downloaded", label: "Downloaded" },
  { key: "analyzed", label: "Analyzed" },
  { key: "refined", label: "Refined" },
  { key: "sent_to_triage", label: "To triage" },
  { key: "preserved_manual", label: "Kept" },
  { key: "skipped", label: "Skipped" },
] as const satisfies ReadonlyArray<{ key: keyof AnalyzeResult; label: string }>;

/**
 * Audio panel: the opt-in download gate and the analysis run.
 * @param props.reloadKey - re-reads the setting when another panel changed the library
 * @param props.onLibraryChanged - announces that a run changed the library
 */
export function AudioAnalysisPanel({ reloadKey, onLibraryChanged }: AudioAnalysisPanelProps) {
  const [downloadEnabled, setEnabled] = useState(false);
  const [downloadDir, setDownloadDir] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [analyzing, setAnalyzing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<AnalyzeResult | null>(null);

  const refreshSettings = useCallback(async () => {
    try {
      const settings = await getSettings();
      setEnabled(settings.download_enabled);
      setDownloadDir(settings.download_dir);
    } catch (e) {
      setError(toMessage(e));
    }
  }, []);

  useEffect(() => {
    void refreshSettings();
  }, [refreshSettings, reloadKey]);

  const applyDownloadEnabled = useCallback(async (enabled: boolean) => {
    setError(null);
    try {
      const settings = await setDownloadEnabled(enabled);
      setEnabled(settings.download_enabled);
      setConfirming(false);
    } catch (e) {
      setError(toMessage(e));
    }
  }, []);

  const handleAnalyze = useCallback(async () => {
    setAnalyzing(true);
    setError(null);
    try {
      setResult(await analyzeLibrary());
      onLibraryChanged();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setAnalyzing(false);
    }
  }, [onLibraryChanged]);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <AudioLines className="size-4" aria-hidden="true" />
          Audio analysis
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ErrorBanner message={error} onDismiss={() => setError(null)} />

        <p className="text-sm text-muted-foreground">
          Optional. Downloads each track&apos;s audio so BPM, musical key and energy can be measured,
          which sorts crates by energy role. Everything works without it — tracks just stay sorted by
          genre alone.
        </p>

        {!downloadEnabled && !confirming && (
          <Button type="button" className="mt-4" onClick={() => setConfirming(true)}>
            Enable audio download
          </Button>
        )}

        {/* The notice is the gate: it appears only when the user reaches for the switch, and
            enabling requires acting on it. */}
        {!downloadEnabled && confirming && (
          <Alert className="mt-4">
            <ShieldAlert aria-hidden="true" />
            <AlertTitle>Before you turn this on</AlertTitle>
            <AlertDescription>
              <p>
                Downloading audio is for your personal use only, and may be against SoundCloud&apos;s
                terms of service. You are responsible for how you use it.
              </p>
              <p>
                Audio will be written to{" "}
                <code className="font-mono text-xs break-all text-foreground">{downloadDir}</code>.
              </p>
              <div className="mt-2 flex flex-wrap gap-2">
                <Button type="button" onClick={() => void applyDownloadEnabled(true)}>
                  I understand — enable
                </Button>
                <Button type="button" variant="ghost" onClick={() => setConfirming(false)}>
                  Cancel
                </Button>
              </div>
            </AlertDescription>
          </Alert>
        )}

        {downloadEnabled && (
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <Button type="button" onClick={handleAnalyze} disabled={analyzing}>
              {analyzing ? <Loader2 className="size-4 animate-spin" aria-hidden="true" /> : null}
              {analyzing ? "Analyzing…" : "Analyze audio"}
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={() => void applyDownloadEnabled(false)}
              disabled={analyzing}
            >
              Turn off downloading
            </Button>
          </div>
        )}

        {downloadEnabled && (
          <p className="mt-2 text-xs text-muted-foreground">
            Downloading to{" "}
            <code className="font-mono break-all">{downloadDir}</code>. Analysis picks up where it
            left off, so it is safe to stop and re-run.
          </p>
        )}

        {analyzing && (
          // Analysis is minutes of native work with no per-track callback to report, so an honest
          // "this is running and will take a while" beats a progress bar that would have to invent
          // its own percentage.
          <p className="mt-3 text-sm text-muted-foreground" role="status">
            Downloading and analyzing tracks. This can take a while on a large library — you can
            leave it running.
          </p>
        )}

        {result?.halted_reason && (
          // A halt has exactly one cause and one fix; say both rather than report "0 analyzed".
          <Alert variant="destructive" className="mt-3">
            <ShieldAlert aria-hidden="true" />
            <AlertTitle>Analysis stopped</AlertTitle>
            <AlertDescription>{result.halted_reason}</AlertDescription>
          </Alert>
        )}

        {result && !result.halted_reason && (
          <dl className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
            {RESULT_STATS.map((stat) => (
              <div key={stat.key} className="rounded-md border bg-card p-3 text-center">
                <dt className="text-xs uppercase tracking-wide text-muted-foreground">
                  {stat.label}
                </dt>
                <dd className="mt-1 font-mono text-2xl font-bold tabular-nums">
                  {result[stat.key]}
                </dd>
              </div>
            ))}
          </dl>
        )}
      </CardContent>
    </Card>
  );
}
