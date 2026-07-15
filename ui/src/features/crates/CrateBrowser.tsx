// Crate browsing view (T037): crates with their members, per-track confidence, and an expandable
// audit trail per track ("why is this track in this crate?" — Principle VII / V6).
import { useCallback, useEffect, useState } from "react";
import { Inbox, RefreshCw } from "lucide-react";
import { listCrates, type CrateView } from "@/shared/api";
import { ErrorBanner } from "@/shared/ErrorBanner";
import { toMessage } from "@/shared/errors";
import { Button } from "@/shared/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/ui/card";
import { Skeleton } from "@/shared/ui/skeleton";
import { CrateCard } from "@/features/crates/CrateCard";

interface CrateBrowserProps {
  // Bumped by the run controls whenever the library changes, to trigger a reload.
  reloadKey: number;
}

/** Lists crates and their member tracks, with loading and empty states. */
export function CrateBrowser({ reloadKey }: CrateBrowserProps) {
  const [crates, setCrates] = useState<CrateView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setCrates(await listCrates());
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload, reloadKey]);

  const isEmpty = crates.length === 0 && !loading;

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <CardTitle>Crates</CardTitle>
        <Button type="button" variant="ghost" onClick={reload} disabled={loading}>
          <RefreshCw className={loading ? "size-4 animate-spin" : "size-4"} />
          {loading ? "Loading…" : "Refresh"}
        </Button>
      </CardHeader>
      <CardContent>
        <ErrorBanner message={error} onDismiss={() => setError(null)} />

        {loading && crates.length === 0 && (
          <div className="flex flex-col gap-3" role="status" aria-label="Loading crates">
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-24 w-full" />
          </div>
        )}

        {isEmpty ? (
          <div className="flex flex-col items-center gap-2 py-10 text-center">
            <Inbox className="size-8 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No crates yet. Scan a profile and classify to build crates.</p>
          </div>
        ) : (
          <ul className="flex flex-col gap-4">
            {crates.map((crate) => (
              <li key={crate.id}>
                <CrateCard crate={crate} />
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
