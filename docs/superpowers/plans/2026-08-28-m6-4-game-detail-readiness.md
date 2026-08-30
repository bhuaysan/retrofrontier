# M6.4 Game Detail and Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a bounded, informational `/games/:id` detail screen that presents local content, normalized M5 metadata, and Rust-authoritative system readiness without launching a game.

**Architecture:** Extend the existing hand-rolled history layer with a safe discriminated game route. Retain full `SystemStatus` objects in `useSystemCatalog` and feed the active game's system status to a focused `useGameDetail` hook, which independently loads the M6.1 local detail and M5 metadata state and owns targeted refresh events. Cards use a sibling title link and Favorite button, while the detail page renders B6/C1/C4-aligned sections with local availability, metadata, and readiness kept separate.

**Tech Stack:** React 19, TypeScript 6, Vitest, Testing Library, existing typed Tauri IPC wrappers, CSS design tokens, and the existing Rust `get_systems`, `get_library_game_detail`, and `get_game_metadata` contracts.

**Spec:** `docs/superpowers/specs/2026-08-28-m6-4-game-detail-readiness-design.md`

## Global Constraints

- Work only on `feat/m6-library-ui`; preserve the exact untracked historical review artifacts; do not push, merge, or rewrite prior M6 commits.
- Starting implementation HEAD is `622614298c010ca6aa3a38c8f80624019f338bd8`; the design checkpoint is `fee9eef`.
- Reuse `get_systems` for one coherent Rust-verified runtime/core/BIOS/system-readiness snapshot; do not add a parallel backend readiness DTO.
- Detail local data comes only from `get_library_game_detail`; metadata comes only from `get_game_metadata`; never use `get_library_snapshot` or library-wide metadata queries.
- React displays normalized readiness; it does not select cores, identify BIOS files, verify runtime artifacts, scan paths, or launch RetroArch.
- Keep Game, Content Unit, Content File, and Content Root distinct; do not display local hashes, fingerprints, provider payloads, authenticated URLs, credentials, job queues, or candidate internals.
- Omit Launch/Play, save states, screenshots, controller footer/focus graph, metadata candidate selection, provider settings, and metadata refresh management.
- Use synthetic/legal fixtures only and follow red-green-refactor: every production behavior change starts with a failing test that demonstrates the missing behavior.
- Run the required frontend and Rust verification commands before claiming completion; do not claim Windows/macOS validation.

---

### Task 1: Extend the safe route/history contract

**Files:**
- Create: `src/app/routes.test.ts`
- Modify: `src/app/routes.ts`

**Interfaces:**

```ts
export type GameRoute = {
  kind: 'game';
  gameId: number | null;
  rawId: string;
};

export type AppRoute = 'library' | 'settings' | GameRoute;
export type AppRoutePath = '/library' | '/settings' | `/games/${string}`;

export function gameRoute(gameId: number): GameRoute;
export function routeFromPath(pathname: string): AppRoute;
export function routePath(route: AppRoute): AppRoutePath;
export function isGameRoute(route: AppRoute): route is GameRoute;
```

- [ ] **Step 1: Write route parsing and canonicalization tests**

Assert `/library` and `/settings` retain their current routes, `/games/42` produces `{ kind:
'game', gameId: 42, rawId: '42' }`, and `gameRoute(42)` produces the canonical valid route. Assert
that `/games/`, `/games/abc`, `/games/0`, `/games/-1`, a decimal, and an integer larger than
`Number.MAX_SAFE_INTEGER` produce an invalid game route with `gameId: null` and never throw. Assert
that `/unknown` still maps to `library`.

```ts
it('keeps malformed game paths in a safe invalid-detail route', () => {
  expect(routeFromPath('/games/not-an-id')).toEqual({
    kind: 'game',
    gameId: null,
    rawId: 'not-an-id',
  });
});
```

- [ ] **Step 2: Run the route tests and verify the correct RED failure**

