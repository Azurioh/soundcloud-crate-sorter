import type { CrateView } from "@/shared/api";
import { Card, CardHeader, CardTitle } from "@/shared/ui/card";
import { ScrollArea, ScrollBar } from "@/shared/ui/scroll-area";
import { Table, TableBody, TableHead, TableHeader, TableRow } from "@/shared/ui/table";
import { TrackRow } from "@/features/crates/TrackRow";

/** One crate: its name, member count, and a scrollable table of member tracks. */
export function CrateCard({ crate }: { crate: CrateView }) {
  return (
    <Card className="overflow-hidden">
      <CardHeader className="flex flex-row items-baseline justify-between gap-4 space-y-0">
        <CardTitle className="text-base">{crate.name}</CardTitle>
        <span className="font-mono text-sm text-muted-foreground tabular-nums">{crate.members.length} tracks</span>
      </CardHeader>
      <ScrollArea className="w-full">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead scope="col">Title</TableHead>
              <TableHead scope="col">Artist</TableHead>
              <TableHead scope="col">Confidence</TableHead>
              <TableHead scope="col">Status</TableHead>
              <TableHead scope="col" aria-label="Audit trail" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {crate.members.map((track) => (
              <TrackRow key={track.id} track={track} />
            ))}
          </TableBody>
        </Table>
        <ScrollBar orientation="horizontal" />
      </ScrollArea>
    </Card>
  );
}
