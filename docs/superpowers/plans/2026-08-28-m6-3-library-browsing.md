# M6.3 Library Browsing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the M6.2 populated placeholder with a bounded, race-safe, accessible, production-quality local library browser.

**Architecture:** A focused `useLibraryQuery` hook owns debounced inputs, bounded page replacement, request/loading ownership, authoritative favorite mutations, scan refreshes, and coalesced visible metadata invalidations. `AppShell` owns search/system/favorite controls that span the header/sidebar/content layout; `LibraryBrowser` and `GameCard` render only M6.1 list DTOs.

**Tech Stack:** React 19, TypeScript 6, Vitest, Testing Library, project CSS/design tokens, existing typed Tauri IPC wrappers.

**Spec:** `docs/superpowers/specs/2026-08-28-m6-3-library-browsing-design.md`

## Global Constraints

- Work only on `feat/m6-library-ui`; do not push, merge, or rewrite M6.1/M6.2 history.
- Preserve untracked review artifacts and unrelated files.
- M6.3 starts at `28e20dab7c5d68e100555ac94f7f610b2583c728`; design checkpoint `acdf1da`.
- Use bounded `query_library`; never use `get_library_snapshot` for browsing.
- No per-card detail/metadata IPC, global state library, debounce library, UI framework, or backend facet subsystem.
- Genre/region facet discovery remains deferred because no bounded aggregate option contract exists.
- Do not implement M6.4, M6.5, M6.6, or later milestone behavior.
- Use synthetic/local fixtures only.
- Follow red-green-refactor for each behavior change.

---

### Task 1: Correct M6.2 DELTA-LOW-1 Loading Ownership

**Files:**
- Modify: `src/hooks/useScanState.ts`
- Test: `src/app/AppShell.test.tsx`
- Modify after green: `docs/M6_REPORT.md`

**Interfaces:** Keep the existing `ScanStateModel`; add only private operation-owner refs.

- [ ] **Step 1: Write the overlapping-refresh regression**

Add a test beside the existing issue supersession cases. Hold the bootstrap offset-zero request and
a completion-triggered offset-zero request separately. Resolve the older request first and assert
`READING SAVED ISSUES…` remains until the newer request resolves. Use complete local
`ScanSummary`/`ScanIssuePage` literals; assert the newer issue is the rendered result.

- [ ] **Step 2: Verify RED**

```bash
pnpm vitest run src/app/AppShell.test.tsx -t "keeps refresh loading owned"
```

Expected: FAIL because the older request clears `issueLoading` while the newer request is pending.

- [ ] **Step 3: Implement operation-owned teardown**

Add:

```ts
const issueLoadingOwner = useRef(0);
const issueLoadingMoreOwner = useRef(0);
```

Assign the operation's `requestVersion` to its owner before setting the flag. Each `finally` clears
only when mounted and its owner still equals that request. Keep the shared result/error generation
guards unchanged so refresh and load-more can supersede each other's data without stranding flags.

- [ ] **Step 4: Verify GREEN and record the correction**

```bash
pnpm vitest run src/app/AppShell.test.tsx -t "loading|supersedes"
```

Expected: the new test and both existing supersession tests PASS. Add the exact owner/version model
and regression to M6_REPORT section E.

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useScanState.ts src/app/AppShell.test.tsx docs/M6_REPORT.md
git commit -m "fix(ui): harden scan request loading ownership"
```

---

### Task 2: Build the Bounded Query Hook

**Files:**
- Create: `src/hooks/useLibraryQuery.ts`
- Create: `src/hooks/useLibraryQuery.test.tsx`

**Interfaces:**

```ts
export interface LibraryQueryControls {
  enabled: boolean;
  scanCompletionRunId: number | null;
  onFavoriteCommitted?: () => void | Promise<void>;
}

