// The playful swipe deck (T043): one card at a time, with hotkeys and a peek at the next card.
import { useCallback, useState } from "react";
import type { CrateOption, TriageAction, TriageCard as TriageCardData } from "@/shared/api";
import { TriageCard } from "@/features/triage/TriageCard";
import { useTriageHotkeys } from "@/features/triage/useTriageHotkeys";
import {
  ACCEPT_HOTKEY,
  ALTERNATIVE_HOTKEYS,
  DEFER_HOTKEY,
  PICKER_HOTKEY,
} from "@/features/triage/hotkeys";

interface SwipeDeckProps {
  cards: TriageCardData[];
  crates: CrateOption[];
  committing: string | null;
  onAction: (trackId: string, action: TriageAction) => void;
}

/** One row of the shortcut legend. */
function ShortcutHint({ keyLabel, action }: { keyLabel: string; action: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <kbd className="rounded border bg-muted px-1.5 py-0.5 font-mono text-[10px]">{keyLabel}</kbd>
      {action}
    </span>
  );
}

/**
 * The card deck: decide the top card, the rest wait behind it.
 * @param props.cards - the queue, top card first
 * @param props.crates - every crate, for the picker
 * @param props.committing - the track id whose decision is in flight, if any
 * @param props.onAction - commits an action for a track
 */
export function SwipeDeck({ cards, crates, committing, onAction }: SwipeDeckProps) {
  const [pickerOpen, setPickerOpen] = useState(false);
  const top = cards[0] ?? null;
  const busy = committing !== null;

  const handleAction = useCallback(
    (action: TriageAction) => {
      if (!top) {
        return;
      }
      setPickerOpen(false);
      onAction(top.track.id, action);
    },
    [top, onAction],
  );

  useTriageHotkeys({
    card: top,
    enabled: !busy && !pickerOpen,
    onAction: handleAction,
    onOpenPicker: () => setPickerOpen(true),
  });

  if (!top) {
    return (
      <p className="py-10 text-center text-sm text-muted-foreground">
        Nothing left to triage — the queue is clear.
      </p>
    );
  }

  return (
    <div className="space-y-4">
      <div className="relative">
        {/* A static peek at the next card, so the deck reads as a stack with depth remaining. */}
        {cards.length > 1 && (
          <div
            aria-hidden="true"
            className="absolute inset-x-3 -bottom-2 h-10 rounded-lg border bg-card/60"
          />
        )}
        <TriageCard
          // Keying on the track id remounts the card per track, so the next one starts centred
          // rather than inheriting the previous card's fly-off transform.
          key={top.track.id}
          card={top}
          crates={crates}
          onAction={handleAction}
          committing={committing === top.track.id}
          pickerOpen={pickerOpen}
          onPickerOpenChange={setPickerOpen}
        />
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground">
        <span className="tabular-nums">
          {cards.length} {cards.length === 1 ? "track" : "tracks"} left
        </span>
        <div className="flex flex-wrap gap-3">
          <ShortcutHint keyLabel={ACCEPT_HOTKEY} action="accept" />
          <ShortcutHint keyLabel={ALTERNATIVE_HOTKEYS.join("/")} action="alternative" />
          <ShortcutHint keyLabel={PICKER_HOTKEY} action="all crates" />
          <ShortcutHint keyLabel={DEFER_HOTKEY} action="later" />
        </div>
      </div>
    </div>
  );
}
