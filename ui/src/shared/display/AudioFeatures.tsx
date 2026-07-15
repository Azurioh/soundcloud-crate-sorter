// Renders a track's measured audio features (US4). Shared: the crate table shows them today, and
// the export summary will want the same reading of the same facts.
import { TriangleAlert } from "lucide-react";
import type { TrackView } from "@/shared/api";

/**
 * Renders BPM / Camelot key / energy for a track, or a dash when it has not been analyzed.
 *
 * An uncertain BPM (FR-031) is always shown *with* its warning, never bare: the analyzer could not
 * tell the tempo from its half or double, and a number presented as fact would be exactly the
 * silent mis-tagging the flag exists to prevent.
 *
 * @param props.track - the track whose features to render
 */
export function AudioFeatures({ track }: { track: TrackView }) {
  const analyzed = track.bpm !== null || track.camelot_key !== null || track.energy !== null;
  if (!analyzed) {
    return (
      <span className="text-muted-foreground" aria-label="Not analyzed">
        —
      </span>
    );
  }

  return (
    <span className="flex flex-wrap items-center gap-x-2 gap-y-1 font-mono text-xs tabular-nums">
      {track.bpm !== null && (
        <span
          className={track.bpm_uncertain ? "flex items-center gap-1 text-amber-500" : undefined}
          title={
            track.bpm_uncertain
              ? "This tempo is ambiguous with its half/double — confirm it before trusting the BPM."
              : undefined
          }
        >
          {track.bpm_uncertain && <TriangleAlert className="size-3" aria-hidden="true" />}
          {track.bpm} BPM
          {/* The doubt is spelled out, not left to the colour and the icon alone. */}
          {track.bpm_uncertain && <span className="sr-only"> (uncertain — needs review)</span>}
          {track.bpm_uncertain && <span aria-hidden="true">?</span>}
        </span>
      )}
      {track.camelot_key !== null && <span>{track.camelot_key}</span>}
      {track.energy !== null && <span className="text-muted-foreground">E{track.energy}</span>}
    </span>
  );
}
