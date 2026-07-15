# Dark DJ-Console UI Refactor — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the `ui/` webview (Run + Crates screens + shared ErrorBanner) on Tailwind v4 + shadcn/ui with a dark DJ-console design system, changing presentation only.

**Architecture:** Feature-first React 19 + TS + Vite webview inside Tauri. Components depend on `shared/api.ts` (the one Tauri-bridge path) — unchanged. shadcn primitives are copied into `shared/ui/`; a theme provider lives in `shared/theme/`; pure display helpers (status→badge, confidence→tone) live in `shared/`. No Rust, no Tauri command, no data-shape change.

**Tech Stack:** React 19, TypeScript (strict, `verbatimModuleSyntax`), Vite 7, Tailwind CSS v4 (`@tailwindcss/vite`, CSS-first `@theme`), shadcn/ui (CSS variables, `lucide-react`), `@fontsource-variable/inter` + `@fontsource-variable/jetbrains-mono` (bundled offline).

## Global Constraints

Every task's requirements implicitly include these.

- **Offline / no CDN.** No Google Fonts `<link>`, no remote CSS/JS/asset. Fonts via `@fontsource-variable/*` npm packages, imported in CSS. Icons via bundled `lucide-react`.
- **Canonical core path.** Components import from `shared/api.ts` only — never `@tauri-apps/api` `invoke` directly. This refactor adds no second path.
- **Preserve upstream fix `02bc116`.** `feat/ui-refactor` is rebased onto it; these behaviors are live in the tree and MUST survive any rewrite:
  - `shared/api.ts` sends **camelCase** IPC keys (`{ profileUrl }`, `{ trackId }`) — Tauri v2's command macro defaults to `rename_all = "camelCase"`. Never revert to snake_case. `api.ts` stays untouched by this refactor.
  - `RunControl` trims the profile URL before scanning: `scan(profileUrl.trim())`.
  - The audit-trail read never caches a failure as "no events": on error keep `events` null, show a distinct message with a **Retry** action.
- **Feature-first, no cross-feature imports.** `features/run` and `features/crates` never import each other. Code used by both lives in `shared/`.
- **Install via package manager.** `pnpm add` / `pnpm dlx` inside `ui/`; never hand-edit `package.json` versions. Prefer latest stable.
- **Accessibility floors.** Tap targets ≥ 44px; `role="alert"` on error banner; `aria-expanded` on the "Why?" toggle; `scope="col"` on table headers; visible focus rings; contrast ≥ 4.5:1 both themes; `color-not-only` (status/confidence carry text, not color alone); respect `prefers-reduced-motion`.
- **Responsive.** Desktop-first (Tauri window) but reflows to a narrow window; tables scroll horizontally in their own container; no page-level horizontal scroll.
- **Code style.** Braces on every `if`/`else`; no function defined inside another function (extract pure helpers to files); no magic values (named constants); immutable updates; JSDoc on exported functions/types; one operation per line for non-trivial chains.
- **Clean build gate.** `cd ui && pnpm typecheck` and `pnpm build` finish with zero errors and zero warnings before any task is "done".
- **Commits in English**, Conventional Commits, trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

## File Structure

```
ui/
  vite.config.ts                      # MODIFY: add tailwindcss() plugin + "@" alias
  tsconfig.json                       # MODIFY: baseUrl + paths "@/*"
  components.json                     # CREATE: shadcn config (aliases → shared/)
  package.json                        # (managed by pnpm add)
  src/
    styles.css                        # REPLACE: Tailwind import + design tokens + fonts
    App.tsx                           # MODIFY: app shell + ThemeProvider + header
    shared/
      api.ts                          # UNCHANGED
      errors.ts                       # UNCHANGED
      ErrorBanner.tsx                 # MODIFY: rebuild on shadcn Alert
      lib/utils.ts                    # CREATE (shadcn): cn() helper
      ui/                             # CREATE (shadcn CLI): button, input, label, card,
                                      #   table, badge, collapsible, alert, skeleton,
                                      #   scroll-area, separator, tooltip
      ui/tone-badge.tsx               # CREATE: semantic tone badge (success/warning/danger/neutral)
      theme/
        theme-provider.tsx            # CREATE: dark-default provider (persist + prefers-color-scheme)
        theme-toggle.tsx              # CREATE: header toggle button
      display/
        track-status.ts              # CREATE: status → { label, tone } (compile-exhaustive)
        confidence-tone.ts           # CREATE: confidence number → tone + formatted %
    features/
      run/
        RunControl.tsx                # MODIFY: Card / Input / Button / busy states
        SummaryStats.tsx              # CREATE: extracted stat-card grid + STAT config
      crates/
        CrateBrowser.tsx              # MODIFY: list + empty + loading (Skeleton)
        CrateCard.tsx                 # CREATE: extracted Card + Collapsible + Table
        TrackRow.tsx                  # CREATE: extracted row (badges + Why? toggle)
        AuditTrail.tsx                # CREATE: extracted audit timeline
```

---

### Task 1: Tooling foundation — Tailwind v4 + path alias + shadcn init

