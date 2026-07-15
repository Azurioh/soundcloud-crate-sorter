import { useCallback, useState } from "react";
import { RunControl } from "@/features/run/RunControl";
import { AudioAnalysisPanel } from "@/features/run/AudioAnalysisPanel";
import { CrateBrowser } from "@/features/crates/CrateBrowser";
import { TriagePanel } from "@/features/triage/TriagePanel";
import { ThemeProvider } from "@/shared/theme/theme-provider";
import { ThemeToggle } from "@/shared/theme/theme-toggle";

/** Root component: app shell + the run→triage→crates reload wiring. */
export function App() {
  // Two keys, not one. `runKey` marks work that changed the library *underneath* the triage queue
  // (a scan or a classify), which the queue must re-read or it shows a stale "queue is clear".
  // `crateKey` marks anything that changed crate membership. A triage decision bumps only the
  // latter: it already knows which card it removed, and re-fetching the queue mid-session would
  // reshuffle the deck under the user's cursor between swipes.
  const [runKey, setRunKey] = useState(0);
  const [crateKey, setCrateKey] = useState(0);

  const handleRunFinished = useCallback(() => {
    setRunKey((k) => k + 1);
    setCrateKey((k) => k + 1);
  }, []);
  const handleTriageDecided = useCallback(() => setCrateKey((k) => k + 1), []);

  return (
    <ThemeProvider>
      <div className="min-h-dvh bg-background text-foreground">
        <header className="sticky top-0 z-10 border-b bg-background/80 backdrop-blur">
          <div className="mx-auto flex max-w-5xl items-center justify-between gap-4 px-4 py-3">
            <div>
              <h1 className="text-lg font-semibold tracking-tight">SoundCloud Crate Sorter</h1>
              <p className="text-sm text-muted-foreground">Turn your likes into organized DJ crates.</p>
            </div>
            <ThemeToggle />
          </div>
        </header>
        <main className="mx-auto flex max-w-5xl flex-col gap-5 px-4 py-6">
          <RunControl reloadKey={crateKey} onLibraryChanged={handleRunFinished} />
          <AudioAnalysisPanel reloadKey={crateKey} onLibraryChanged={handleRunFinished} />
          <TriagePanel reloadKey={runKey} onLibraryChanged={handleTriageDecided} />
          <CrateBrowser reloadKey={crateKey} />
        </main>
      </div>
    </ThemeProvider>
  );
}