export interface LibraryQueryModel {
  searchInput: string;
  setSearchInput(value: string): void;
  systemId: SystemId | null;
  setSystemId(value: SystemId | null): void;
  favoritesOnly: boolean;
  setFavoritesOnly(value: boolean): void;
  page: LibraryPage | null;
  initialLoading: boolean;
  refreshing: boolean;
  pageLoading: boolean;
  error: IpcError | null;
  favoriteError: IpcError | null;
  favoritePendingIds: ReadonlySet<number>;
  retry(): Promise<void>;
  clearSearch(): void;
  resetQuery(): void;
  previousPage(): void;
  nextPage(): void;
  toggleFavorite(item: LibraryListItem): Promise<void>;
}
```

- [ ] **Step 1: Write initial/error/retry tests**

Mock only `platform/ipc`, render the hook, and assert the first call is exactly:

```ts
expect(mocks.queryLibrary).toHaveBeenCalledWith({ sort: 'titleAsc', offset: 0 });
```

Assert a successful fixture page becomes current. For rejection, assert `page === null`, loading
ends, and the normalized code is retained; switch the mock to success, call `retry`, and assert the
error clears.

- [ ] **Step 2: Verify RED**

```bash
pnpm vitest run src/hooks/useLibraryQuery.test.tsx -t "initial|retry"
```

Expected: FAIL because the hook does not exist.

- [ ] **Step 3: Implement minimal request state**

Use mounted, request-generation, and per-loading-channel owner refs. Build requests from non-empty
debounced search, non-null system, true favorites, `sort: 'titleAsc'`, and offset. Omit `limit` to use
the backend default. Only the latest generation writes page/error; only the current owner clears its
loading channel. Preserve an existing page during refresh/page work.

- [ ] **Step 4: Verify initial GREEN**

Run Step 2; expect PASS.

- [ ] **Step 5: Write paging/reset tests**

Test first page, full middle page, partial final page, and no-more-pages. Assert next uses
`page.offset + page.limit`, previous uses `max(0, offset - limit)`, and disabled directions issue no
request. Call the hook's system/favorite setters while page two is pending; assert offset resets to
zero and the stale page-two response cannot install.

- [ ] **Step 6: Verify paging RED, implement, then GREEN**

```bash
pnpm vitest run src/hooks/useLibraryQuery.test.tsx -t "page|filter"
```

Expected RED: paging missing. Implement replacement-only paging. If a response reports `total > 0`,
empty items, and `offset >= total`, calculate the last valid offset from its effective limit and
request it. Rerun; expected PASS.

- [ ] **Step 7: Write debounce/literal/race/unmount tests**

Use fake timers. Type `A`, advance 100 ms, type `A%_\\`, and prove no call before 200 ms. Then assert
the exact literal reaches IPC:

```ts
expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
  search: 'A%_\\',
  sort: 'titleAsc',
  offset: 0,
});
```

Hold A and B: resolve B then A and assert B remains; reject A after B success and assert no stale
error; resolve A while B is pending and assert loading remains owned by B. Clear search and verify a
debounced empty query. Unmount with request/timer pending and prove no later IPC/state update.

- [ ] **Step 8: Verify RED, implement 200 ms debounce/cleanup, then GREEN**

```bash
pnpm vitest run src/hooks/useLibraryQuery.test.tsx -t "debounce|literal|stale|loading|unmount"
```

Use one effect-local 200 ms timeout and clear it on change/unmount. Keep result generation separate
from loading ownership. Rerun the entire hook file; expected PASS.

- [ ] **Step 9: Commit**

```bash
git add src/hooks/useLibraryQuery.ts src/hooks/useLibraryQuery.test.tsx
git commit -m "feat(library-ui): add bounded library query state"
```

---

### Task 3: Add Favorites and Metadata/Scan Invalidation

**Files:**
- Modify: `src/hooks/useLibraryQuery.ts`
- Test: `src/hooks/useLibraryQuery.test.tsx`

**Interfaces:** Extend Task 2 behavior without new IPC types.

- [ ] **Step 1: Write favorite tests and verify RED**

Cover toggle on/off, mutation rejection, two calls while one mutation is pending, summary callback,
and unfavorite on page two under favorites-only. Assert exact payloads and that page DTO state never
changes optimistically.

```bash
pnpm vitest run src/hooks/useLibraryQuery.test.tsx -t "favorite"
```

Expected: FAIL.

- [ ] **Step 2: Implement authoritative favorite behavior and verify GREEN**

Track pending IDs in a ref for synchronous duplicate suppression and state for rendering. Call:

```ts
setGameFavorite({ gameId: item.gameId, favorite: !item.favorite });
```

On success refetch bounded state and call `onFavoriteCommitted`; under favorites-only unfavorite,
reset to offset zero. On failure retain the page and expose `favoriteError`. Always release the ID
after mounted completion. Rerun the focused tests; expected PASS.

- [ ] **Step 3: Write metadata listener lifecycle/cadence tests and verify RED**

Capture real listener handlers/unlisten spies. Assert visible IDs 1, 1, and 2 coalesce to one request
after 180 ms; off-page 99 causes none; repeated IDs deduplicate; unmount clears timer/unlistens; a
listener registration resolving after unmount immediately unlistens; late rejection writes nothing.

```bash
pnpm vitest run src/hooks/useLibraryQuery.test.tsx -t "metadata invalidation|listener"
```

Expected: FAIL.

- [ ] **Step 4: Implement visible-page coalescing and verify GREEN**

Keep visible IDs in a ref. In the listener effect, retain affected IDs in an effect-owned `Set` and
reset one 180 ms timer. Ignore off-page IDs. Timer expiry clears the set and refetches the current
page once. Cleanup marks disposal, clears timer/set, and unregisters; late registration unregisters
itself. Rerun; expected PASS.

- [ ] **Step 5: Write/implement scan completion identity tests**

Rerender with completion run 31: one refresh. Rerender with 31 again: none. Rerender with 32: one.
The interface contains no scan-progress input, structurally preventing progress refresh. Run the
whole hook suite; expected PASS.

- [ ] **Step 6: Commit**

```bash
git add src/hooks/useLibraryQuery.ts src/hooks/useLibraryQuery.test.tsx
git commit -m "feat(library-ui): add favorites and bounded invalidation"
```

---

### Task 4: Implement GameCard and C4 Cover Fallback

**Files:**
- Create: `src/features/library/GameCard.tsx`
- Create: `src/features/library/GameCard.test.tsx`
- Modify: `src/styles/index.css`

**Interfaces:**

```ts
interface GameCardProps {
  item: LibraryListItem;
  systemName: string;
  accent: string;
  favoritePending: boolean;
  onToggleFavorite(item: LibraryListItem): void;
}
```

- [ ] **Step 1: Write card semantics/state tests and verify RED**

Assert metadata display title, local fallback, system, release year, favorite accessible label/
`aria-pressed`/pending disabled state, one interactive descendant, unavailable content label, stale
metadata retaining title/cover, and failed metadata not implying local unavailability.

```bash
pnpm vitest run src/features/library/GameCard.test.tsx -t "card|favorite|availability|stale"
```

Expected: FAIL because the component is absent.

- [ ] **Step 2: Implement semantic card content and verify GREEN**

Use an `<article aria-labelledby>` with heading and a separate favorite `<button>`. Do not add card
navigation or detail IPC. Show matched silently; use concise pending/no-match/ambiguous/deferred/
failed/stale labels. Derive year only from a leading four-digit release date. Rerun; expected PASS.

- [ ] **Step 3: Write cover fallback tests and verify RED**

Assert a non-null `coverRef` is used unchanged by `img loading="lazy"`. Fire `error` and assert an
accessible `No cover available for <title>` placeholder replaces it. With null cover, assert the
placeholder exists immediately, uses the passed accent, and retains a long title.

```bash
pnpm vitest run src/features/library/GameCard.test.tsx -t "cover|placeholder"
```

Expected: FAIL.

- [ ] **Step 4: Implement covers/C4/CSS and verify GREEN**

Reset local `coverFailed` when `coverRef` changes. Reserve 3:4 ratio. Use the opaque reference only;
404 is local fallback, not query failure. Add token-only card/cover/placeholder/favorite/badge styles,
hard edges, `--shadow-rest`, hover scale 1.04, focus-within scale 1.08/`--shadow-focus`, title clamp,
and wrapped centered Press Start 2P placeholder text. Run the whole card suite; expected PASS.

- [ ] **Step 5: Commit**

```bash
git add src/features/library/GameCard.tsx src/features/library/GameCard.test.tsx src/styles/index.css
git commit -m "feat(library-ui): add accessible game cards"
```

---

### Task 5: Integrate Header Search, Sidebar/Bar Filters, Grid, States, and Paging

**Files:**
- Create: `src/features/library/LibraryBrowser.tsx`
- Modify: `src/features/library/LibraryPage.tsx`
- Modify: `src/app/AppShell.tsx`
- Modify: `src/app/AppShell.test.tsx`
- Modify: `src/styles/index.css`

**Interfaces:** `AppShell` owns the terminal scan run ID, calls `useLibraryQuery`, and uses the
hook-owned search/system/favorites state across the header/sidebar/content layout. `LibraryBrowser`
consumes that model and `SystemLabel[]`.

- [ ] **Step 1: Write populated/error/retry integration tests and verify RED**

Extend IPC mocks with `queryLibrary`, `setGameFavorite`, and `onMetadataStateChanged`. Expect real
fixture titles, absence of `LIBRARY READY`, and no empty-library CTA. Reject initial query and assert
shell/sidebar plus retry remain; retry loads cards.

```bash
pnpm vitest run src/app/AppShell.test.tsx -t "populated|library query"
```

Expected: FAIL because the transitional state remains.

- [ ] **Step 2: Wire hook and replace transitional populated state**

Enable queries only for Library route with summary total greater than zero. Pass `refreshSummary` as
favorite callback. Scan completion refreshes summary and updates one terminal run ID. Delete
`PopulatedLibraryState`; render `LibraryBrowser`. Keep empty/scan/issues behavior unchanged.

Add a header `input type="search"` with visible `SEARCH LIBRARY` label, current value, subtle busy
copy, and clear button. Convert sidebar rows to backend-ID filters through `model.setSystemId`;
selecting one also navigates to Library. Use `aria-pressed` for filter state, not `aria-current`.

- [ ] **Step 3: Implement initial/error/loading browser and verify GREEN**

Initial loading uses six geometry-only skeleton cards. Query errors use `InlineError`/retry and keep
prior data if available. Successful pages render `GameCard`s only. Run Step 1; expected PASS.

- [ ] **Step 4: Write search/system/favorite/reset tests and verify RED**

Assert All Systems and a backend `nes` selection, summary counts, unknown ID, 200 ms search debounce,
literal query, clear search, FAVORITES `aria-pressed`, query offset reset, RESET clearing all inputs,
and catalog error remaining visible without invented rows.

```bash
pnpm vitest run src/app/AppShell.test.tsx -t "search|system filter|favorites filter|reset filters"
```

Expected: FAIL.

- [ ] **Step 5: Implement the supported B3 bar and verify GREEN**

Render `// FILTER`, FAVORITES, active-filter summary, and RESET. Search stays in the B2-style header;
system filter stays in B1 sidebar. Do not render genre, region, unplayed, recent, core, BIOS, or
readiness controls. Rerun; expected PASS.