Run `pnpm vitest run src/app/routes.test.ts`. Expected: FAIL because the current route union only
knows `library` and `settings` and does not parse game paths.

- [ ] **Step 3: Implement the route helpers and history navigation**

Keep `/library` as the normalization target for unrelated unknown paths. Treat only a single
segment below `/games/` as the game ID, accept digits only when the result is a positive safe
integer, and retain malformed segments as invalid detail routes without interpolating them into
rendered HTML. Update `useRoute` so `pushState`, `popstate`, and same-path guards work for string
routes and game route objects; valid game routes use `/games/<safe integer>` and invalid routes use
an encoded invalid segment.

- [ ] **Step 4: Run the route tests and verify GREEN**

Run `pnpm vitest run src/app/routes.test.ts`. Expected: all route parsing and canonicalization tests
PASS.

---

### Task 2: Preserve the full system readiness projection

**Files:**
- Create: `src/hooks/useSystemCatalog.test.tsx`
- Modify: `src/hooks/useSystemCatalog.ts`

**Interfaces:**

```ts
export interface SystemCatalogModel {
  systems: SystemLabel[];
  statuses: SystemStatus[];
  loading: boolean;
  error: IpcError | null;
  refresh: () => Promise<SystemLabel[] | null>;
}
```

The existing `systems` labels remain unchanged for the sidebar. `statuses` contains the exact
`SystemStatus[]` returned by `getSystems()` and is cleared together with labels on a failed
refresh.

- [ ] **Step 1: Write tests for atomic status retention and failure**

Mock `getSystems` at the platform boundary. Assert a successful refresh exposes both the existing
label projection and the complete `SystemStatus` readiness/core/BIOS object. Hold a second refresh
pending and assert the hook reports loading while the old data is cleared, then reject it and assert
both arrays are empty with the normalized error. Add a successful retry assertion.

- [ ] **Step 2: Run the catalog tests and verify RED**

Run `pnpm vitest run src/hooks/useSystemCatalog.test.tsx`. Expected: FAIL because the hook currently
discards the full status objects and does not expose a `statuses` collection.

- [ ] **Step 3: Retain and return the authoritative system statuses**

Store `response.systems` before mapping labels. Preserve the current loading/error lifecycle and
clear both projections together on failure. Do not add a second IPC call or derive readiness in
TypeScript.

- [ ] **Step 4: Run catalog and existing shell tests**

Run `pnpm vitest run src/hooks/useSystemCatalog.test.tsx src/app/AppShell.test.tsx`. Expected: all
tests PASS with existing sidebar behavior unchanged.

---

### Task 3: Make cards semantically navigable and share cover fallback behavior

**Files:**
- Create: `src/features/library/GameCover.tsx`
- Create: `src/features/library/GameCover.test.tsx`
- Modify: `src/features/library/GameCard.tsx`
- Modify: `src/features/library/GameCard.test.tsx`
- Modify: `src/features/library/LibraryBrowser.tsx`
- Modify: `src/styles/index.css`

**Interfaces:**

```ts
interface GameCoverProps {
  title: string;
  coverRef: string | null;
  accent: string;
  resetKey: object;
  loading?: 'lazy' | 'eager';
  className?: string;
}

interface GameCardProps {
  item: LibraryListItem;
  systemName: string;
  accent: string;
  favoritePending: boolean;
  onOpenGame: (gameId: number) => void;
  onToggleFavorite: (item: LibraryListItem) => void;
}
```

- [ ] **Step 1: Write shared cover and card semantics tests**

Assert a cover reference renders an image, a missing reference renders the C4 system-accent
placeholder, an image error falls back without another automatic request, and a new authoritative
`resetKey` allows the same reference to be tried again. Update card tests to assert the title is an
`/games/<id>` link, Favorite remains a separate button, the card has no nested interactive
descendants, and a normal link activation calls `onOpenGame` without invoking Favorite.