Stands up the build stack. Deliverable: app builds and renders unchanged, with an (empty) Tailwind layer and shadcn wired to `shared/`.

**Files:**
- Modify: `ui/vite.config.ts`
- Modify: `ui/tsconfig.json`
- Create: `ui/components.json`
- Create: `ui/src/shared/lib/utils.ts`
- Modify: `ui/src/styles.css` (top of file only, this task)

**Interfaces:**
- Produces: path alias `@` → `ui/src`; `cn(...inputs)` from `@/shared/lib/utils`; shadcn `add` target `@/shared/ui`.

- [ ] **Step 1: Install Tailwind v4 + Vite plugin + node types**

```bash
cd ui
pnpm add tailwindcss @tailwindcss/vite
pnpm add -D @types/node
```

- [ ] **Step 2: Add the Tailwind plugin and `@` alias to Vite**

Replace `ui/vite.config.ts` with:

```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

// Tauri serves the built assets; keep the dev server on a fixed port for the webview.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022", outDir: "dist" },
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
});
```

- [ ] **Step 3: Add path mapping to `tsconfig.json`**

Add `baseUrl` + `paths` inside `compilerOptions` (keep every existing option):

```json
    "verbatimModuleSyntax": true,
    "baseUrl": ".",
    "paths": { "@/*": ["./src/*"] }
```

- [ ] **Step 4: Turn `styles.css` into the Tailwind entry**

Replace the entire current contents of `ui/src/styles.css` with just this line for now (full tokens land in Task 2):

```css
@import "tailwindcss";
```

- [ ] **Step 5: Create the `cn` helper (shadcn dependency)**

```bash
cd ui
pnpm add clsx tailwind-merge
```

Create `ui/src/shared/lib/utils.ts`:

```typescript
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merges class names, resolving conflicting Tailwind utilities (last wins).
 * @param inputs - class values (strings, arrays, conditionals)
 * @returns the merged className string
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
```

- [ ] **Step 6: Create `ui/components.json` (aliases point at `shared/`)**

```json
{
  "$schema": "https://ui.shadcn.com/schema.json",
  "style": "new-york",
  "rsc": false,
  "tsx": true,
  "tailwind": {
    "config": "",
    "css": "src/styles.css",
    "baseColor": "neutral",
    "cssVariables": true,
    "prefix": ""
  },
  "aliases": {
    "components": "@/shared/components",
    "utils": "@/shared/lib/utils",
    "ui": "@/shared/ui",
    "lib": "@/shared/lib",
    "hooks": "@/shared/hooks"
  },
  "iconLibrary": "lucide"
}
```

- [ ] **Step 7: Verify build + render is unchanged**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS, zero warnings. The bundle now includes Tailwind's reset; the old hand-written classes still work because Task 2 keeps them until components migrate. (If the app visibly loses styling because `@import "tailwindcss"` reset removed defaults, that is expected and fixed in Task 2 — do not chase it here.)

- [ ] **Step 8: Commit**

```bash
git add ui/vite.config.ts ui/tsconfig.json ui/components.json ui/src/shared/lib/utils.ts ui/src/styles.css ui/package.json ui/pnpm-lock.yaml
git commit -m "build(ui): add Tailwind v4, @ alias, and shadcn config"
```

---

### Task 2: Design tokens, bundled fonts, and theme provider

Deliverable: the dark-DJ-console token system + a working dark/light toggle (dark default).

**Files:**
- Modify: `ui/src/styles.css` (full token system)
- Create: `ui/src/shared/theme/theme-provider.tsx`

**Interfaces:**
- Produces: CSS variables per shadcn convention plus `--success` / `--warning` / `--danger`; theme colors `success|warning|danger` usable as Tailwind utilities (`bg-success/15 text-success`); `<ThemeProvider>` and `useTheme()` from `@/shared/theme/theme-provider`. (The `ThemeToggle` that consumes this provider is built in Task 3, once `Button` exists.)

- [ ] **Step 1: Bundle the fonts offline**

```bash
cd ui
pnpm add @fontsource-variable/inter @fontsource-variable/jetbrains-mono
```

- [ ] **Step 2: Write the full token system**

Replace `ui/src/styles.css` with:

