// The searchable full-crate picker, with create-on-the-fly (FR-016).
import { useMemo, useRef, useState } from "react";
import { Plus, Search } from "lucide-react";
import type { CrateOption, TriageAction } from "@/shared/api";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

interface CratePickerProps {
  crates: CrateOption[];
  onPick: (action: TriageAction) => void;
  onClose: () => void;
  disabled: boolean;
}

/** Filters crates by a case-insensitive substring of the typed query. */
function matching(crates: CrateOption[], query: string): CrateOption[] {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) {
    return crates;
  }
  return crates.filter((crate) => crate.name.toLowerCase().includes(needle));
}

/** Whether the typed query names a crate that does not exist yet (so it can be created). */
function isNewCrate(crates: CrateOption[], query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) {
    return false;
  }
  return !crates.some((crate) => crate.name.toLowerCase() === needle);
}

/**
 * A searchable list of every crate, plus a "create" affordance when the query names a new one.
 * @param props.crates - every existing crate
 * @param props.onPick - called with the chosen action (assign to existing, or create)
 * @param props.onClose - called to dismiss the picker
 * @param props.disabled - whether a commit is already in flight
 */
export function CratePicker({ crates, onPick, onClose, disabled }: CratePickerProps) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const results = useMemo(() => matching(crates, query), [crates, query]);
  const canCreate = useMemo(() => isNewCrate(crates, query), [crates, query]);

  return (
    // Escape must dismiss: the picker is a trap otherwise, since the deck's own hotkeys are
    // suspended while it is open.
    <div
      className="space-y-2"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <div className="relative">
        <Search
          aria-hidden="true"
          className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
        />
        <Input
          ref={inputRef}
          autoFocus
          type="search"
          aria-label="Search crates"
          placeholder="Search crates, or type a new name…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          className="pl-9"
          disabled={disabled}
        />
      </div>

      <ul className="max-h-56 space-y-1 overflow-y-auto" aria-label="Crates">
        {results.map((crate) => (
          <li key={crate.id}>
            <Button
              type="button"
              variant="ghost"
              className="w-full justify-start"
              disabled={disabled}
              onClick={() => onPick({ kind: "assign_to_crate", crate_id: crate.id })}
            >
              {crate.name}
            </Button>
          </li>
        ))}
        {results.length === 0 && !canCreate && (
          <li className="px-3 py-2 text-sm text-muted-foreground">No crates yet.</li>
        )}
      </ul>

      {canCreate && (
        <Button
          type="button"
          variant="outline"
          className="w-full justify-start"
          disabled={disabled}
          onClick={() => onPick({ kind: "create_crate", genre: query.trim() })}
        >
          <Plus aria-hidden="true" />
          Create “{query.trim()}”
        </Button>
      )}
    </div>
  );
}