```ts
expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveAttribute(
  'href',
  '/games/1',
);
expect(screen.getAllByRole('button')).toHaveLength(1);
```

- [ ] **Step 2: Run focused UI tests and verify RED**

Run `pnpm vitest run src/features/library/GameCover.test.tsx src/features/library/GameCard.test.tsx`.
Expected: FAIL because the card has no detail link and cover rendering is still embedded in the
card.

- [ ] **Step 3: Extract the cover component and add the sibling detail link**

Move the current opaque-reference/error fallback behavior into `GameCover`. Use `resetKey` object
identity to clear a prior image failure when a new authoritative DTO arrives, while keeping the
failed reference on the same DTO as a normal fallback. Put the title link inside the card copy,
outside the Favorite button, and preserve normal modified-click browser behavior. Have
`LibraryBrowser` pass the navigation callback through without changing its bounded page query.

- [ ] **Step 4: Add only detail-link styles and verify GREEN**

Style the link as the existing card title, keep card focus-within zoom/shadow behavior, and ensure
the link uses the existing A6 keyboard treatment without adding a nested interactive wrapper. Run
the focused UI test command from Step 2; expected: all tests PASS.

---

### Task 4: Build the independent bounded detail-state hook

**Files:**
- Create: `src/hooks/useGameDetail.ts`
- Create: `src/hooks/useGameDetail.test.tsx`

**Interfaces:**

```ts
export interface GameDetailModel {
  local: LibraryGameDetail | null;
  metadata: GameMetadataState | null;
  localLoading: boolean;
  metadataLoading: boolean;
  localLoaded: boolean;
  metadataLoaded: boolean;
  localError: IpcError | null;
  metadataError: IpcError | null;
  favoritePending: boolean;
  favoriteError: IpcError | null;
  retryLocal: () => Promise<void>;
  retryMetadata: () => Promise<void>;
  refreshLocalAndMetadata: () => Promise<void>;
  toggleFavorite: () => Promise<void>;
}

interface UseGameDetailOptions {
  enabled: boolean;
  gameId: number | null;
  scanCompletionRunId: number | null;
  onFavoriteCommitted?: () => void | Promise<void>;
}
```

- [ ] **Step 1: Write initial loading, success, null, and partial-failure tests**

Mock `getLibraryGameDetail`, `getGameMetadata`, and `setGameFavorite` only through the IPC module.
Assert initial valid detail starts both loading channels and issues exactly one bounded call per
source. Resolve local while rejecting metadata and assert local remains available, metadata receives
a normalized error, and no provider error text is rendered by the hook model. Resolve metadata while
local returns `null` and assert `localLoaded` plus a stable null result. Assert retrying only the
failed channel leaves the successful channel untouched.

- [ ] **Step 2: Run the hook tests and verify RED**

Run `pnpm vitest run src/hooks/useGameDetail.test.tsx -t "loading|partial|not found|retry"`.
Expected: FAIL because `useGameDetail` does not exist.

- [ ] **Step 3: Implement independent request generations and retries**

Use separate local and metadata request generations plus mounted guards. Retain successful data on
refresh failure, clear errors only for the channel being retried, and treat a successful `null`
local result as not found. `refreshLocalAndMetadata` starts both bounded calls independently; no
single rejection may blank the other channel.

- [ ] **Step 4: Run the initial hook tests and verify GREEN**

Run the Step 2 command; expected: PASS.

- [ ] **Step 5: Write metadata invalidation and scan-run tests**

Capture the real metadata listener callback and unlisten function. Assert an unrelated game event
does not refetch, one current-game event refetches after 180 ms, repeated current-game events
coalesce, and unmount clears the timer and listener. Resolve registration after unmount and assert
the late unlisten runs; reject late registration and assert no state update. Rerender with scan run
31 and assert one bounded refresh, with run 31 again no refresh, and with run 32 one refresh. The
hook accepts terminal run IDs only, so scan progress has no detail side effect.