```css
@import "tailwindcss";
@import "@fontsource-variable/inter";
@import "@fontsource-variable/jetbrains-mono";

/* shadcn convention: :root = light, .dark = dark. Dark is the product default —
   the ThemeProvider adds `.dark` on <html> unless the user selects light. */
@custom-variant dark (&:is(.dark *));

:root {
  --radius: 0.625rem;

  /* Light */
  --background: #f6f7f9;
  --foreground: #1b1e24;
  --card: #ffffff;
  --card-foreground: #1b1e24;
  --popover: #ffffff;
  --popover-foreground: #1b1e24;
  --primary: #ff5500; /* SoundCloud orange */
  --primary-foreground: #ffffff;
  --secondary: #eef0f3;
  --secondary-foreground: #1b1e24;
  --muted: #eef0f3;
  --muted-foreground: #6b7280;
  --accent: #e9ebef;
  --accent-foreground: #1b1e24;
  --destructive: #dc2626;
  --destructive-foreground: #ffffff;
  --border: #e2e5ea;
  --input: #e2e5ea;
  --ring: #ff5500;

  /* Semantic status/confidence tones */
  --success: #16a34a;
  --warning: #d97706;
  --danger: #dc2626;
}

.dark {
  /* Dark DJ console (default) */
  --background: #0b0d10;
  --foreground: #e8eaed;
  --card: #16181d;
  --card-foreground: #e8eaed;
  --popover: #16181d;
  --popover-foreground: #e8eaed;
  --primary: #ff5500;
  --primary-foreground: #ffffff;
  --secondary: #1f2229;
  --secondary-foreground: #e8eaed;
  --muted: #1f2229;
  --muted-foreground: #9aa0aa;
  --accent: #23262e;
  --accent-foreground: #e8eaed;
  --destructive: #ef4444;
  --destructive-foreground: #ffffff;
  --border: #2c313a;
  --input: #2c313a;
  --ring: #ff5500;

  --success: #22c55e;
  --warning: #f59e0b;
  --danger: #ef4444;
}

@theme inline {
  --radius-sm: calc(var(--radius) - 4px);
  --radius-md: calc(var(--radius) - 2px);
  --radius-lg: var(--radius);

  --color-background: var(--background);
  --color-foreground: var(--foreground);
  --color-card: var(--card);
  --color-card-foreground: var(--card-foreground);
  --color-popover: var(--popover);
  --color-popover-foreground: var(--popover-foreground);
  --color-primary: var(--primary);
  --color-primary-foreground: var(--primary-foreground);
  --color-secondary: var(--secondary);
  --color-secondary-foreground: var(--secondary-foreground);
  --color-muted: var(--muted);
  --color-muted-foreground: var(--muted-foreground);
  --color-accent: var(--accent);
  --color-accent-foreground: var(--accent-foreground);
  --color-destructive: var(--destructive);
  --color-destructive-foreground: var(--destructive-foreground);
  --color-border: var(--border);
  --color-input: var(--input);
  --color-ring: var(--ring);

  --color-success: var(--success);
  --color-warning: var(--warning);
  --color-danger: var(--danger);

  --font-sans: "Inter Variable", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  --font-mono: "JetBrains Mono Variable", ui-monospace, SFMono-Regular, Menlo, monospace;
}

@layer base {
  * {
    border-color: var(--color-border);
  }
  body {
    margin: 0;
    background: var(--color-background);
    color: var(--color-foreground);
    font-family: var(--font-sans);
  }
}
```

- [ ] **Step 3: Create the theme provider (dark default, persisted)**

Create `ui/src/shared/theme/theme-provider.tsx`:

```typescript
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

/** The two supported themes. Dark is the product default. */
export type Theme = "dark" | "light";

const STORAGE_KEY = "scs-theme";

interface ThemeContextValue {
  theme: Theme;
  toggle: () => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

/** Reads the persisted theme, falling back to the OS preference, then dark. */
function initialTheme(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "dark" || stored === "light") {
    return stored;
  }
  if (window.matchMedia("(prefers-color-scheme: light)").matches) {
    return "light";
  }
  return "dark";
}

/**
 * Provides the current theme and a toggle, and reflects it as a class on <html>.
 * @param props.children - the app subtree
 */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", theme === "dark");
    localStorage.setItem(STORAGE_KEY, theme);
  }, [theme]);

  const value = useMemo<ThemeContextValue>(
    () => ({ theme, toggle: () => setTheme((t) => (t === "dark" ? "light" : "dark")) }),
    [theme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

/**
 * Accesses the theme context.
 * @returns the current theme and its toggle
 * @throws Error if used outside a ThemeProvider
 */
export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (ctx === null) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return ctx;
}
```

- [ ] **Step 4: Verify tokens + provider compile**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS, zero warnings. The provider imports only React, so it typechecks standalone; nothing imports it yet, which is fine.

- [ ] **Step 5: Commit**

```bash
git add ui/src/styles.css ui/src/shared/theme ui/package.json ui/pnpm-lock.yaml
git commit -m "feat(ui): dark DJ-console design tokens, bundled fonts, theme provider"
```

---

### Task 3: shadcn primitives + semantic display helpers

Deliverable: every primitive the two screens need is present under `shared/ui/`, plus the pure status/confidence helpers.

**Files:**
- Create (CLI): `ui/src/shared/ui/{button,input,label,card,table,badge,collapsible,alert,skeleton,scroll-area,separator,tooltip}.tsx`
- Create: `ui/src/shared/ui/tone-badge.tsx`
- Create: `ui/src/shared/display/track-status.ts`
- Create: `ui/src/shared/display/confidence-tone.ts`
- Create: `ui/src/shared/theme/theme-toggle.tsx` (deferred from Task 2 — needs `Button`)