- [ ] **Step 6: Write pagination/no-results/favorite integration tests**

For total 61, NEXT queries offset 60 and renders one final item with NEXT disabled/PREVIOUS enabled;
PREVIOUS returns offset zero. For `items: []`/`total: 0` with active inputs, assert B5 copy, query
echo/context, RESET action, and no first-run folder CTA. Favorite tests assert exact mutation,
duplicate suppression, error preserving `aria-pressed`, and favorites-only unfavorite removing the
card through an offset-zero refetch.

- [ ] **Step 7: Implement controls/states and verify complete integration GREEN**

Use native buttons named `PREVIOUS PAGE`/`NEXT PAGE`, a polite page summary, a dashed B5 panel, and
one reset action. Run:

```bash
pnpm vitest run src/app/AppShell.test.tsx
```

Expected: existing M6.2 plus new M6.3 tests PASS.

- [ ] **Step 8: Add responsive/token CSS**

Add header search, filter bar, grid (`repeat(auto-fill, minmax(158px, 1fr))`), query status,
no-results, pagination, and skeleton styles. Controls wrap without overflow; retain the 232 px
sidebar at 960 px and legible grid columns. Extend the existing compact breakpoint so search remains
reachable. Use tokens only and preserve reduced-motion behavior.

- [ ] **Step 9: Commit**

