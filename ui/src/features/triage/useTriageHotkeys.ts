// Keyboard shortcuts for the swipe deck (T045): accept / alt-1..3 / picker / defer.
import { useHotkeys } from "react-hotkeys-hook";
import type { TriageAction, TriageCard } from "@/shared/api";
import {
  ACCEPT_HOTKEY,
  ALTERNATIVE_HOTKEYS,
  DEFER_HOTKEY,
  PICKER_HOTKEY,
} from "@/features/triage/hotkeys";

interface TriageHotkeysParams {
  /** The card on top of the deck, or null when the queue is empty. */
  card: TriageCard | null;
  /** Whether shortcuts should fire at all (suspended while committing or picking). */
  enabled: boolean;
  /** Commits an action for the top card. */
  onAction: (action: TriageAction) => void;
  /** Opens the searchable picker. */
  onOpenPicker: () => void;
}

/**
 * Binds the deck's shortcuts to the top card.
 *
 * Deliberately not bound while the picker is open or a decision is in flight: the picker owns the
 * keyboard for typing a crate name (where "d" is a letter, not "defer"), and a second keystroke
 * mid-commit would file the *next* track under the previous card's answer.
 * @param params - the top card, enablement, and the action callbacks
 */
export function useTriageHotkeys({
  card,
  enabled,
  onAction,
  onOpenPicker,
}: TriageHotkeysParams): void {
  // `preventDefault` is not cosmetic here: the shortcut fires on keydown, and opening the picker
  // autofocuses its search box *within that same event* — so without it the browser's default
  // action types the shortcut's own letter into the field the shortcut just revealed ("p" landing
  // in the search box). Applied to every binding so no shortcut can leak a character into whatever
  // it focuses.
  const options = { enabled: enabled && card !== null, preventDefault: true };

  useHotkeys(
    ACCEPT_HOTKEY,
    () => {
      if (card?.suggestion) {
        onAction({ kind: "accept_suggestion" });
      }
    },
    options,
    [card, onAction],
  );

  useHotkeys(DEFER_HOTKEY, () => onAction({ kind: "defer" }), options, [card, onAction]);

  useHotkeys(PICKER_HOTKEY, onOpenPicker, options, [card, onOpenPicker]);

  useHotkeys(
    ALTERNATIVE_HOTKEYS.join(","),
    (_event, handler) => {
      const index = ALTERNATIVE_HOTKEYS.indexOf(
        handler.keys?.[0] as (typeof ALTERNATIVE_HOTKEYS)[number],
      );
      const alternative = index >= 0 ? card?.alternatives[index] : undefined;
      if (alternative) {
        onAction({ kind: "create_crate", genre: alternative.genre });
      }
    },
    options,
    [card, onAction],
  );
}