**Interfaces:**
- Consumes: `ThemeProvider` / `useTheme()` from `@/shared/theme/theme-provider` (Task 2).
- Produces:
  - shadcn `Button`, `Input`, `Label`, `Card` (+`CardHeader`/`CardTitle`/`CardContent`), `Table` (+ parts), `Badge`, `Collapsible` (+`CollapsibleTrigger`/`CollapsibleContent`), `Alert` (+`AlertTitle`/`AlertDescription`), `Skeleton`, `ScrollArea`, `Separator`, `Tooltip`.
  - `type Tone = "success" | "warning" | "danger" | "neutral" | "info"`
  - `<ToneBadge tone={Tone}>…</ToneBadge>` from `@/shared/ui/tone-badge`
  - `statusToBadge(status: string): { label: string; tone: Tone }` from `@/shared/display/track-status`
  - `confidenceTone(confidence: number | null): { label: string; tone: Tone }` from `@/shared/display/confidence-tone`
  - `<ThemeToggle />` from `@/shared/theme/theme-toggle`

- [ ] **Step 1: Add the primitives via the shadcn CLI**

```bash
cd ui
pnpm dlx shadcn@latest add button input label card table badge collapsible alert skeleton scroll-area separator tooltip
```
Expected: files written under `src/shared/ui/`, `lucide-react` + `@radix-ui/*` added to `package.json`. If the CLI prompts to overwrite `styles.css` or `components.json`, decline (our token file from Task 2 is authoritative).

- [ ] **Step 2: Verify primitives typecheck under `verbatimModuleSyntax`**

Run:
```bash
cd ui && pnpm typecheck
```
Expected: PASS. If any generated file errors on a value-vs-type import (`verbatimModuleSyntax`), fix by marking the offending import `import type { … }` — do not disable the compiler flag.

- [ ] **Step 3: Create the semantic `Tone` type + ToneBadge**

Create `ui/src/shared/ui/tone-badge.tsx`:

```typescript
import type { ReactNode } from "react";
import { Badge } from "@/shared/ui/badge";
import { cn } from "@/shared/lib/utils";

/** Semantic tone for status/confidence chips. */
export type Tone = "success" | "warning" | "danger" | "neutral" | "info";

const TONE_CLASSES: Record<Tone, string> = {
  success: "bg-success/15 text-success border-success/30",
  warning: "bg-warning/15 text-warning border-warning/30",
  danger: "bg-danger/15 text-danger border-danger/30",
  info: "bg-primary/15 text-primary border-primary/30",
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
```

- [ ] **Step 4: Create the status helper (compile-exhaustive over known statuses)**

Create `ui/src/shared/display/track-status.ts`:

```typescript
import type { Tone } from "@/shared/ui/tone-badge";

/** Track statuses emitted by the core (mirrors the `status` string on TrackView). */
export type KnownStatus =
  | "auto_classified"
  | "in_triage"
  | "scanned"
  | "manually_decided"
  | "deferred";

interface StatusBadge {
  label: string;
  tone: Tone;
}

/**
 * Presentation for every known status. `satisfies` makes an unhandled
 * status a compile error rather than a silent gap.
 */
const STATUS_BADGES = {
  auto_classified: { label: "Auto", tone: "success" },
  in_triage: { label: "Triage", tone: "warning" },
  scanned: { label: "Scanned", tone: "neutral" },
  manually_decided: { label: "Decided", tone: "info" },
  deferred: { label: "Deferred", tone: "neutral" },
} satisfies Record<KnownStatus, StatusBadge>;

/** Humanizes an unknown status token for display (never throws — this is presentation). */
function humanize(status: string): string {
  return status.replace(/_/g, " ");
}

/**
 * Maps a track status string to its badge label and tone.
 * @param status - the raw status string from the core
 * @returns label + semantic tone; unknown statuses render neutral + humanized
 */
export function statusToBadge(status: string): StatusBadge {
  if (status in STATUS_BADGES) {
    return STATUS_BADGES[status as KnownStatus];
  }
  return { label: humanize(status), tone: "neutral" };
}
```

- [ ] **Step 5: Create the confidence helper**

Create `ui/src/shared/display/confidence-tone.ts`:

```typescript
import type { Tone } from "@/shared/ui/tone-badge";

/** Confidence band cutoffs (fraction 0..1). High ≥ 0.85, mid ≥ 0.7, else low. */
const CONFIDENCE_HIGH = 0.85;
const CONFIDENCE_MID = 0.7;
const PERCENT = 100;

interface ConfidenceBadge {
  label: string;
  tone: Tone;
}

/**
 * Maps a confidence fraction to a display percentage and semantic tone.
 * Green above {@link CONFIDENCE_HIGH}, amber above {@link CONFIDENCE_MID}, red below.
 * @param confidence - fraction 0..1, or null when not classified
 * @returns formatted percentage label + tone; null renders "—" neutral
 */
export function confidenceTone(confidence: number | null): ConfidenceBadge {
  if (confidence === null) {
    return { label: "—", tone: "neutral" };
  }
  const label = `${Math.round(confidence * PERCENT)}%`;
  if (confidence >= CONFIDENCE_HIGH) {
    return { label, tone: "success" };
  }
  if (confidence >= CONFIDENCE_MID) {
    return { label, tone: "warning" };
  }
  return { label, tone: "danger" };
}
```

