// The triage feature's shell: threshold control + the two views over one queue (FR-015/016/017).
import { useCallback, useState } from "react";
import { LayoutGrid, Rows3, Undo2 } from "lucide-react";
import { resumeDeferred } from "@/shared/api";
import { ErrorBanner } from "@/shared/ErrorBanner";
import { toMessage } from "@/shared/errors";
import { Button } from "@/shared/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/ui/card";
import { Skeleton } from "@/shared/ui/skeleton";
import { AssignmentTable } from "@/features/triage/AssignmentTable";
import { SwipeDeck } from "@/features/triage/SwipeDeck";
import { ThresholdControl } from "@/features/triage/ThresholdControl";
import { useTriageQueue } from "@/features/triage/useTriageQueue";

/** Which presentation of the queue is showing. */
type TriageView = "deck" | "table";

interface TriagePanelProps {
  reloadKey: number;
  onLibraryChanged: () => void;
}

/**
 * The triage panel: resolve uncertain tracks as a card deck or a dense table.
 * @param props.reloadKey - bumped when a scan/classify changed the library beneath the queue
 * @param props.onLibraryChanged - called after any decision, so the crate browser reloads
 */
export function TriagePanel({ reloadKey, onLibraryChanged }: TriagePanelProps) {
  const [view, setView] = useState<TriageView>("deck");
  const [thresholdError, setThresholdError] = useState<string | null>(null);
  const queue = useTriageQueue({ reloadKey, onLibraryChanged });

  const handleResume = useCallback(async () => {
    try {
      await resumeDeferred();
      await queue.refresh();
    } catch (e) {
      setThresholdError(toMessage(e));
    }
  }, [queue]);

  const error = queue.error ?? thresholdError;

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between gap-4">
        <CardTitle>
          Triage
          {queue.cards.length > 0 && (
            <span className="ml-2 font-mono text-sm tabular-nums text-muted-foreground">
              {queue.cards.length}
            </span>
          )}
        </CardTitle>
        <div className="flex gap-1" role="group" aria-label="Triage view">
          <Button
            type="button"
            size="sm"
            variant={view === "deck" ? "secondary" : "ghost"}
            aria-pressed={view === "deck"}
            onClick={() => setView("deck")}
          >
            <LayoutGrid aria-hidden="true" />
            Cards
          </Button>
          <Button
            type="button"
            size="sm"
            variant={view === "table" ? "secondary" : "ghost"}
            aria-pressed={view === "table"}
            onClick={() => setView("table")}
          >
            <Rows3 aria-hidden="true" />
            Table
          </Button>
        </div>
      </CardHeader>

      <CardContent className="space-y-4">
        <ErrorBanner
          message={error}
          onDismiss={() => {
            queue.dismissError();
            setThresholdError(null);
          }}
        />

        <ThresholdControl
          reloadKey={reloadKey}
          onLibraryChanged={onLibraryChanged}
          onQueueChanged={() => void queue.refresh()}
          onError={setThresholdError}
        />

        {queue.deferredCount > 0 && (
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-warning/30 bg-warning/10 px-3 py-2">
            <p className="text-sm text-muted-foreground">
              <span className="tabular-nums">{queue.deferredCount}</span>{" "}
              {queue.deferredCount === 1 ? "track is" : "tracks are"} waiting for a later session.
            </p>
            <Button type="button" size="sm" variant="outline" onClick={handleResume}>
              <Undo2 aria-hidden="true" />
              Bring back
            </Button>
          </div>
        )}

        {queue.loading ? (
          <div className="space-y-2">
            <Skeleton className="h-40 w-full" />
            <Skeleton className="h-4 w-40" />
          </div>
        ) : view === "deck" ? (
          <SwipeDeck
            cards={queue.cards}
            crates={queue.crates}
            committing={queue.committing}
            onAction={(trackId, action) => void queue.commit(trackId, action)}
          />
        ) : (
          <AssignmentTable
            cards={queue.cards}
            crates={queue.crates}
            committing={queue.committing}
            onAction={(trackId, action) => void queue.commit(trackId, action)}
          />
        )}
      </CardContent>
    </Card>
  );
}