- [ ] **Step 6: Run event tests and verify RED**

Run `pnpm vitest run src/hooks/useGameDetail.test.tsx -t "metadata invalidation|scan completion|cleanup"`.
Expected: FAIL because the listener and terminal refresh behavior are not implemented.

- [ ] **Step 7: Implement targeted/coalesced refresh and cleanup**

Use an effect-owned timer and disposal flag. Filter by `gameId`, use the existing 180 ms trailing
debounce, call only `getGameMetadata` for metadata invalidation, and call both detail sources once
for a new terminal scan run. Unregister immediately when an asynchronously resolved listener is
late. Keep scan work bounded and event-driven with no polling.

- [ ] **Step 8: Write Favorite authority tests, implement, and verify GREEN**

Assert one mutation at a time, exact payload `{ gameId, favorite: !local.favorite }`, no optimistic
state change before the backend response, response state committed after success, summary callback
triggered without making a successful favorite look failed when summary refresh rejects, and the
prior state retained with a retryable error on mutation rejection. Run the entire hook file and
expect PASS.

---

### Task 5: Implement the B6/C1/C4 detail and readiness view

**Files:**
- Create: `src/features/library/GameDetailPage.tsx`
- Create: `src/features/library/GameDetailPage.test.tsx`
- Create: `src/features/library/readinessCopy.ts`
- Create: `src/features/library/readinessCopy.test.ts`
- Modify: `src/styles/index.css`

**Interfaces:**

```ts
interface GameDetailPageProps {
  gameId: number | null;
  systemStatus: SystemStatus | null;
  systemLoading: boolean;
  systemError: IpcError | null;
  refreshSystems: () => Promise<unknown>;
  scanCompletionRunId: number | null;
  onBackToLibrary: () => void;
  onFavoriteCommitted?: () => void | Promise<void>;
}

export function readinessReasonText(reason: ReadinessReason): string;
export function runtimeStatusText(state: RuntimeState): string;
export function biosStatusText(status: SystemBiosStatus): string;
```

- [ ] **Step 1: Write normalized readiness-copy tests**

Use synthetic `ReadinessReason`, runtime, core, and BIOS fixtures. Assert every current reason maps
to user-readable text without returning a provider payload, filesystem path, research-internal
detail, or hash. Assert runtime states distinguish usable, missing, transitional, and broken;
required BIOS missing/invalid/not-covered differ from `notRequired`; and unresolved core policy is
not shown as an available core.

- [ ] **Step 2: Run readiness-copy tests and verify RED**

Run `pnpm vitest run src/features/library/readinessCopy.test.ts`. Expected: FAIL because the
readiness copy helpers do not exist.

- [ ] **Step 3: Implement readiness copy helpers from existing DTOs**

Map only the normalized discriminated unions already returned by Rust. Use `SystemReadiness.ready`
for the overall emulation state; do not recompute readiness from individual rows. Treat runtime
`ready` and `rollbackAvailable` as usable because that is the existing domain rule, and explain
rollback availability without exposing installation paths. Treat BIOS identity not covered as
unverified rather than present.

- [ ] **Step 4: Run readiness-copy tests and verify GREEN**

Run the Step 2 command; expected: PASS.

- [ ] **Step 5: Write detail rendering tests for hierarchy and independent states**

Mock the platform boundary and render the real detail hook through `GameDetailPage`. Cover loading;
valid local detail with one unit; multiple units including CUE/BIN and M3U labels; missing content;
metadata matched, stale with cached title/cover, pending, no-match, ambiguous, deferred, and
failed; metadata error while local/readiness remain; readiness all-satisfied, missing BIOS, runtime
unavailable, core unavailable/unresolved, and readiness API/catalog error; local null after refresh;
Favorite; retry buttons; and absence of Launch/save-state/controller controls.

