// A persistent, dismissible error message (spec edge cases / T070: blocking errors stay on
// screen rather than disappearing like a toast).
import { X } from "lucide-react";
import { Alert, AlertDescription } from "@/shared/ui/alert";
import { Button } from "@/shared/ui/button";

interface ErrorBannerProps {
  message: string | null;
  onDismiss: () => void;
}

/**
 * Renders a blocking error banner, or nothing when there is no message.
 * @param props.message - the error text, or null to render nothing
 * @param props.onDismiss - called when the user dismisses the banner
 */
export function ErrorBanner({ message, onDismiss }: ErrorBannerProps) {
  if (message === null) {
    return null;
  }
  return (
    <Alert variant="destructive" role="alert" className="mb-3 flex items-center justify-between gap-3">
      <AlertDescription>{message}</AlertDescription>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="size-6 shrink-0"
        onClick={onDismiss}
        aria-label="Dismiss error"
      >
        <X className="size-4" />
      </Button>
    </Alert>
  );
}
