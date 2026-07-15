// The triage queue's state and the single commit path both views share (T043/T044).
//
// The swipe deck and the assignment table are two presentations of one queue, so they must not each
// own a copy of it: a decision made in the table has to disappear from the deck. Both call the
// `commit` returned here, which is the only place a triage action reaches the core.
import { useCallback, useEffect, useState } from "react";
import {
  applyTriageAction,
  countDeferred,
  listCrateOptions,
  listTriageQueue,
  type CrateOption,
  type TriageAction,
  type TriageCard,
} from "@/shared/api";
import { toMessage } from "@/shared/errors";

interface TriageQueue {
  /** The cards awaiting a decision. */
  cards: TriageCard[];
  /** Every crate, for the picker. */
  crates: CrateOption[];
  /** How many tracks are deferred, awaiting a later session. */
  deferredCount: number;
  /** Whether the queue is loading for the first time. */
  loading: boolean;
  /** The track id currently being committed, if any. */
  committing: string | null;
  /** The last failure, if any. */
  error: string | null;
  /** Commits one action, removing the card on success. */
  commit: (trackId: string, action: TriageAction) => Promise<void>;
  /** Re-reads the queue, crates, and deferred count. */
  refresh: () => Promise<void>;
  /** Clears the current error. */
  dismissError: () => void;
}

/**
 * Loads the triage queue and exposes the one action that commits a decision.
 * @param params.reloadKey - bump to re-read the queue after a scan/classify changed the library
 * @param params.onLibraryChanged - called after a successful commit so the crate browser can reload
 * @returns the queue state plus its commit/refresh actions
 */
export function useTriageQueue({
  reloadKey,
  onLibraryChanged,
}: {
  reloadKey: number;
  onLibraryChanged: () => void;
}): TriageQueue {
  const [cards, setCards] = useState<TriageCard[]>([]);
  const [crates, setCrates] = useState<CrateOption[]>([]);
  const [deferredCount, setDeferredCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [committing, setCommitting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [queue, options, deferred] = await Promise.all([
        listTriageQueue(),
        listCrateOptions(),
        countDeferred(),
      ]);
      setCards(queue);
      setCrates(options);
      setDeferredCount(deferred);
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  // Re-reads on mount and whenever a run changed the library beneath the queue.
  useEffect(() => {
    void refresh();
  }, [refresh, reloadKey]);

  const commit = useCallback(
    async (trackId: string, action: TriageAction) => {
      setCommitting(trackId);
      setError(null);
      try {
        await applyTriageAction(trackId, action);
        // Drop the decided card locally rather than re-fetching: the deck would otherwise reshuffle
        // under the user's cursor between every swipe.
        setCards((current) => current.filter((card) => card.track.id !== trackId));
        if (action.kind === "defer") {
          setDeferredCount((count) => count + 1);
        }
        if (action.kind === "create_crate") {
          setCrates(await listCrateOptions());
        }
        onLibraryChanged();
      } catch (e) {
        setError(toMessage(e));
      } finally {
        setCommitting(null);
      }
    },
    [onLibraryChanged],
  );

  return {
    cards,
    crates,
    deferredCount,
    loading,
    committing,
    error,
    commit,
    refresh,
    dismissError: () => setError(null),
  };
}
