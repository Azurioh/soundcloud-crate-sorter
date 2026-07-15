// One triage card: preview, top suggestion, alternative chips, picker, defer (FR-015/FR-016).
import { useState } from "react";
import { motion, useMotionValue, useReducedMotion, useTransform } from "motion/react";
import { useDrag } from "@use-gesture/react";
import { Check, Clock, ListMusic, Loader2 } from "lucide-react";
import type { TriageAction, TriageCard as TriageCardData } from "@/shared/api";
import { confidenceTone } from "@/shared/display/confidence-tone";
import { Button } from "@/shared/ui/button";
import { Card, CardContent } from "@/shared/ui/card";
import { ToneBadge } from "@/shared/ui/tone-badge";
import { CratePicker } from "@/features/triage/CratePicker";
import { ALTERNATIVE_HOTKEYS } from "@/features/triage/hotkeys";
import type { CrateOption } from "@/shared/api";

/** Horizontal travel (px) past which a release commits the swipe. */
const SWIPE_COMMIT_PX = 120;
/** Travel (px) over which the accept/defer intent hints reach full opacity. */
const HINT_RAMP_PX = 80;
/** How far (px) a committed card flies off before it unmounts. */
const SWIPE_EXIT_PX = 400;
/** Rotation (deg) at full drag travel — the "playful" tilt. */
const SWIPE_TILT_DEG = 8;
/** Fly-off duration (s). Kept inside the 150–300ms micro-interaction band. */
const EXIT_DURATION_S = 0.2;

interface TriageCardProps {
  card: TriageCardData;
  crates: CrateOption[];
  onAction: (action: TriageAction) => void;
  committing: boolean;
  pickerOpen: boolean;
  onPickerOpenChange: (open: boolean) => void;
}

/**
 * A swipeable triage card. Swiping right accepts the suggestion, left defers — but every gesture
 * has an equivalent button and hotkey, because a gesture-only control is unreachable by keyboard
 * and invisible to anyone who has never been told it exists.
 * @param props.card - the queued track with its suggestion and alternatives
 * @param props.crates - every crate, for the picker
 * @param props.onAction - called with the chosen triage action
 * @param props.committing - whether this card's decision is in flight
 * @param props.pickerOpen - whether the full crate picker is showing
 * @param props.onPickerOpenChange - opens/closes the picker
 */
