import type { Tone } from "@/shared/display/tone";

/** Confidence band cutoffs (fraction 0..1). High ≥ 0.85, mid ≥ 0.7, else low. */
const CONFIDENCE_HIGH = 0.85;
const CONFIDENCE_MID = 0.7;
const PERCENT = 100;

interface ConfidenceBadge {
  label: string;
  tone: Tone;
}

/**
 * Maps a confidence fraction to a display percentage and semantic tone.
 * Green above {@link CONFIDENCE_HIGH}, amber above {@link CONFIDENCE_MID}, red below.
 * @param confidence - fraction 0..1, or null when not classified
 * @returns formatted percentage label + tone; null renders "—" neutral
 */
export function confidenceTone(confidence: number | null): ConfidenceBadge {
  if (confidence === null) {
    return { label: "—", tone: "neutral" };
  }
  const label = `${Math.round(confidence * PERCENT)}%`;
  if (confidence >= CONFIDENCE_HIGH) {
    return { label, tone: "success" };
  }
  if (confidence >= CONFIDENCE_MID) {
    return { label, tone: "warning" };
  }
  return { label, tone: "danger" };
}