- [ ] **Step 6: Create the theme toggle button (`Button` now exists)**

Create `ui/src/shared/theme/theme-toggle.tsx`:

```typescript
import { Moon, Sun } from "lucide-react";
import { Button } from "@/shared/ui/button";
import { useTheme } from "@/shared/theme/theme-provider";

/** Header control that flips between dark and light themes. */
export function ThemeToggle() {
  const { theme, toggle } = useTheme();
  const isDark = theme === "dark";
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      onClick={toggle}
      aria-label={isDark ? "Switch to light theme" : "Switch to dark theme"}
    >
      {isDark ? <Sun className="size-4" /> : <Moon className="size-4" />}
    </Button>
  );
}
```

- [ ] **Step 7: Verify**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS, zero warnings.

- [ ] **Step 8: Commit**

```bash
git add ui/src/shared/ui ui/src/shared/display ui/src/shared/theme/theme-toggle.tsx ui/package.json ui/pnpm-lock.yaml
git commit -m "feat(ui): add shadcn primitives, tone badge, status/confidence helpers, theme toggle"
```

---

### Task 4: Rebuild ErrorBanner on shadcn Alert

Deliverable: same persistent, dismissible error contract, styled as a destructive Alert.

**Files:**
- Modify: `ui/src/shared/ErrorBanner.tsx`

**Interfaces:**
- Consumes: `Alert`, `AlertDescription` from `@/shared/ui/alert`; `Button` from `@/shared/ui/button`.
- Produces: `ErrorBanner({ message, onDismiss })` — unchanged props/behavior; keeps `role="alert"`.

- [ ] **Step 1: Rewrite the component**

Replace `ui/src/shared/ErrorBanner.tsx` with:

```typescript
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
```

- [ ] **Step 2: Verify**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS, zero warnings.

- [ ] **Step 3: Commit**

```bash
git add ui/src/shared/ErrorBanner.tsx
git commit -m "feat(ui): rebuild ErrorBanner on shadcn Alert"
```

---

### Task 5: App shell — header, ThemeProvider, layout

Deliverable: the dark app frame with a sticky header (title, subtitle, theme toggle) wrapping the two feature panels.

**Files:**
- Modify: `ui/src/App.tsx`

**Interfaces:**
- Consumes: `ThemeProvider` from `@/shared/theme/theme-provider`; `ThemeToggle` from `@/shared/theme/theme-toggle`; existing `RunControl`, `CrateBrowser`.
- Produces: unchanged `App` export + the `reloadKey` wiring between the two features.

- [ ] **Step 1: Rewrite `App.tsx`**

Replace `ui/src/App.tsx` with:

```typescript
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
```

- [ ] **Step 2: Verify**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS. (RunControl/CrateBrowser still use old markup — fine; they migrate in Tasks 6–7.)

- [ ] **Step 3: Commit**

```bash
git add ui/src/App.tsx
git commit -m "feat(ui): dark app shell with sticky header and theme toggle"
```

---

### Task 6: Rebuild the Run panel

Deliverable: Run panel on Card / Input / Button with busy spinners and a stat-card summary. No behavior change.

**Files:**
- Modify: `ui/src/features/run/RunControl.tsx`
- Create: `ui/src/features/run/SummaryStats.tsx`

**Interfaces:**
- Consumes: `Card`/`CardHeader`/`CardTitle`/`CardContent`, `Input`, `Label`, `Button` from `@/shared/ui/*`; `Loader2` from `lucide-react`; existing `scan`, `classifyAll`, `runSummary`, types, `ErrorBanner`, `toMessage`.
- Produces: `RunControl({ onLibraryChanged })` unchanged; `SummaryStats({ summary })` from `@/features/run/SummaryStats`.

- [ ] **Step 1: Create the extracted stat grid**

Create `ui/src/features/run/SummaryStats.tsx`:

```typescript
import type { RunSummary } from "@/shared/api";

/** Ordered stat tiles derived from a run summary. Keys index into RunSummary. */
const STATS = [
  { key: "total_tracks", label: "Tracks" },
  { key: "crate_count", label: "Crates" },
  { key: "auto_classified", label: "Auto" },
  { key: "in_triage", label: "Triage" },
  { key: "scanned", label: "Unclassified" },
  { key: "manually_decided", label: "Decided" },
] as const satisfies ReadonlyArray<{ key: keyof RunSummary; label: string }>;

/**
 * Renders the library summary as a responsive grid of stat tiles.
 * @param props.summary - the current run summary counts
 */
export function SummaryStats({ summary }: { summary: RunSummary }) {
  return (
    <dl className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6">
      {STATS.map((stat) => (
        <div key={stat.key} className="rounded-md border bg-card p-3 text-center">
          <dt className="text-xs uppercase tracking-wide text-muted-foreground">{stat.label}</dt>
          <dd className="mt-1 font-mono text-2xl font-bold tabular-nums">{summary[stat.key]}</dd>
        </div>
      ))}
    </dl>
  );
}
```

- [ ] **Step 2: Rewrite `RunControl.tsx`**

