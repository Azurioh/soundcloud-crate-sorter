# UI Refactor — Dark DJ Console (Tailwind v4 + shadcn/ui)

- **Date:** 2026-07-14
- **Status:** Approved (design), pending implementation plan
- **Branch:** `feat/ui-refactor` (based on `feat/desktop-shell`, stacked PR)
- **Scope owner:** `ui/` webview only — no Rust, no Tauri command, no `api.ts` contract change

## 1. Problem & Goal

The MVP webview (`ui/`, React 19 + TS + Vite) ships correct behavior on hand-written
`styles.css` with no design system. As the app grows (triage, settings, audio analysis)
the ad-hoc CSS and bespoke markup will not scale, and the current look does not read as a
DJ tool.

**Goal:** a presentation-only overhaul that (a) adopts a maintainable UI stack
(Tailwind v4 + shadcn/ui), and (b) applies a coherent **dark DJ-console** design system —
without touching behavior, data, or the boundary to the Rust core.

## 2. Scope

**In scope**
- Introduce Tailwind v4 + shadcn/ui + a bundled font/icon setup.
- Rebuild the two existing screens (Run, Crates) and shared `ErrorBanner` on the new system.
- Add proper loading / empty / error / busy states.
- Design tokens (dark-first, light secondary) with SoundCloud orange as primary.

**Non-goals (explicit)**
- No new screens (no Triage view, no Settings/threshold UI).
- No change to the Rust core, Tauri commands, or the `shared/api.ts` / `shared/errors.ts` contracts.
- No change to pipeline behavior, data shapes, or classification logic.
- No unrelated refactor of the feature/state structure (it is already clean feature-first).

## 3. Constraints (from CLAUDE.md + constitution + global rules)

- **Feature-first, no cross-feature imports.** Preserve `features/run`, `features/crates`,
  `shared/`. shadcn primitives are shared → live under `shared/ui/`.
- **Canonical path to the core.** Components depend on `shared/api.ts` only, never on
  `invoke` directly. This refactor must not add a second path.
- **Offline / no CDN.** Tauri webview: all fonts, icons, and assets bundled locally
  (no Google Fonts link, no remote CSS). Fonts via `@fontsource-*` npm packages.
- **Responsive.** Desktop-first (Tauri window) but must reflow down to a narrow window;
  tables scroll horizontally inside their own container; tap targets ≥ 44px.
- **Accessibility.** Keep `role="alert"` on the error banner, `aria-expanded` on the
  "Why?" toggle, `scope="col"` on table headers, visible focus states, dialog/collapsible
  semantics from shadcn primitives.
- **Clean build gate.** `pnpm typecheck` + `vite build` must pass with zero warnings before "done".

## 4. Stack Changes

| Concern | Choice | Notes |
|---|---|---|
| CSS engine | **Tailwind v4** | CSS-first `@theme` config in `styles.css`; no `tailwind.config.js` in v4. Verify exact v4 + Vite plugin setup via Context7 at implementation time. |
| Components | **shadcn/ui** (Tailwind v4 / React 19 compatible release) | Copied into `shared/ui/`; not a runtime dependency lock-in. |
| Primitives used | `button, input, label, card, table, badge, collapsible, alert, skeleton, scroll-area, separator, tooltip` | Add only what the two screens use — no speculative set. |
| Fonts (bundled) | Inter (UI) + JetBrains Mono (numerics) via `@fontsource-variable/*` | Imported in CSS, offline. Mono used for BPM / Camelot key / energy / confidence %. |
| Icons | `lucide-react` (shadcn default) | Bundled, tree-shaken. |

Dependencies installed via `pnpm add` in `ui/` (never hand-edited into `package.json`).

## 5. Design Tokens

Dark-first, light as secondary theme, using shadcn's CSS-variable convention so every
primitive inherits them.

- **Base layers (dark):** app background (deepest) → panel/card → elevated row, reusing the
  existing dark palette as the starting point (`#16181d` / `#1f2229` / `#2c313a` borders).
- **Text:** `#e8eaed` foreground, `#9aa0aa` muted.
- **Primary / accent:** SoundCloud orange `#ff5500`, foreground white. Used for primary
  actions, active accents, audit stage labels.