Assert one detail `h1`, visible readiness text, no hashes/fingerprints/provider IDs/jobs/candidates
in the rendered output, no `READY TO PLAY` wording, and no full-library/snapshot IPC call.

- [ ] **Step 6: Run detail tests and verify RED**

Run `pnpm vitest run src/features/library/GameDetailPage.test.tsx`. Expected: FAIL because the
detail page and layout do not exist.

- [ ] **Step 7: Implement the detail page and page-level focus**

Render a semantic `<main>` with a focusable `h1`, real `/library` back link, B6 hero grid, shared
`GameCover` with eager detail loading, effective metadata/local title, normalized metadata rows,
metadata state panel, four independent readiness rows, local content-unit summaries, and bounded
retry/error surfaces. Keep local failure/not-found page-level while retaining unrelated data on
metadata/readiness failure. Use Settings navigation only as a normal route link for missing BIOS;
do not open arbitrary folders. Focus the heading on valid route entry, first local result/error, or
invalid/not-found state.

- [ ] **Step 8: Add responsive/token styles and verify GREEN**

Use existing tokens, hard borders/shadows, Press Start 2P/VT323/Space Grotesk, system accent
placeholder, readable light-theme foreground/background combinations, 3:4 cover, hero collapse at
1080px, and wrapping/ellipsis rules for long content. Do not add gradients or rounded controls. Run
`pnpm vitest run src/features/library/readinessCopy.test.ts src/features/library/GameDetailPage.test.tsx`;
expected: all focused detail tests PASS.

---

### Task 6: Wire routes, scan refresh, Favorite, and return focus in AppShell

**Files:**
- Modify: `src/app/AppShell.tsx`
- Modify: `src/features/library/LibraryPage.tsx`
- Modify: `src/features/library/LibraryBrowser.tsx`
- Modify: `src/hooks/useLibraryQuery.ts`
- Modify: `src/hooks/useLibraryQuery.test.tsx`
- Modify: `src/app/AppShell.test.tsx`

**Interfaces:**

```ts
interface LibraryPageProps {
  // existing props
  onOpenGame: (gameId: number) => void;
  restoreFocusGameId: number | null;
  onFocusRestored: () => void;
}
```

- [ ] **Step 1: Write the library context-preservation regression**

Extend the existing hook test to load a page at offset 60, disable the hook for a detail route, then
re-enable it. Assert the next query uses offset 60 and the committed page is not reset to offset 0
unless the query identity changed. Keep existing M6.3 generation/loading-owner assertions intact.

- [ ] **Step 2: Run the regression and verify RED**

Run `pnpm vitest run src/hooks/useLibraryQuery.test.tsx -t "preserve.*offset|detail"`. Expected:
FAIL because the current enable effect always requests offset zero.

- [ ] **Step 3: Preserve the committed library offset on re-enable**

Change only route-driven re-enable behavior: use the latest committed/target offset when an existing
page is present, while search/filter changes still set target offset zero. Do not alter M6.3
metadata invalidation rules or add detail calls to the grid hook.

- [ ] **Step 4: Run the hook suite and verify GREEN**

Run `pnpm vitest run src/hooks/useLibraryQuery.test.tsx`; expected: all existing and new context
tests PASS.

- [ ] **Step 5: Write AppShell route/deep-link/back/focus tests**

Add detail and metadata read mocks plus a complete system response. Assert a card title click changes
the path to `/games/1`, renders the detail heading, leaves Favorite independent, and explicit back
returns to `/library` with the originating title link focused when visible. Assert browser Back has
the same result, direct `/games/1` renders without visiting Library first, `/games/nope` renders
invalid-detail copy without detail IPC, and `/games/999` renders game-not-found after null local
detail. Assert terminal scan completion refreshes the system catalog once and detail bounded data
once, while a progress event does not refresh detail. Assert no `get_library_snapshot` call occurs.

