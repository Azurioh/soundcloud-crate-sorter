import { useCallback, useState } from "react";
import { RunControl } from "@/features/run/RunControl";
import { CrateBrowser } from "@/features/crates/CrateBrowser";
import { ThemeProvider } from "@/shared/theme/theme-provider";
import { ThemeToggle } from "@/shared/theme/theme-toggle";

/** Root component: app shell + the run→crates reload wiring. */
export function App() {
  // A monotonically-increasing key the run controls bump to ask the crate browser to reload.
  const [reloadKey, setReloadKey] = useState(0);
  const handleLibraryChanged = useCallback(() => setReloadKey((k) => k + 1), []);

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
          <RunControl onLibraryChanged={handleLibraryChanged} />
          <CrateBrowser reloadKey={reloadKey} />
        </main>
      </div>
    </ThemeProvider>
  );
}