Replace `ui/src/features/run/RunControl.tsx` with (logic identical to current — only markup and the extracted `SummaryStats` change):

```typescript
// Scan / classify run control + run summary (T036). Drives the metadata-only MVP pipeline:
// enter a public profile URL → Scan → Classify all → see counts.
import { useCallback, useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import {
  classifyAll,
  runSummary,
  scan,
  type ClassifyResult,
  type RunSummary,
  type ScanResult,
} from "@/shared/api";
import { ErrorBanner } from "@/shared/ErrorBanner";
import { toMessage } from "@/shared/errors";
import { Button } from "@/shared/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/ui/card";
import { Input } from "@/shared/ui/input";
import { Label } from "@/shared/ui/label";
import { SummaryStats } from "@/features/run/SummaryStats";

interface RunControlProps {
  onLibraryChanged: () => void;
}

/** Run panel: scan a profile, classify all, and show library counts. */
export function RunControl({ onLibraryChanged }: RunControlProps) {
  const [profileUrl, setProfileUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scanResult, setScanResult] = useState<ScanResult | null>(null);
  const [classifyResult, setClassifyResult] = useState<ClassifyResult | null>(null);
  const [summary, setSummary] = useState<RunSummary | null>(null);

  const refreshSummary = useCallback(async () => {
    try {
      setSummary(await runSummary());
    } catch (e) {
      setError(toMessage(e));
    }
  }, []);

  useEffect(() => {
    void refreshSummary();
  }, [refreshSummary]);

  const handleScan = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setScanResult(await scan(profileUrl.trim()));
      await refreshSummary();
      onLibraryChanged();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setBusy(false);
    }
  }, [profileUrl, refreshSummary, onLibraryChanged]);

  const handleClassify = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setClassifyResult(await classifyAll());
      await refreshSummary();
      onLibraryChanged();
    } catch (e) {
      setError(toMessage(e));
    } finally {
      setBusy(false);
    }
  }, [refreshSummary, onLibraryChanged]);

  const canScan = profileUrl.trim().length > 0 && !busy;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Run</CardTitle>
      </CardHeader>
      <CardContent>
        <ErrorBanner message={error} onDismiss={() => setError(null)} />

        <div className="space-y-2">
          <Label htmlFor="profile-url">Public SoundCloud profile URL</Label>
          <div className="flex flex-wrap gap-2">
            <Input
              id="profile-url"
              type="url"
              inputMode="url"
              placeholder="https://soundcloud.com/your-name"
              value={profileUrl}
              onChange={(e) => setProfileUrl(e.target.value)}
              disabled={busy}
              className="min-w-[260px] flex-1 font-mono"
            />
            <Button type="button" onClick={handleScan} disabled={!canScan}>
              {busy ? <Loader2 className="size-4 animate-spin" /> : null}
              {busy ? "Working…" : "Scan"}
            </Button>
            <Button type="button" variant="outline" onClick={handleClassify} disabled={busy}>
              Classify all
            </Button>
          </div>
        </div>

        {scanResult && (
          <p className="mt-3 text-sm text-muted-foreground">
            Scanned {scanResult.liked_total} likes — {scanResult.new_tracks} new,{" "}
            {scanResult.duplicates_collapsed} duplicates collapsed, {scanResult.already_in_library}{" "}
            already in library.
          </p>
        )}
        {classifyResult && (
          <p className="mt-3 text-sm text-muted-foreground">
            Classified — {classifyResult.auto_classified} auto-sorted, {classifyResult.sent_to_triage}{" "}
            to triage, {classifyResult.skipped} skipped.
          </p>
        )}

        {summary && <SummaryStats summary={summary} />}
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 3: Verify**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS, zero warnings.

- [ ] **Step 4: Commit**

```bash
git add ui/src/features/run/RunControl.tsx ui/src/features/run/SummaryStats.tsx
git commit -m "feat(ui): rebuild Run panel on Card/Input/Button with stat tiles"
```

---

### Task 7: Rebuild the Crates browser

Deliverable: Crates on Card/Collapsible/Table/ScrollArea with color-coded confidence + status badges, a Skeleton loading state, and a proper empty state. The nested components are split into their own files.

**Files:**
- Modify: `ui/src/features/crates/CrateBrowser.tsx`
- Create: `ui/src/features/crates/CrateCard.tsx`
- Create: `ui/src/features/crates/TrackRow.tsx`
- Create: `ui/src/features/crates/AuditTrail.tsx`

**Interfaces:**
- Consumes: `Card`/`CardHeader`/`CardTitle`, `Button`, `Table` parts, `ScrollArea`, `Skeleton`, `Collapsible` parts from `@/shared/ui/*`; `ToneBadge`; `statusToBadge`; `confidenceTone`; `ChevronDown`/`RefreshCw`/`Inbox` from `lucide-react`; existing `listCrates`, `trackAudit`, types, `ErrorBanner`, `toMessage`.
- Produces: `CrateBrowser({ reloadKey })` unchanged; `CrateCard({ crate })`; `TrackRow({ track })`; `AuditTrail({ events })`.

- [ ] **Step 1: Create `AuditTrail.tsx`**

```typescript
import type { AuditEvent } from "@/shared/api";

/** Renders a track's audit events as a compact timeline ("why is this track here?"). */
export function AuditTrail({ events }: { events: AuditEvent[] }) {
  if (events.length === 0) {
    return <span className="text-sm text-muted-foreground">No audit events recorded for this track yet.</span>;
  }
  return (
    <ol className="flex flex-col gap-2 py-1">
      {events.map((event, index) => (
        <li key={index} className="flex flex-wrap items-baseline gap-2 text-sm">
          <span className="text-xs font-bold uppercase tracking-wide text-primary">{event.stage}</span>
          <span className="text-foreground">
            {event.kind} · {event.outcome}
          </span>
          <span className="font-mono text-xs text-muted-foreground">
            {Object.entries(event.detail)
              .map(([key, value]) => `${key}=${value}`)
              .join("  ")}
          </span>
        </li>
      ))}
    </ol>
  );
}
```

- [ ] **Step 2: Create `TrackRow.tsx`**

> **Preserve commit `02bc116` behavior.** The current `CrateBrowser.tsx` already hardens the
> audit-trail load (added upstream on `feat/desktop-shell`): a failed read must NOT be cached as
> "no events". Keep `events` null on error, surface a distinct message with a **Retry** action, and
> keep `aria-expanded` on the toggle. The code below carries that behavior forward onto the shadcn
> primitives — do not regress it back to `catch { setEvents([]) }`. The old `.audit-error` CSS class
> no longer exists (tokens replaced it); style the error with `text-destructive`.

```typescript
import { useCallback, useState } from "react";
import { trackAudit, type AuditEvent, type TrackView } from "@/shared/api";
import { toMessage } from "@/shared/errors";
import { statusToBadge } from "@/shared/display/track-status";
import { confidenceTone } from "@/shared/display/confidence-tone";
import { ToneBadge } from "@/shared/ui/tone-badge";
import { Button } from "@/shared/ui/button";
import { TableCell, TableRow } from "@/shared/ui/table";
import { AuditTrail } from "@/features/crates/AuditTrail";