```bash
git add src/app/AppShell.tsx src/app/AppShell.test.tsx src/features/library/LibraryBrowser.tsx src/features/library/LibraryPage.tsx src/styles/index.css
git commit -m "feat(library-ui): implement M6 library browsing"
```

---

### Task 6: Pin App-Level Invalidation and Scan Cadence

**Files:**
- Modify: `src/app/AppShell.test.tsx`
- Modify only if exposed: `src/app/AppShell.tsx`, `src/hooks/useLibraryQuery.ts`

- [ ] **Step 1: Write cadence tests**

On a visible page with IDs 1/2, dispatch metadata events 1, 1, 2, 99 before 180 ms; assert zero
immediate calls and exactly one current-page query afterward. Assert metadata unlisten on unmount
and safe late registration. Dispatch several scan progress events: zero query calls. Dispatch one
terminal completion: exactly one query. Exercise same-run command result and assert dedupe.

- [ ] **Step 2: Verify RED or mutation-valid existing GREEN**

```bash
pnpm vitest run src/app/AppShell.test.tsx -t "metadata invalidation|visible library page once"
```

If already green, prove the tests fail when the corresponding refresh signal is temporarily removed,
then restore it. If red, fix only missing wiring; do not duplicate hook logic in AppShell.

- [ ] **Step 3: Run all frontend tests and commit**