- **Semantic — confidence:** high = green, mid = amber, low / near-threshold = red. This
  visually reinforces the constitution's *no silent misfiling* rule (below-threshold routes
  to triage, never a guessed crate).
- **Semantic — status badges:** distinct color per track status (auto / triage / scanned /
  manually decided / deferred), derived from the `status` string.
- **Typography:** Inter for UI; JetBrains Mono for all technical numerics.
- **Light theme:** same tokens re-mapped; toggled via a header control and `prefers-color-scheme`
  as the initial signal.

Confidence is shown as a **color-coded badge with the mono percentage** (not a bar).

## 6. Component Architecture (boundaries unchanged)

```
ui/src/
  App.tsx                      # app shell: sticky header (title, subtitle, theme toggle)
  features/
    run/RunControl.tsx         # rebuilt on Card / Input / Button / stat cards
    crates/CrateBrowser.tsx    # rebuilt on Card / Collapsible / Table / Badge / ScrollArea
  shared/
    api.ts                     # UNCHANGED (Tauri bridge, canonical path)
    errors.ts                  # UNCHANGED
    ErrorBanner.tsx            # rebuilt on shadcn Alert; same props + role="alert" + dismiss
    ui/                        # NEW — shadcn primitives (shared, reused across features)
    theme/                     # NEW — theme provider + toggle (dark/light), if extracted
```

- No change to component props/behavior beyond presentation and added state UI.
- Any state-derivation helper that would nest a function inside a component (e.g. status →
  badge variant, confidence → semantic color) is extracted to a small pure module in
  `shared/` and unit-reasoned in isolation (no function-in-function).

## 7. Screen Behavior

### App shell
Dark background, sticky top bar: app title + subtitle + theme toggle. Content column centered,
max width preserved, responsive down to a narrow window.

### Run
- `Card`. Profile-URL `Input` + `Label`. Primary `Button` **Scan** (shows spinner + disables
  while busy), secondary `Button` **Classify all**.
- Scan / Classify result → inline status line (muted), persistent (not a toast).
- Summary → a row of compact stat `Card`s (Tracks / Crates / Auto / Triage / Unclassified /
  Decided) with icons and accent coloring.

### Crates
- Header with a ghost **Refresh** button (icon + label; spinner while loading).
- Empty state: icon + guidance copy ("Scan a profile and classify to build crates").
- Loading state: `Skeleton` rows.
- Each crate → `Card` with `Collapsible` body; track `Table` inside a `ScrollArea`
  (horizontal scroll on narrow widths).
- Columns: Title, Artist, Confidence (`Badge`, color-coded, mono %), Status (`Badge`),
  and a "Why?" toggle.
- "Why?" → `Collapsible` per row (chevron, `aria-expanded`); audit trail rendered as a
  compact timeline (stage accent, kind · outcome, mono detail k=v).

### Global states
- Error → shadcn `Alert`, persistent + dismissible (matches the existing blocking-error UX rule).
- All busy actions disable + show a spinner.

## 8. Testing & Verification

- `cd ui && pnpm typecheck` — zero errors.
- `cd ui && pnpm build` (`tsc --noEmit && vite build`) — zero warnings.
- `cargo tauri dev` — manual smoke of Scan → Classify → Crates → Why? in the real window.
- **`ui-fidelity-check`** skill — dual-viewport (desktop + narrow) verification before "done".
- No Rust changed → `cargo clippy` / `cargo fmt` unaffected.

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Tailwind v4 + shadcn setup differs from memory (v4 is CSS-first) | Resolve setup via Context7 before writing config; do not trust memory. |
| shadcn primitive pulls a second data path / breaks the api.ts boundary | Primitives are presentational only; data still flows through `api.ts`. Reviewed in code review. |
| Font/icon fetched from CDN (breaks offline Tauri) | Only `@fontsource-*` + bundled `lucide-react`; no remote `<link>`. |
| Scope creep into Triage/Settings | Explicit non-goal; kept out of this PR. |

## 10. Rollout

Single stacked PR: `feat/ui-refactor` → `feat/desktop-shell`. PR description carries the
**PR Stack** table (desktop-shell → main is the parent). Merges after (or alongside) the
desktop-shell PR lands.