- [ ] **Step 6: Run AppShell tests and verify RED**

Run `pnpm vitest run src/app/AppShell.test.tsx -t "detail|game route|focus|deep link"`. Expected:
FAIL because AppShell still renders only Library/Settings and cards have no navigation callback.

- [ ] **Step 7: Wire the route and shared state without adding a router**

Keep `useLibraryQuery` mounted in AppShell. Record the originating game ID only when activating a
card detail link; clear it for unrelated Library/Settings navigation. Render `GameDetailPage` for
valid and invalid game routes, pass the matching `systemCatalog.statuses` object and catalog retry
callback, pass the one terminal scan run ID, and refresh the system catalog from the existing
deduplicated scan-completion callback. Use a keyed detail page for a changed valid ID.

- [ ] **Step 8: Implement return-focus restoration and verify GREEN**

On Library re-entry, focus the recorded visible game detail link and clear the record; if the game
is no longer visible, focus the Library heading instead. Ensure the heading and links remain
keyboard-usable when the target disappears. Run `pnpm vitest run src/app/AppShell.test.tsx
src/hooks/useLibraryQuery.test.tsx`; expected: all route, focus, scan-refresh, and existing M6.3
tests PASS.

---

### Task 7: Integrate documentation, perform static design audit, and verify

**Files:**
- Modify: `docs/M6_REPORT.md`
- Modify only if current behavior statements require it: `BACKLOG.md`, `README.md`, `docs/DEVELOPMENT.md`

- [ ] **Step 1: Update the living M6 report**

Record exact starting HEAD, design/spec and implementation commits, M6.3 corrective completion,
M6.4 route model, the two detail IPC calls, reuse of `get_systems`, Rust readiness authority, local/
metadata/readiness separation, content-unit treatment, cover boundary, targeted event cleanup,
terminal scan behavior, focus/context behavior, design coverage, tests, verification, and explicit
M6.5/M6.6/M7/M8/M9 deferrals. State that no backend readiness projection was added. Do not modify
`M6_3_REVIEW.md` or any other historical review artifact.

- [ ] **Step 2: Run frontend quality checks**

Run each command separately and record the exact result:

```bash
pnpm typecheck
pnpm lint
pnpm format:check
pnpm test
pnpm build
```

Expected: each command exits 0 with the complete frontend suite/build passing.

- [ ] **Step 3: Perform the static design-fidelity audit**

Inspect source/layout behavior against B6, C1, C4, A6, and `tokens.css` at 960×640, 1280×800, and
larger desktop proportions, in dark and light themes. Check long title/synopsis/path, no-cover,
unavailable content, stale metadata, metadata failure, missing BIOS, runtime unavailable, and
unresolved core policy. No rendered native screenshot harness is present, so record static/source
inspection and do not claim screenshot validation.

- [ ] **Step 4: Run Rust and packaging verification**

Run each command separately and record the exact result:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml --release
pnpm tauri:build
git diff --check
```

Expected: each command exits 0. The Rust suite is required even though M6.4 adds no Rust code.

- [ ] **Step 5: Run the changed-web manual detector and inspect the worktree**

Run the Impeccable detector once over every changed web UI target, inspect any warning, and then
run `git status --short --branch`, `git diff --stat`, and `git diff --check`. Confirm historical
untracked review files remain untracked and unmodified, no prohibited binary/secret/build artifact
is present, and only intended M6.4 changes are tracked.

- [ ] **Step 6: Commit the coherent implementation**

Stage only intended M6.4 implementation files and `docs/M6_REPORT.md`, leaving historical
untracked review artifacts alone, then create one coherent commit:

```bash
git add src docs/M6_REPORT.md
git commit -m "feat(game-detail): implement M6 detail and readiness UI"
```

If verification requires a narrowly scoped correction, use a second `fix(game-detail):` commit; do
not amend or rewrite prior M6 history.