```bash
pnpm test
git add src/app/AppShell.test.tsx src/app/AppShell.tsx src/hooks/useLibraryQuery.ts
git commit -m "test(library-ui): cover refresh invalidation cadence"
```

Expected: all frontend tests PASS with no warnings. Omit unchanged production files from `git add`.

---

### Task 7: Documentation and Manual Design Inspection

**Files:**
- Modify: `docs/M6_REPORT.md`
- Modify: `BACKLOG.md`
- Modify: `README.md`
- Modify: `docs/DEVELOPMENT.md`

- [ ] **Step 1: Update current-state docs**

Mark M6.3/library UI/GameCard/search/system filters/favorites complete in BACKLOG while leaving
details/readiness/M6.4+ unchecked. Update README/DEVELOPMENT from future to implemented bounded
browsing. In M6_REPORT section E record architecture, request/loading identity, paging, 200 ms
search, filters, authoritative favorites, genre/region deferral, cards/covers, availability/metadata,
coalesced invalidation, scan cadence, accessibility, performance, tests, and deferrals.

- [ ] **Step 2: Perform honest manual design inspection**

Using synthetic fixture state, inspect 960×640, 1280×800, and larger desktop; dark/light; populated
grid; partial final page; long title; missing/failed cover; no results; system/favorite filters;
query/favorite errors; unavailable local content; stale/failed metadata. Record the exact environment
and whether inspection was fixture-based or native. Do not claim an automated visual detector.

- [ ] **Step 3: Check documentation/diff**

```bash
pnpm exec prettier --check docs/DEVELOPMENT.md
git diff --check
```

Expected: PASS.

---

### Task 8: Full Verification, Scope Audit, and Final Commit

**Files:** Finalize `docs/M6_REPORT.md`; modify code only for failures caused by M6.3.

- [ ] **Step 1: Run frontend verification**

```bash
pnpm typecheck
pnpm lint
pnpm format:check
pnpm test
pnpm build
```

Expected: all exit 0; record exact test and build results.

- [ ] **Step 2: Run Rust verification**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml --release
```

Expected: all exit 0; record counts/results.

- [ ] **Step 3: Run desktop/diff checks**

```bash
pnpm tauri:build
git diff --check
```

Expected: PASS.

- [ ] **Step 4: Audit scope/prohibited implementation**

```bash
rg -n "getLibrarySnapshot|getLibraryGameDetail|getGameMetadata" src --glob '!platform/ipc.ts' --glob '!platform/ipc.test.ts'
rg -n "https?://|base64|crc32|md5|sha1|fingerprint" src/features/library src/hooks/useLibraryQuery.ts
git status --short
```

Expected: no browsing call site for snapshot/detail/metadata; no provider URL/base64/hash handling;
only intended tracked changes plus preserved untracked review artifacts.

- [ ] **Step 5: Mark complete only after every gate**

Set M6_REPORT verdict exactly to:

```text
M6.3 IMPLEMENTATION COMPLETE — ready for review before M6.4
```

Keep overall M6 incomplete and state M6.4 has not started.

- [ ] **Step 6: Final commit and repository capture**

```bash
git add BACKLOG.md README.md docs/DEVELOPMENT.md docs/M6_REPORT.md src
git commit -m "docs(m6): finalize library browsing report"
git rev-parse HEAD
git log --oneline 28e20dab..HEAD
git status --short --branch
```

Do not stage review artifacts. Use exact results in the required A–N report, state nothing was
pushed/merged, and stop without beginning M6.4.
