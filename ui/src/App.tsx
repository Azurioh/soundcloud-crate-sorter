import { useCallback, useState } from "react";
import { RunControl } from "./features/run/RunControl";
import { CrateBrowser } from "./features/crates/CrateBrowser";

export function App() {
  // A monotonically-increasing key the run controls bump to ask the crate browser to reload.
  const [reloadKey, setReloadKey] = useState(0);
  const handleLibraryChanged = useCallback(() => setReloadKey((k) => k + 1), []);

  return (
    <main className="app">
      <header className="app__header">
        <h1>SoundCloud Crate Sorter</h1>
        <p className="app__subtitle">Turn your likes into organized DJ crates.</p>
      </header>
      <RunControl onLibraryChanged={handleLibraryChanged} />
      <CrateBrowser reloadKey={reloadKey} />
    </main>
  );
}
