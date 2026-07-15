// Normalizes a rejected Tauri command (which rejects with our neutral message string) into text.
export function toMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Something went wrong.";
}
