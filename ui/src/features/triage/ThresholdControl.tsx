// The confidence threshold slider with its live auto-vs-manual split preview (FR-013).
//
// The split is previewed on every drag but committed only on demand: FR-013 requires the user to
// see the consequence *before* accepting it, and committing per drag frame would re-route the whole
// library dozens of times on one gesture.
import { useCallback, useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import {
  getSettings,
  previewThreshold,
  updateThreshold,
  type ThresholdSplit,
} from "@/shared/api";
import { toMessage } from "@/shared/errors";
import { Button } from "@/shared/ui/button";
import { Label } from "@/shared/ui/label";

/** Slider bounds and step, as percentages (the core takes a 0..1 fraction). */
const PERCENT_MIN = 0;
const PERCENT_MAX = 100;
const PERCENT_STEP = 5;
const PERCENT = 100;

interface ThresholdControlProps {
  /** Bumped when a scan/classify changed the library, so the split is recomputed against it. */
  reloadKey: number;
  /** Called after a committed threshold re-routes the library. */
  onLibraryChanged: () => void;
  /** Called after a commit, so the queue reloads with the newly-routed tracks. */
  onQueueChanged: () => void;
  /** Reports a failure to the panel's shared error banner. */
  onError: (message: string) => void;
}

/**
 * The threshold slider: drag to preview the split, commit to apply it.
 * @param props.reloadKey - recomputes the split when the library changed underneath it
 * @param props.onLibraryChanged - notifies the crate browser after a commit
 * @param props.onQueueChanged - reloads the triage queue after a commit
 * @param props.onError - surfaces failures
 */
export function ThresholdControl({
  reloadKey,
  onLibraryChanged,
  onQueueChanged,
  onError,
}: ThresholdControlProps) {
  const [percent, setPercent] = useState<number | null>(null);
  const [committed, setCommitted] = useState<number | null>(null);
  const [split, setSplit] = useState<ThresholdSplit | null>(null);
  const [committing, setCommitting] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        const settings = await getSettings();
        const current = Math.round(settings.confidence_threshold * PERCENT);
        setPercent(current);
        setCommitted(current);
      } catch (e) {
        onError(toMessage(e));
      }
    })();
  }, [onError]);

  useEffect(() => {
    if (percent === null) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const preview = await previewThreshold(percent / PERCENT);
        // A slower earlier preview must not overwrite a newer one and show the wrong split for the
        // slider's current position.
        if (!cancelled) {
          setSplit(preview);
        }
      } catch (e) {
        if (!cancelled) {
          onError(toMessage(e));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // `reloadKey` is a dependency because the split counts the library, not just the threshold —
    // a classify changes the answer while the slider has not moved.
  }, [percent, reloadKey, onError]);

  const handleCommit = useCallback(async () => {
    if (percent === null) {
      return;
    }
    setCommitting(true);
    try {
      setSplit(await updateThreshold(percent / PERCENT));
      setCommitted(percent);
      onQueueChanged();
      onLibraryChanged();
    } catch (e) {
      onError(toMessage(e));
    } finally {
      setCommitting(false);
    }
  }, [percent, onQueueChanged, onLibraryChanged, onError]);

  if (percent === null) {
    return null;
  }

  const dirty = percent !== committed;

  return (
    <div className="space-y-2 rounded-md border p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Label htmlFor="confidence-threshold">Confidence threshold</Label>
        <span className="font-mono text-sm tabular-nums text-muted-foreground">{percent}%</span>
      </div>

      <input
        id="confidence-threshold"
        type="range"
        min={PERCENT_MIN}
        max={PERCENT_MAX}
        step={PERCENT_STEP}
        value={percent}
        disabled={committing}
        onChange={(event) => setPercent(Number(event.target.value))}
        aria-describedby="threshold-split"
        className="h-2 w-full cursor-pointer appearance-none rounded-full bg-muted accent-primary focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
      />

      <div className="flex flex-wrap items-center justify-between gap-2">
        {/* aria-live: the split is the whole point of moving the slider, and a sighted user sees it
            update — a screen reader user must hear it too. */}
        <p id="threshold-split" aria-live="polite" className="text-xs text-muted-foreground">
          {split ? (
            <span className="tabular-nums">
              {split.auto} auto-filed · {split.manual} to triage
              {split.preserved > 0 && ` · ${split.preserved} kept (your decisions)`}
            </span>
          ) : (
            "Calculating…"
          )}
        </p>
        <Button
          type="button"
          size="sm"
          variant={dirty ? "default" : "outline"}
          disabled={!dirty || committing}
          onClick={handleCommit}
        >
          {committing ? <Loader2 aria-hidden="true" className="animate-spin" /> : null}
          {committing ? "Applying…" : dirty ? "Apply" : "Applied"}
        </Button>
      </div>
    </div>
  );
}
