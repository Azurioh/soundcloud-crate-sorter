// The dense assignment table (T044): the same queue, built for batch speed rather than play.
//
// Virtualized because the queue is unbounded — a cautious threshold can push a whole library into
// triage, and rendering thousands of rows would make the table the slow path it exists to avoid.
import { useMemo, useRef } from "react";
import {
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check, Clock } from "lucide-react";
import type { CrateOption, TriageAction, TriageCard } from "@/shared/api";
import { confidenceTone } from "@/shared/display/confidence-tone";
import { cn } from "@/shared/lib/utils";
import { Button } from "@/shared/ui/button";
import { ToneBadge } from "@/shared/ui/tone-badge";

/** Row height (px) the virtualizer reserves — matches the row's padding + line-height. */
const ROW_HEIGHT_PX = 52;
/** Rows rendered beyond the viewport, so fast scrolling does not reveal blanks. */
const OVERSCAN_ROWS = 8;
/** Height (px) of the scroll viewport. */
const VIEWPORT_HEIGHT_PX = 460;

/**
 * Per-column width classes.
 *
 * Virtualized rows must be positioned, so they cannot participate in a native table's column
 * layout — the header has to be laid out by the same flex rules as the body or the two drift apart.
 * Holding the classes in one place is what keeps them honest. The assign column is sized to hold
 * its select + accept + defer controls outright; the viewport scrolls horizontally rather than
 * letting them clip.
 */
const COLUMN_WIDTHS = {
  title: "min-w-0 flex-[2]",
  artist: "min-w-0 flex-[1.5]",
  confidence: "w-24 shrink-0",
  assign: "w-[21rem] shrink-0",
} as const;

interface AssignmentTableProps {
  cards: TriageCard[];
  crates: CrateOption[];
  committing: string | null;
  onAction: (trackId: string, action: TriageAction) => void;
}

const columnHelper = createColumnHelper<TriageCard>();

/** Reads the width class a column declared in its `meta`, for header and body cells alike. */
function columnClass(meta: unknown): string {
  if (typeof meta === "object" && meta !== null && "className" in meta) {
    const { className } = meta as { className: unknown };
    if (typeof className === "string") {
      return className;
    }
  }
  return "";
}

/**
 * A dense, virtualized table of the triage queue with per-row assignment controls.
 * @param props.cards - the queue
 * @param props.crates - every crate, for each row's select
 * @param props.committing - the track id whose decision is in flight, if any
 * @param props.onAction - commits an action for a track
 */
export function AssignmentTable({ cards, crates, committing, onAction }: AssignmentTableProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  const columns = useMemo(
    () => [
      columnHelper.accessor((card) => card.track.title, {
        id: "title",
        header: "Title",
        meta: { className: COLUMN_WIDTHS.title },
        cell: (info) => (
          <span className="block truncate font-medium" title={info.getValue()}>
            {info.getValue()}
          </span>
        ),
      }),
      columnHelper.accessor((card) => card.track.artist, {
        id: "artist",
        header: "Artist",
        meta: { className: COLUMN_WIDTHS.artist },
        cell: (info) => (
          <span className="block truncate text-muted-foreground" title={info.getValue()}>
            {info.getValue()}
          </span>
        ),
      }),
      columnHelper.accessor((card) => card.track.confidence, {
        id: "confidence",
        header: "Confidence",
        meta: { className: COLUMN_WIDTHS.confidence },
        cell: (info) => {
          const tone = confidenceTone(info.getValue());
          return (
            <ToneBadge tone={tone.tone} className="tabular-nums">
              {tone.label}
            </ToneBadge>
          );
        },
      }),
      columnHelper.display({
        id: "assign",
        header: "Assign to",
        meta: { className: COLUMN_WIDTHS.assign },
        cell: ({ row }) => {
          const card = row.original;
          const busy = committing === card.track.id;
          return (
            <div className="flex items-center gap-2">
              <label className="sr-only" htmlFor={`assign-${card.track.id}`}>
                Assign {card.track.title} to a crate
              </label>
              <select
                id={`assign-${card.track.id}`}
                disabled={busy}
                defaultValue=""
                onChange={(event) => {
                  const crateId = event.target.value;
                  if (crateId.length > 0) {
                    onAction(card.track.id, { kind: "assign_to_crate", crate_id: crateId });
                  }
                }}
                className="h-8 min-w-[9rem] rounded-md border bg-background px-2 text-sm focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none disabled:opacity-50"
              >
                <option value="" disabled>
                  Choose crate…
                </option>
                {crates.map((crate) => (
                  <option key={crate.id} value={crate.id}>
                    {crate.name}
                  </option>
                ))}
              </select>
              {card.suggestion && (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  title={`Accept ${card.suggestion.name}`}
                  onClick={() => onAction(card.track.id, { kind: "accept_suggestion" })}
                >
                  <Check aria-hidden="true" />
                  <span className="max-w-[8rem] truncate">{card.suggestion.name}</span>
                </Button>
              )}
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                disabled={busy}
                aria-label={`Defer ${card.track.title}`}
                onClick={() => onAction(card.track.id, { kind: "defer" })}
              >
                <Clock aria-hidden="true" />
              </Button>
            </div>
          );
        },
      }),
    ],
    [crates, committing, onAction],
  );

  const table = useReactTable({
    data: cards,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (card) => card.track.id,
  });

  const rows = table.getRowModel().rows;
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: OVERSCAN_ROWS,
  });

  if (cards.length === 0) {
    return (
      <p className="py-10 text-center text-sm text-muted-foreground">
        Nothing left to triage — the queue is clear.
      </p>
    );
  }

  return (
    <div
      ref={scrollRef}
      className="overflow-auto rounded-md border"
      style={{ height: VIEWPORT_HEIGHT_PX }}
    >
      {/* min-w keeps the columns at their intended widths and lets the viewport scroll instead of
          crushing the assign controls when the panel is narrow. */}
      <table className="w-full min-w-[52rem] text-sm">
        <thead className="sticky top-0 z-10 bg-card">
          {table.getHeaderGroups().map((headerGroup) => (
            <tr key={headerGroup.id} className="flex w-full border-b">
              {headerGroup.headers.map((header) => (
                <th
                  key={header.id}
                  scope="col"
                  className={cn(
                    "px-3 py-2 text-left text-xs font-medium text-muted-foreground",
                    columnClass(header.column.columnDef.meta),
                  )}
                >
                  {flexRender(header.column.columnDef.header, header.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody className="relative block" style={{ height: virtualizer.getTotalSize() }}>
          {virtualizer.getVirtualItems().map((virtualRow) => {
            const row = rows[virtualRow.index];
            return (
              <tr
                key={row.id}
                data-index={virtualRow.index}
                className="absolute flex w-full items-center border-b"
                style={{ height: ROW_HEIGHT_PX, transform: `translateY(${virtualRow.start}px)` }}
              >
                {row.getVisibleCells().map((cell) => (
                  <td key={cell.id} className={cn("px-3", columnClass(cell.column.columnDef.meta))}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