/** One track row plus its lazily-loaded, expandable audit trail. */
export function TrackRow({ track }: { track: TrackView }) {
  const [open, setOpen] = useState(false);
  const [events, setEvents] = useState<AuditEvent[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [auditError, setAuditError] = useState<string | null>(null);
  const confidence = confidenceTone(track.confidence);
  const status = statusToBadge(track.status);

  const load = useCallback(async () => {
    setLoading(true);
    setAuditError(null);
    try {
      setEvents(await trackAudit(track.id));
    } catch (e) {
      // Leave events null so a retry re-fetches, rather than caching the failure as "no events".
      setAuditError(toMessage(e));
    } finally {
      setLoading(false);
    }
  }, [track.id]);

  const toggle = useCallback(() => {
    const next = !open;
    setOpen(next);
    if (next && events === null && !loading) {
      void load();
    }
  }, [open, events, loading, load]);

  return (
    <>
      <TableRow>
        <TableCell className="font-medium">{track.title}</TableCell>
        <TableCell className="text-muted-foreground">{track.artist}</TableCell>
        <TableCell>
          <ToneBadge tone={confidence.tone} className="font-mono tabular-nums">
            {confidence.label}
          </ToneBadge>
        </TableCell>
        <TableCell>
          <ToneBadge tone={status.tone}>{status.label}</ToneBadge>
        </TableCell>
        <TableCell className="text-right">
          <Button type="button" variant="link" size="sm" onClick={toggle} aria-expanded={open} className="h-auto p-0">
            {open ? "Hide" : "Why?"}
          </Button>
        </TableCell>
      </TableRow>
      {open && (
        <TableRow className="bg-muted/40 hover:bg-muted/40">
          <TableCell colSpan={5}>
            <AuditCell loading={loading} auditError={auditError} events={events} onRetry={() => void load()} />
          </TableCell>
        </TableRow>
      )}
    </>
  );
}

/** Renders the audit cell's loading / error / trail branches (module-level: no nested ternary). */
function AuditCell({
  loading,
  auditError,
  events,
  onRetry,
}: {
  loading: boolean;
  auditError: string | null;
  events: AuditEvent[] | null;
  onRetry: () => void;
}) {
  if (loading) {
    return <span className="text-sm text-muted-foreground">Loading trail…</span>;
  }
  if (auditError !== null) {
    return (
      <span className="text-sm text-destructive" role="alert">
        Could not load the audit trail.{" "}
        <Button type="button" variant="link" size="sm" className="h-auto p-0" onClick={onRetry}>
          Retry
        </Button>
      </span>
    );
  }
  return <AuditTrail events={events ?? []} />;
}
```

- [ ] **Step 3: Create `CrateCard.tsx`**

```typescript
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
```

- [ ] **Step 4: Rewrite `CrateBrowser.tsx` (list + empty + loading)**

Replace `ui/src/features/crates/CrateBrowser.tsx` with:

```typescript
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
      <CardHeader className="flex flex-row items-center justify-between space-y-0">
        <CardTitle>Crates</CardTitle>
        <Button type="button" variant="ghost" size="sm" onClick={reload} disabled={loading}>
          <RefreshCw className={loading ? "size-4 animate-spin" : "size-4"} />
          {loading ? "Loading…" : "Refresh"}
        </Button>
      </CardHeader>
      <CardContent>
        <ErrorBanner message={error} onDismiss={() => setError(null)} />

        {loading && crates.length === 0 && (
          <div className="flex flex-col gap-3">
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
```

- [ ] **Step 5: Verify**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS, zero warnings. If `TableHead` rejects the `scope` prop under strict types, pass it via `{...{ scope: "col" }}` or confirm the generated `TableHead` spreads `...props` onto `<th>` (it does by default) — do not drop the attribute.

- [ ] **Step 6: Commit**

```bash
git add ui/src/features/crates
git commit -m "feat(ui): rebuild Crates browser with badges, skeleton, and empty state"
```

---

### Task 8: Cleanup + full-app verification

Deliverable: no dead CSS/markup, a clean build, and a verified running app across viewports.

**Files:**
- Modify: `ui/src/styles.css` (only if dead rules remain)

- [ ] **Step 1: Remove dead hand-written CSS**

Confirm `styles.css` contains only the Tailwind import, font imports, `@custom-variant`, the token blocks, `@theme inline`, and the small `@layer base`. Delete any leftover legacy class rules (`.panel`, `.crate-card`, `.summary-grid`, `.track-table`, `.audit-trail`, etc.) — every one is now replaced by utilities/primitives.

Search for stale class names still referenced in TSX:
```bash
cd ui && grep -rEn "className=\"(app|panel|run-form|summary-|crate-|track-table|audit-|error-banner|link-button|empty|muted)" src || echo "no legacy classes referenced"
```
Expected: `no legacy classes referenced`.

- [ ] **Step 2: Clean build gate**

Run:
```bash
cd ui && pnpm typecheck && pnpm build
```
Expected: PASS, zero errors, zero warnings.

- [ ] **Step 3: Run the real app and smoke every flow**

Run (from repo root):
```bash
cargo tauri dev
```
Verify in the window:
- Header renders dark; theme toggle flips dark↔light and persists across reload.
- Enter a profile URL → **Scan** shows a spinner, then the scan result line + summary tiles.
- **Classify all** updates the classify line + tiles.
- Crates list renders; each track shows a color-coded confidence badge (green/amber/red) and a status badge.
- **Why?** expands an audit timeline; **Hide** collapses it.
- Trigger an error (e.g. empty/invalid profile) → a persistent destructive Alert appears and dismisses via the X.
- Narrow the window → track tables scroll horizontally; no page-level horizontal scroll; tap targets stay ≥ 44px.

- [ ] **Step 4: Dual-viewport fidelity pass**

Invoke the `ui-fidelity-check` skill and complete its desktop + narrow-viewport checklist against this app. Fix anything it flags, then re-run Step 2.

- [ ] **Step 5: Commit any cleanup**

```bash
git add ui/src/styles.css
git commit -m "chore(ui): remove dead legacy CSS after shadcn migration"
```

---

## Self-Review

**Spec coverage** (against `docs/superpowers/specs/2026-07-14-ui-refactor-dark-dj-console-design.md`):
- §4 stack (Tailwind v4, shadcn, fonts, icons) → Tasks 1–3. ✓
- §5 tokens (dark-first, orange primary, confidence/status tones, mono numerics, light theme) → Task 2 + helpers Task 3. ✓
- §6 architecture (feature-first, `shared/ui`, api.ts untouched, no function-in-function) → Tasks 3–7; helpers extracted to files. ✓
- §7 screens (shell, Run, Crates, states: loading/empty/error/busy) → Tasks 5–7. ✓
- §8 verification (typecheck/build/tauri dev/ui-fidelity-check) → per-task + Task 8. ✓
- §3 constraints (offline fonts, canonical path, a11y, responsive, clean build) → Global Constraints + enforced per task. ✓

**Placeholder scan:** No TBD/TODO. The Task 7 Step 5 `scope`-prop note is an explicit, resolved instruction, not a gap.

**Theming:** shadcn convention — `:root` = light, `.dark` = dark, `@custom-variant dark`. Dark-first is achieved by the `ThemeProvider` defaulting to `dark` (adds `.dark` on `<html>`). `theme-toggle.tsx` is built in Task 3, after `Button` exists, so every task's `pnpm typecheck` gate passes standalone.

**Type consistency:** `Tone` defined once (`@/shared/ui/tone-badge`) and imported by helpers + consumers. `statusToBadge`/`confidenceTone` signatures match their call sites in `TrackRow`. `SummaryStats` `key`s are `keyof RunSummary` (compile-checked). `ErrorBanner` props unchanged across Tasks 4/6/7. shadcn imports use the exact primitive paths added in Task 3.

## Execution Handoff

Ordering note: Tasks are sequential (each builds on the prior). Task 2's toggle and Task 4+ components depend on Task 3's primitives, so run in order 1→8; do not parallelize across tasks.
