// The triage keyboard contract (T045), in one place so the card's visible key hints and the
// handlers that fire cannot drift apart.

/** Accepts the top suggestion. */
export const ACCEPT_HOTKEY = "a";
/** Opens the searchable full crate picker. */
export const PICKER_HOTKEY = "p";
/** Defers the current track to a later session. */
export const DEFER_HOTKEY = "d";
/** Picks the 1st/2nd/3rd alternative chip — index-aligned with the card's alternatives. */
export const ALTERNATIVE_HOTKEYS = ["1", "2", "3"] as const;
