import type { ReactNode } from "react";
import type { Tone } from "@/shared/display/tone";
import { Badge } from "@/shared/ui/badge";
import { cn } from "@/shared/lib/utils";

const TONE_CLASSES: Record<Tone, string> = {
  success: "bg-success/15 text-success border-success/30",
  warning: "bg-warning/15 text-warning border-warning/30",
  danger: "bg-danger/15 text-danger border-danger/30",
  info: "bg-info/15 text-info border-info/30",
  neutral: "bg-muted text-muted-foreground border-border",
};

/**
 * A Badge tinted by semantic tone (used for track status and confidence).
 * @param props.tone - the semantic tone driving the color
 * @param props.className - extra classes
 * @param props.children - badge content
 */
export function ToneBadge({
  tone,
  className,
  children,
}: {
  tone: Tone;
  className?: string;
  children: ReactNode;
}) {
  return (
    <Badge variant="outline" className={cn("font-medium", TONE_CLASSES[tone], className)}>
      {children}
    </Badge>
  );
}