export function TriageCard({
  card,
  crates,
  onAction,
  committing,
  pickerOpen,
  onPickerOpenChange,
}: TriageCardProps) {
  const [exitX, setExitX] = useState(0);
  const reducedMotion = useReducedMotion();
  const x = useMotionValue(0);
  // The tilt is pure decoration, so it is the first thing to go when motion is unwelcome; the
  // card still tracks the finger, because a drag that does not follow the hand reads as broken.
  const tilt = reducedMotion ? 0 : SWIPE_TILT_DEG;
  const rotate = useTransform(x, [-SWIPE_COMMIT_PX, SWIPE_COMMIT_PX], [-tilt, tilt]);
  const acceptOpacity = useTransform(x, [0, HINT_RAMP_PX], [0, 1]);
  const deferOpacity = useTransform(x, [-HINT_RAMP_PX, 0], [1, 0]);

  const canAccept = card.suggestion !== null;

  const bind = useDrag(
    ({ down, movement: [mx], last }) => {
      if (committing) {
        return;
      }
      x.set(down ? mx : 0);
      if (!last || Math.abs(mx) < SWIPE_COMMIT_PX) {
        return;
      }
      // Swiping right onto a card with no suggestion has nothing to accept — snap back rather than
      // fly the card away on an action that will fail.
      if (mx > 0 && !canAccept) {
        return;
      }
      setExitX(mx > 0 ? SWIPE_EXIT_PX : -SWIPE_EXIT_PX);
      onAction(mx > 0 ? { kind: "accept_suggestion" } : { kind: "defer" });
    },
    { axis: "x", enabled: !committing && !pickerOpen },
  );

  const confidence = confidenceTone(card.track.confidence);

  return (
    // The gesture binds to a plain wrapper rather than the motion element: use-gesture spreads a
    // native `onDrag` handler, whose type collides with motion's own pan-based `onDrag` prop.
    // `touch-none` stops the webview from claiming the horizontal drag as a scroll.
    <div {...bind()} className="touch-none">
      <motion.div
        style={{ x, rotate }}
        // Reduced motion still needs the card to *leave* — that is feedback, not decoration — so it
        // fades out in place instead of flying across the viewport.
        animate={exitX === 0 ? undefined : reducedMotion ? { opacity: 0 } : { x: exitX, opacity: 0 }}
        transition={{ duration: EXIT_DURATION_S, ease: "easeOut" }}
      >
        <Card className="relative overflow-hidden">
          {/* Intent hints track the finger, so the gesture explains itself mid-drag. */}
          <motion.div
            aria-hidden="true"
            style={{ opacity: acceptOpacity }}
            className="pointer-events-none absolute top-4 left-4 rounded-md border border-success/40 bg-success/15 px-2 py-1 text-xs font-semibold text-success"
          >
            Accept
          </motion.div>
          <motion.div
            aria-hidden="true"
            style={{ opacity: deferOpacity }}
            className="pointer-events-none absolute top-4 right-4 rounded-md border border-warning/40 bg-warning/15 px-2 py-1 text-xs font-semibold text-warning"
          >
            Later
          </motion.div>

          <CardContent className="space-y-4 pt-6">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <p className="truncate font-semibold" title={card.track.title}>
                  {card.track.title}
                </p>
                <p className="truncate text-sm text-muted-foreground" title={card.track.artist}>
                  {card.track.artist}
                </p>
              </div>
              <ToneBadge tone={confidence.tone} className="tabular-nums">
                {confidence.label}
              </ToneBadge>
            </div>

            {/* Audio preview (FR-015). The permalink opens in the user's browser: the webview has no
                SoundCloud session, so an inline stream would not authorize. */}
            <a
              href={card.permalink_url}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 rounded-md text-sm text-info underline-offset-4 hover:underline focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
            >
              <ListMusic aria-hidden="true" className="size-4" />
              Preview on SoundCloud
            </a>

            {pickerOpen ? (
              <CratePicker
                crates={crates}
                onPick={onAction}
                onClose={() => onPickerOpenChange(false)}
                disabled={committing}
              />
            ) : (
              <div className="space-y-3">
                <div>
                  <p className="mb-1.5 text-xs font-medium text-muted-foreground">Top suggestion</p>
                  {card.suggestion ? (
                    <Button
                      type="button"
                      className="w-full justify-center"
                      disabled={committing}
                      onClick={() => onAction({ kind: "accept_suggestion" })}
                    >
                      {committing ? <Loader2 aria-hidden="true" className="animate-spin" /> : <Check aria-hidden="true" />}
                      {card.suggestion.name}
                    </Button>
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      No suggestion — pick a crate below.
                    </p>
                  )}
                </div>

                {card.alternatives.length > 0 && (
                  <div>
                    <p className="mb-1.5 text-xs font-medium text-muted-foreground">Or</p>
                    <div className="flex flex-wrap gap-2">
                      {card.alternatives.map((alternative, index) => (
                        <Button
                          key={alternative.genre}
                          type="button"
                          variant="outline"
                          size="sm"
                          disabled={committing}
                          onClick={() => onAction({ kind: "create_crate", genre: alternative.genre })}
                        >
                          <kbd className="rounded border px-1 font-mono text-[10px] text-muted-foreground">
                            {ALTERNATIVE_HOTKEYS[index]}
                          </kbd>
                          {alternative.genre}
                        </Button>
                      ))}
                    </div>
                  </div>
                )}

                <div className="flex flex-wrap gap-2 pt-1">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={committing}
                    onClick={() => onPickerOpenChange(true)}
                  >
                    <ListMusic aria-hidden="true" />
                    All crates…
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={committing}
                    onClick={() => onAction({ kind: "defer" })}
                  >
                    <Clock aria-hidden="true" />
                    Later
                  </Button>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      </motion.div>
    </div>
  );
}
