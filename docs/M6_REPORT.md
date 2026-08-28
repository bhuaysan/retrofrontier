# M6 Library UI Implementation Report

## A. Repository State

- Starting M6.4 HEAD: `622614298c010ca6aa3a38c8f80624019f338bd8`
- Starting commit: `fix(library-ui): stabilize M6.3 pagination refresh state`
- Starting M6.2 HEAD: `59b10effa6afd80addf5b53ef7a684bfc4e3bccf`
- Branch: `feat/m6-library-ui`
- Main comparison at start: local `main` and `origin/main` were both
  `9a1a1e3d8c38633c1c82bc95293c6a6024e94e93`; no rebase was required.
- Original M6.1 implementation commits remain `1cdf5a2` and `e7f3df8`; they were not rewritten.
- Final M6.4 HEAD: recorded after the focused implementation commit in the final repository
  handoff below.
- M6.2 implementation commit: `8a74438158f221464a972fb89c7774aa2e48f2c3`
- M6.2 corrective commit: `fix(ui): address M6.2 adversarial review findings`
- Merged to main: No
- Pushed: No
- Pre-existing untracked files: `M3_REVIEW.md`, `M4_REVIEW.md`, `M4_REVIEW_2.md`,
  `M4_REVIEW_3.md`, `M5_REVIEW.md`, `M6_1_REVIEW.md`, `M6_1_DELTA_REVIEW.md`, `M6_2_REVIEW.md`,
  `M6_2_DELTA_REVIEW.md`, `M6_3_REVIEW.md`, `docs/M5_IMPLEMENTATION_REPORT.md`

## B. Overall M6 Status

- [x] M6.1 Backend Enablement
- [x] M6.2 Shell / Empty Library / Scan UX
- [x] M6.3 Library Browsing
- [x] M6.4 Game Detail / Readiness
- [ ] M6.5 Metadata UX / Settings
- [ ] M6.6 Hardening / Accessibility / Documentation

Current phase: M6.1, M6.2, and M6.3 are complete. M6.3's adversarial review found 0 CRITICAL and
0 HIGH findings; its focused corrective pass is complete and was not reopened for M6.4.
M6.4 implementation is complete on `feat/m6-library-ui` and is awaiting review before M6.5.

## C. M6.1 — Backend Enablement

### Implemented

- Added bounded `query_library`, `get_library_summary`, and `get_library_game_detail` application,
  repository, Tauri, and TypeScript contracts.
- Added title search with normalized metadata/local-title fallback, system/favorite/genre/region/
  availability filters, deterministic title ordering, total counts, a hard list limit of 60, and
  literal escaping for `\`, `%`, and `_` through SQL `ESCAPE` clauses shared by count and page.
- Added durable user-owned `game_user_state` favorites with a SQLite boolean check, FK ownership,
  set/clear commands, list/detail composition, and explicit delete restriction.
- Added the narrow target-aware cached-media reference and custom protocol. Linux/macOS desktop use
  `rfmedia://localhost/cover/<game-id>`; Windows uses `http://rfmedia.localhost/cover/<game-id>`.
  Rust resolves the durable media row, enforces cache containment, rejects traversal/absolute/symlink
  escapes, validates PNG/JPEG/WebP content, and never exposes the persisted cache path to React.
  The existing CSP already permits both image origins; no filesystem permission was widened.
- Added `metadata-state-changed` as a minimal durable-state invalidation event carrying only game and
  provider identity.
- Added bounded persisted-scan issue paging with total count, offset, default limit 50, hard limit
  100, deterministic newest-first ordering, and one resolved `scanRunId` shared by count and page.
  A newer `running` run does not blank the previous completed/failed issue page.
- Added safe granular content-root IPC errors for invalid paths, unavailable roots, non-directories,
  overlap, and invalid operations.
- Added a shared live-evidence validation service. The M6 list bulk-checks matched rows against
  current M4 evidence before returning them, using the same rule as the M5 detail read; an evidence
  mismatch reports `stale` immediately while retaining last-known-good metadata and cover data.
- Unified the empty provider-match state: `pending` covers both no request yet and queued work;
  `notRequested` is not a separate list/detail state. Mirrored the stable Rust IPC error-code set in
  TypeScript with an unknown-string-compatible fallback.

### Architecture decisions

- The existing full M4 snapshot remains stable as a diagnostic/domain contract; M6 UI consumers use
  bounded query-oriented projections instead.
- M5's evidence-agreement rule is shared by the metadata detail and the bounded M6 list. The list
  validation uses bulk SQLite reads for the at-most-60-page matched rows, performs no provider work,
  and holds no long transaction.
- The list and detail projections do not select or serialize CRC32, MD5, SHA-1, content fingerprints,
  physical-file memberships, provider payloads, credentials, or authenticated URLs.
- User-owned favorite state is separate from scanner-owned library identity and provider-derived M5
  state. A game cannot be deleted while its user-state row remains.
- Cached media is addressed by stable local game identity through an application-owned custom
  protocol, not by a caller-supplied path or provider URL. The strongest boundary is that the
  WebView supplies only an opaque game identity; path containment checks are defence-in-depth
  against corrupted persisted cache paths. No new dependency was added.
- The metadata event is an invalidation signal emitted after durable persistence. M6.3 consumes it
  by coalescing visible affected IDs and refetching bounded authoritative state, never by rebuilding
  normalized metadata from the event or refetching once per emitted event.
- The bounded issue page resolves the latest persisted terminal run (`completed` or `failed`) once and
  uses that identity for both count and rows; `running` runs are not selected by default.
- No ADR was added: this slice uses the existing repository/application/Tauri boundaries and the
  existing Tauri 2 protocol capability without introducing a new cross-cutting architecture.

### Files/layers changed

- Rust domain/application/repository: bounded library DTOs and queries, live evidence validation,
  favorites, stable scan-run issue paging, root-error mapping, and metadata event sink.
- Rust persistence/services: forward-only favorites migration, secure cover delivery, and metadata
  media serialization boundary.
- Tauri commands/bootstrap/configuration: thin commands, `rfmedia` protocol registration, CSP, and
  event wiring.
- TypeScript IPC: mirrored request/response/event contracts, target-independent error-code types with
  forward-compatible fallback, and wrappers only; no screen/component state was added.
- Documentation: architecture, scanner, metadata, development, backlog, README, and this report.

### Tests added

- Empty, single/multiple-page, offset boundary, hard-limit, total-count, deterministic-order,
  tie-breaker, ordinary search, literal `\`/`%`/`_` search, blank search, system, availability,
  genre, region, metadata-state, favorite, cover-reference, and large synthetic-library query
  coverage.
- Bounded game-detail content summaries and serialized no-hash contract coverage.
- Favorite default/set/clear/persistence/filter/detail/invariant/delete behavior and scanner
  reconciliation preservation across unique moves and M3U remove/re-add.
- Synthetic PNG/JPEG/WebP delivery, durable-row-to-protocol delivery, missing/invalid/unsupported
  media, traversal/absolute paths, symlink containment, MIME/content validation, and route parsing
  coverage.
- Metadata invalidation payload and durable processing coverage.
- Empty/bounded/paginated/deterministic large scan-issue coverage, including a newer running run and
  its later terminal selection.
- Granular root path/overlap/invalid-operation and safe IPC error coverage.

### Verification

Focused and full Rust/frontend/release verification for the corrective pass is recorded in section M.

### Known issues / deferred items

- The bounded issue page intentionally covers persisted issues from the latest completed or failed
  run; transient watcher diagnostics remain on the legacy aggregate issue command.
- M6.1 exposes only the product-required title-ascending sort.
- A durable cover row may become unavailable between the list query and protocol request; the native
  protocol returns a safe 404 and M6.3 treats `coverRef` as an availability hint with C4 fallback.
- SQLite `lower()` is ASCII-oriented in the current configuration. Full Unicode case folding is an
  accepted search-quality deferral; this pass adds no persisted folded-search columns or replacement
  search subsystem.
- `metadata-state-changed` remains per-game by contract. M6.3 now debounces/coalesces visible bulk
  invalidations and refetches bounded current state instead of the entire library per event.
- At the M6.1 boundary, M6.3 and later UI work was intentionally not started; M6.3 now consumes these
  contracts without changing them.

### Review status

Adversarial review occurred in `M6_1_REVIEW.md`. Accepted findings were corrected in this pass; the
accepted Unicode case-fold and metadata-event-volume items remain documented deferrals/consumer
contracts. `M6_1_DELTA_REVIEW.md` completed the follow-up review and records M6.1 as READY for
M6.2. Not merged or pushed.

## D. M6.2 — Shell / Empty Library / Scan UX

Implementation started from `59b10effa6afd80addf5b53ef7a684bfc4e3bccf`, was committed as
`8a74438158f221464a972fb89c7774aa2e48f2c3`, and is complete within the M6.2 boundary. Browsing was
explicitly out of that slice and is now implemented separately in section E.

### Application shell and routes

- Replaced the M3 foundation placeholder with a real shell in `src/app/AppShell.tsx`: header and
  wordmark, desktop sidebar, narrow-window primary navigation, theme toggle, main region, and
  local-library footer status.
- Added the small typed route/history layer in `src/app/routes.ts`. `/library` and `/settings`
  use `pushState`/`popstate`, normalize unknown initial paths to `/library`, and preserve browser
  back/forward behavior. No routing dependency was added.
- System rows use the backend-owned systems catalog for labels and the bounded summary for counts.
  Catalog loading and failure are shown honestly, with retry and no fabricated fallback rows. The
  rows were visibly inert within M6.2; M6.3 activates them with backend-owned system identities.

### Components and root handling

- Added `LibraryPage` with empty, active-scan, completion, issue, and populated transitional states;
  `SettingsPage` with the M6.2 content-root surface; shared inline/root error components; crisp
  pixel icons; and the reusable button/row adjustments needed by the shell.
- `useLibrarySummary` consumes only `get_library_summary` for the empty/populated decision.
  `useContentRoots` consumes the existing list/add/remove/enable application commands and refreshes
  root state after mutations.
- The managed root path is displayed from the content-root response. `open_managed_rom_folder` is
  a narrow Rust command with no caller-provided path; Rust resolves the canonical managed root,
  rejects missing/non-directory/symlink replacement, and starts the platform file manager without a
  shell. Failure maps to the existing safe content-root-unavailable error. The platform opener
  executable names remain an accepted PATH/CWD-resolution follow-up rather than a claimed absolute
  executable guarantee.
- Added the official `@tauri-apps/plugin-dialog` / `tauri-plugin-dialog` 2.7.2 dependency. The
  picker requests one directory only (`directory: true`, `multiple: false`) and treats cancellation
  as normal. The capability is limited to `dialog:allow-open`; Rust still owns path validation,
  overlap checks, duplicate behavior, and system-hint rules.
- Settings exposes only managed-root presentation/opening, external-root availability, enable/
  disable, remove-from-RetroFrontier confirmation, add-folder, and rescan. Removing a root never
  deletes its files. Managed-root restrictions remain enforced by Rust.

### Scan state and UX

- Added `useScanState`, which registers both scan event listeners before querying initial status and
  saved issues. It uses an event-version guard for startup races, mounted guards for async request
  work, effect-local disposal for late listeners, request-version guards for authoritative issue
  result/error writes, and no polling/timers. Each issue request releases its own loading flag even
  when a newer request supersedes its result; terminal same-run progress is ignored.
- Initial status and saved-issue loading are explicit. Progress events update only the scan panel;
  counters and phase labels come directly from M4 DTOs. Progress is indeterminate during discovery
  and becomes determinate only after discovery has supplied a meaningful file denominator. There is
  no invented ETA, current filename, root name, system counter, or synthetic progress timer.
- Completion events update the local scan status from the payload, refresh the bounded library
  summary once, and refresh the bounded persisted issue page. A command result is also handled if an
  event is missed; queued requests are represented as running status rather than fabricated work.
- Scan failures are presented as completed failed runs when the backend emits the terminal payload;
  a request error with no scan event remains a retryable start/request error.

### Scan issues and populated state

- The issue surface uses only `get_scan_issue_page` with a page size of 50. It presents issue kind,
  safe root/path context, persisted detail, total count, terminal `scanRunId`, and load-more paging.
  Refresh failures and pagination failures have separate truthful retry actions.
- A previous terminal issue page remains visible while a newer scan is running and is explicitly
  labelled with its persisted `scanRunId` and the current run where available.
- At the M6.2 boundary, a non-empty summary rendered a restrained `LIBRARY READY` transitional state
  with real totals and no fabricated cards. M6.3 removes that component and its styles rather than
  maintaining two populated-library implementations.

### Corrective pass after adversarial review

The complete adversarial review in `M6_2_REVIEW.md` was performed before this corrective pass; the
review artifact remains unchanged. The focused corrections are:

- HIGH-1: `loadMoreIssues` and `refreshIssues` still use request versions to guard authoritative
  issue-page result and error writes, but each request's `finally` now releases its own loading flag
  whenever the component is mounted. A superseded response cannot overwrite a newer page and cannot
  strand either issue loading state.
- LOW-1: scan-listener registration uses an effect-local disposal flag. A listener that resolves
  after effect teardown is immediately unregistered; a live registration is retained for normal
  cleanup. The component-level mounted guard remains for request/state work.
- LOW-2: progress for a run already handled as terminal is ignored, so late same-run progress cannot
  resurrect the running UI or trigger another summary/issue refresh.
- MEDIUM-2: the frontend no longer has a fabricated systems fallback. It shows a checking state
  while `getSystems` is pending, exposes a retryable catalog error, and renders only the backend
  catalog on success. The shared frontend accent map contains presentation colors only and returns a
  safe default accent for unknown future IDs.
- MEDIUM-1: the scan live region announces phase/status only. The visual processed-file counter is
  outside the live region and is hidden from assistive technology, with no frontend throttling.
- MEDIUM-3: root-removal confirmation uses alert-dialog semantics, moves focus to the destructive
  confirmation control, restores focus to the original Remove Root trigger on cancel, and moves focus
  to the roots heading after successful removal.
- MEDIUM-4: native opener executable resolution is an accepted follow-up. The implementation uses
  `explorer.exe`, `open`, and `xdg-open` as host-resolved executable names without a shell; the report
  makes no absolute-path claim, records that Windows/macOS were not runtime-validated, and requires
  revisiting resolution before Windows packaging/release if unresolved.
- Documentation drift was corrected in `README.md`, `docs/DEVELOPMENT.md`, `BACKLOG.md`, and this
  report. No nonexistent visual anti-pattern detector is claimed as verification; any design check
  is manual.

### Tests and design coverage

- Frontend coverage is in `src/app/AppShell.test.tsx`, `src/features/settings/RootActionError.test.tsx`,
  `src/platform/folderPicker.test.ts`, and `src/platform/ipc.test.ts`. It covers navigation/history,
  managed-root actions, picker cancellation/selection, all typed root-error messages, scan loading/
  progress/completion/race/cleanup behavior, superseded issue loading teardown, late listener
  registration cleanup, post-completion progress, scan live-region semantics, catalog loading/
  success/failure/retry/unknown-ID behavior, root-removal confirmation focus, bounded issue paging/
  failure/context, and the populated transitional state.
- Rust coverage adds managed-path resolution, missing-directory/symlink rejection, and fixed
  command/no-shell argument assertions. No large backend feature or domain rule was added for this
  UI slice.
- The implementation maps the handoff's A1/A2/A3/A6, B1/B4/B9/B11, and C1/C7/C8 surfaces to
  supported data. It preserves the existing tokens, Press Start 2P/VT323/Space Grotesk typography,
  hard pixel borders/shadows, focus treatment, scanlines, theme support, and responsive shell.

### Deviations and accepted deferrals

- The backend exposes no scan-cancel command, current filename, current root/system, or ETA, so
  those A2 affordances are omitted rather than simulated. The explicit rescan action is disabled
  while a scan is active to prevent accidental duplicate requests; the backend's queued behavior
  remains available to other application-triggered requests without being simulated in the UI.
- B11's HTML browser mockup is not used; the official native folder-only picker is the source of
  truth for selection. There is no arbitrary external-root opener because only managed-root opening
  is supported safely in this slice.
- Issue remediation, duplicate repair, system assignment, file movement/deletion, metadata/provider
  settings and core/video/controller/save settings remained deferred; browsing is now section E and
  game detail/readiness is section F.
- Native managed-folder opening intentionally keeps the platform executable names `explorer.exe`,
  `open`, and `xdg-open`; they are resolved by the host process rather than by an absolute path.
  The existing safety boundary remains: no caller-provided path, no shell, one validated canonical
  directory argument, and no broad opener capability. Windows/macOS runtime and packaging were not
  exercised; executable resolution must be revisited before Windows packaging/release if unresolved.
- Accepted LOW follow-ups remain intentionally untouched: stale-running recovery control, frontend-
  authored error copy, duplicate scan-start retry guarding, opener-specific unavailable copy,
  heading/anchor cleanup, minor scan-status wording, Unix child reaping, and an inert-system
  explanation tooltip.
- Verification was performed on Linux x86_64. Conditional opener code is compiled for desktop
  targets by Rust configuration, but Windows/macOS packaging was not exercised in this environment.

## E. M6.3 — Library Browsing

Implementation started from `28e20dab7c5d68e100555ac94f7f610b2583c728`. The approved design and
implementation plan are recorded in `docs/superpowers/specs/2026-08-28-m6-3-library-browsing-design.md`
and `docs/superpowers/plans/2026-08-28-m6-3-library-browsing.md`.

### Carried M6.2 correction

- Corrected DELTA-LOW-1 before introducing M6.3 query state. Scan issue refresh and load-more
  indicators now have operation-scoped owners in addition to the shared response version, so a
  superseded request cannot clear loading state owned by a newer operation while stale data and
  errors remain rejected.
- Added a focused overlapping-refresh regression in `AppShell.test.tsx`: the older refresh resolves
  first, the newer refresh retains its loading indicator, and only the newest operation releases it.

### Query architecture and performance

- Added focused `useLibraryQuery` state. It is the only M6.3 consumer of `query_library` and owns
  raw/debounced search, system and favorite filters, the current replacement page, three loading
  channels, query/favorite errors, paging, favorite mutations, scan-completion invalidation, and
  metadata-event invalidation.
- Requests omit `limit`, deliberately using M6.1's bounded default of 60. No snapshot, full-library
  download, per-card metadata call, per-card detail call, provider request, or unbounded accumulation
  was added. Title ascending is the only exposed sort because it is the only supported backend sort.
- Query identity includes the debounced literal search, backend system ID, favorites-only flag,
  title sort, and the requested offset. Search/filter changes issue an offset-zero request. The
  committed rendered page is the authoritative pagination position; previous/next controls derive
  from it and replace the page rather than append it. A total that shrinks below the requested offset
  redirects to the last valid page without installing the invalid empty offset.
- A monotonic result generation rejects stale data and stale errors. Initial, refresh, and page
  loading channels have independent operation owners, so an older operation cannot release loading
  owned by a newer same-channel request. A private latest-query ref carries the current logical
  request target for refreshes and retries, while a target is not treated as committed until its
  successful page becomes authoritative. Favorite completions read the latest query callback and
  target, so a held mutation cannot restore a superseded query identity. Unmount invalidates active
  work.

### Corrective pass after the M6.3 adversarial review

`M6_3_REVIEW.md` completed the M6.3 adversarial review with 0 CRITICAL, 0 HIGH, 5 MEDIUM, 11 LOW,
and 3 INFO findings. The review marked M6.3 `READY FOR M6.4`; MEDIUM-1 and MEDIUM-2 were corrected
before M6.4 because they affect the foundational `useLibraryQuery` state boundary. MEDIUM-3 through
MEDIUM-5 and LOW-1 through LOW-11 remain documented deferrals for later hardening.

- MEDIUM-1: metadata invalidation now invokes the latest query callback with the latest logical
  request target rather than reading the obsolete rendered-page offset. A navigation to offset 60
  therefore remains the target when a visible metadata event arrives during the request. Query
  identity changes update the callback/target ref before the invalidation timer can resurrect an old
  filter.
- MEDIUM-2: the rendered `page.offset` is the committed position. Page navigation requests are
  issued directly from that page, and a failed page-forward leaves the page unchanged while the
  failed target remains available to the dedicated retry. Ordinary Next derives from the still-
  authoritative page and reissues the target normally; Back also derives only from the committed
  page. No requested offset is committed before a successful response.
- The existing monotonic generation still arbitrates navigation, invalidation, query, favorite,
  and scan operations. Stale success/error paths remain unable to replace page state or errors, and
  operation-scoped loading owners still release only their own channel work.
- Exact corrective regression tests:
  - `keeps a requested page authoritative when metadata invalidation overlaps navigation`
  - `does not revive the previous query when a filter changes during invalidation debounce`
  - `keeps the committed page and allows ordinary Next after page-forward failure`
  - `retries a failed page-forward at its failed target`

### Search, filters, and states

- The B2 header search uses a visible `SEARCH LIBRARY` label with `<input type="search">`, a 200 ms
  effect-owned debounce, literal query forwarding, a keyboard-reachable clear action, preserved
  current content during refresh, and cleanup on change/unmount.
- Sidebar rows are now real one-system-at-a-time filters. IDs/display names come only from the Rust
  catalog, counts come from `get_library_summary`, and unknown future IDs use the existing visual
  accent fallback without inventing catalog data. All Systems and system rows expose pressed state.
- The M6-compatible B3 subset includes the handoff's `// FILTER` hierarchy, favorites-only, pixel
  shadow treatment, and a combined clear-search/filters action.
  Genre/region exact filters exist in M6.1, but M6.1 exposes no bounded distinct-value/facet contract.
  M6.3 therefore does not download all games or add an unbounded facet endpoint; selectable genre and
  region discovery remains explicitly deferred. Unplayed/recent/core/BIOS filters remain out of scope.
- Initial loading reserves card geometry. Refreshes preserve the current page with a small updating
  status. Page loading leaves cards/favorites available. Query errors retain the shell and retry.
  B5 filtered-empty copy is distinct from M6.2 first-run empty-library onboarding and never suggests
  a rescan merely because a query found no matches.

### GameCard, covers, availability, and metadata

- Replaced the `LIBRARY READY` transitional state with the real bounded grid. `GameCard` consumes
  `LibraryListItem` only: effective title/local fallback, catalog system label, release year,
  genre/region where present, favorite, local availability, coarse metadata state, and `coverRef`.
- Cover space is reserved at 3:4. The opaque native reference is passed unchanged to a lazy image;
  load/404 failure changes only that card to the C4 fallback. A later authoritative DTO retries even
  when the stable opaque reference is unchanged. The fallback uses the system accent,
  Press Start 2P title treatment, centered wrapping, and no fabricated artwork or gradients. A
  changed authoritative cover reference is retried after an earlier reference failed.
- Local availability is always separate from metadata status. A failed/no-match/deferred metadata
  state never implies missing content. Stale metadata retains its last-known-good title and cover and
  adds only a concise browsing-level status. Matched metadata is intentionally quiet.

### Favorites and invalidation

- Favorite writes use `set_game_favorite` and do not optimistically alter the DTO. A synchronous
  per-game pending set suppresses duplicate clicks; success refetches the bounded authoritative page
  and summary, while failure preserves the confirmed card and shows a safe error. Unfavoriting under
  favorites-only resets a later page exactly once or refreshes page zero, so the removed card leaves
  coherently without mixing offsets. The success continuation reads the latest committed query
  identity, preventing a delayed write from overwriting a newer search/system/filter/page.
- `metadata-state-changed` is consumed as invalidation only. Visible game IDs schedule one trailing
  180 ms timer, while off-page IDs cause no immediate work. The timer uses the latest logical query
  target and query callback, so it cannot replace an in-flight navigation with the old rendered-page
  offset. Cleanup clears the timer/set and unregisters normally or immediately after late async
  registration.
- `useScanState` continues to deduplicate command/event terminal runs. AppShell passes only the newly
  handled terminal run ID to the query hook, producing one bounded refresh per completed run. Scan
  progress has no query-hook input and never refreshes the library page.

### Accessibility and visual decisions

- Cards are semantic articles with headings and exactly one independent favorite button; there is no
  nested button, dead detail route, or M6.4 navigation. Search, filter, clear/reset, favorite, and page
  controls are keyboard reachable with names/states. Real covers have useful alt text and placeholders
  use an equivalent image role/name.
- B1/B2/B3/B5/C4 and A6 are implemented with the existing dark/cream themes, token colors, bundled
  typography, hard pixel borders/shadows, focus inversion, search inset focus, list cursor language,
  and card scale/shadow focus. The grid uses `auto-fill` with a 158 px floor at desktop widths and a
  140 px narrow fallback; the sidebar remains present at the configured 960 px minimum.
- Active system rows retain B1's accent treatment and A6 cursor-only focus language; the light theme
  mixes that accent toward white just enough for its black 14.5 px label to clear the contrast floor.
  Light-theme system chips/placeholders use the same bounded presentation adjustment. Reset and
  missing-local-content copy uses theme-safe text contrast while retaining the Arcade accent as a
  non-text underline. The weakest adjusted pairing (light `accent-3`) is 6.36:1. No source token or
  new color token was introduced.
- The Impeccable mechanical detector reported one warning for the existing 3 px black structural
  sidebar divider. It is the established B1 shell separator, not a colored card side-tab, so it was
  retained. No generated image or fake cover asset was introduced.

### Tests

- Added focused hook coverage for initial success/error/retry, forward/back/final paging, no-more,
  filter reset, total shrink, 200 ms literal search debounce, stale result/error/loading races,
  unmount, favorite on/off/failure/duplicate suppression/favorites-page reset, a held favorite write
  completing after a system-filter change, visible/off-page metadata cadence/deduplication/timer/
  listener cleanup, terminal scan-run identity, invalidation during page navigation, query identity
  changes during invalidation debounce, failed page-forward recovery, and retry at the failed target.
- Added GameCard coverage for title fallback, system/year/list metadata, favorite semantics/pending,
  local unavailability, independent metadata failure, stale presentation, lazy opaque covers, 404
  fallback, missing-cover C4 placeholder, long titles, and changed cover references.
- Expanded AppShell integration coverage for the real populated grid, query error/retry, search/no
  results/clear, system IDs/counts/pressed state, favorites-only/unfavorite behavior, and scan refresh
  cadence. All fixtures are synthetic/local; no live ScreenScraper, ROM, BIOS, or copyrighted art is
  used.

### Deferrals

- M6.5 metadata/settings workflows, M6.6 final hardening, M7 play data,
  M8 controller/on-screen-keyboard behavior, and later milestones were not implemented.
- Genre/region facet discovery is deferred as described above. Existing accepted M6.2 LOWs other
  than required DELTA-LOW-1 remain untouched.
- The remaining M6.3 MEDIUM findings (light-theme contrast, the library section heading, and
  favorite-button focus custody) and all 11 M6.3 LOW findings remain deferred; they do not affect
  the corrected pagination/query-state contract and do not block M6.4.

## F. M6.4 — Game Detail / Readiness

M6.4 uses the existing bounded M6.1/M5 contracts and the existing Rust-owned system readiness
snapshot. No backend readiness projection was added: `get_systems` already evaluates one coherent
`VerifiedRuntimeSnapshot`, BIOS discovery result, `SystemCatalog` core policy, and
`SystemReadiness` per system. React selects the returned status for the game's system and formats
it for display; it does not infer core policy, BIOS substitution, runtime trust, or launchability.

### Routing and navigation

- Extended the small `pushState`/`popstate` route layer with `/games/:id`. IDs accept only positive
  safe integer path segments. Missing, malformed, oversized, and unknown game IDs remain a typed
  game route and render a stable invalid/not-found page instead of throwing. Unrelated unknown paths
  still normalize to `/library`.
- A card title is now a semantic detail link. Its normal left-click is handled by the route layer,
  while modified clicks retain native link behavior. Favorite remains a separate sibling button;
  no interactive element contains another interactive element.
- Detail provides explicit semantic Back to Library navigation. Browser/WebView back and forward
  continue to use history. The library query remains mounted in `AppShell`, and its committed page,
  search, system, and favorites state survive the detail route. Returning also restores focus to the
  originating card when it is still rendered, or to the Library heading when it is not.
- Direct valid deep links load detail without a library visit. Invalid route IDs and missing local
  games expose a truthful message and a Library path. M7 launch behavior is deliberately absent.

### Detail data architecture and boundaries

- `useGameDetail` owns separate local-detail, metadata, readiness-facing, and favorite channels.
  Local detail calls `get_library_game_detail({ gameId })`; metadata calls the authoritative M5
  `get_game_metadata({ gameId })`; readiness reuses the already-loaded `get_systems` status for the
  matching system. There is no full-library snapshot, library-wide metadata lookup, queue read,
  provider request, or per-content-file query.
- Local detail and metadata load independently and use generation guards. A local failure does not
  blank metadata/readiness, and a metadata failure does not make a valid local game look missing.
  Local not-found is represented separately from transport/application failure and has its own
  stable page state. Each failed channel has its own retry.
- The detail projection displays the local title, normalized title when available, system identity,
  local availability, content-unit summaries, relative primary paths, root identity, file counts,
  normalized metadata fields, provenance credit, and readiness state. It never adds hashes,
  fingerprints, physical memberships, provider payloads, credentials, authenticated URLs, queue
  internals, or runtime filesystem paths.
- Favorite mutation reuses `set_game_favorite`, does not optimistically invent a state, updates the
  detail from the authoritative response, and refreshes the bounded library summary. No launch,
  save-state, controller, or metadata candidate-selection control was added.

### State separation and presentation

- Local content availability, emulation readiness, and metadata enrichment remain visibly separate.
  The local content panel reports the game and each bounded content unit as available, incomplete,
  or missing. Single-file, CHD, CUE/BIN, GDI, and M3U units retain their unit kind and summary rather
  than being flattened into one arbitrary ROM path; zero-unit records remain understandable.
- Metadata uses normalized M5 data only. Pending, matched, no match, ambiguous, deferred, failed,
  and stale states have truthful copy. Stale state retains its last-known-good title, synopsis, and
  cover while explicitly asking for revalidation. Metadata failure/no-match/deferred/ambiguous does
  not alter local availability.
- Covers reuse the opaque `coverRef` boundary and the shared `GameCover` component. React never
  resolves a filesystem path or provider URL. Missing/404 media falls back to the system-accent
  placeholder without retries; a later authoritative DTO identity permits recovery from an earlier
  failed image.
- Readiness is presented as separate LOCAL CONTENT, RUNTIME, CORE, and BIOS rows. Overall status is
  Rust's `SystemReadiness` with local unavailability taking priority in the presentation. The page
  says `EMULATION READY` / `REQUIREMENTS NOT SATISFIED` rather than claiming `READY TO PLAY`, and
  reports unavailable, missing, invalid, and unknown dependencies in text as well as color.
- BIOS output uses the existing per-system policy/requirement state and safe expected/matched
  filenames only. It does not scan from React, expose BIOS hashes or paths, import/download files,
  or offer destructive file actions. Missing BIOS copy points to the managed BIOS location and a
  readiness retry.

### Refresh, events, and focus

- `metadata-state-changed` is subscribed only for the open game. Matching events use one trailing
  180 ms debounce, repeated transitions coalesce, unrelated game IDs do nothing, and listener/timer
  cleanup handles ordinary and late registration. Refresh refetches only that game's metadata.
- AppShell passes the deduplicated terminal scan run ID from `useScanState` to the detail hook. Scan
  progress does not refresh detail; each new terminal run refreshes bounded local detail and
  metadata/readiness-facing state once. Duplicate terminal delivery is ignored. A disappeared game
  becomes the stable not-found state after refresh.
- Detail heading focus moves on entry/route change. The page has one H1, semantic main/back/link
  structure, keyboard-accessible favorite and retry controls, meaningful cover fallback naming, and
  text readiness statuses. No M8 controller focus graph was introduced.

### Tests and design coverage

- Added route, AppShell, GameCard, GameCover, detail-page, readiness projection, `useGameDetail`,
  library-query preservation, and system-catalog status-retention coverage. Tests cover deep links,
  invalid/nonexistent IDs, browser back, context/focus restoration, bounded IPC calls, valid and
  partial loads, independent retries/errors, all metadata states, stale cached display, multi-unit
  content, all normalized readiness dependency states, readiness retry, favorite authority, current
  versus unrelated metadata invalidations, debounce/coalescing/cleanup, scan progress/terminal
  cadence, and accessibility semantics.
- Static/manual comparison covered B6 Game Detail, B1/B3/B5 navigation context, C1 readiness/error
  language, C4 cover fallback, A6 focus language, the existing typography/tokens, dark/light themes,
  960×640 and larger desktop layouts, long text, no cover, unavailable content, stale metadata,
  readiness dependency failure, and not-found states. No native screenshot harness exists in this
  repository, so no rendered screenshot claim is made.

## G. M6.5 — Metadata UX / Settings

Not started. No implementation details are inferred here.

## H. M6.6 — Hardening / Accessibility / Documentation

Not started. No implementation details are inferred here.

## I. Architecture and Security Audit

- React has no SQLite, filesystem, scanner, provider, credential, or runtime access.
- SQL remains in repositories and Tauri commands remain thin.
- UI projections are bounded and omit physical hashes/fingerprints and provider internals.
- Cached-cover delivery accepts only stable game identity routes, enforces app-owned containment, and
  serves only validated supported image types.
- Provider URLs, credentials, raw responses, candidate lists, and queue internals do not cross the
  new list/detail/event/media boundaries.
- M6.4 adds no Rust production code or new IPC readiness model. The detail page reuses the existing
  `get_systems` response, whose Rust service composes `SystemCatalog`, one verified runtime snapshot,
  BIOS discovery, and `SystemReadiness`; the UI only selects and presents the matching system status.
- Detail-specific React state uses the bounded `get_library_game_detail` and `get_game_metadata`
  commands and contains no launch, process, filesystem, BIOS mutation, or provider-management path.
- React still has no SQLite, filesystem enumeration, scanner, provider, credential, or runtime
  access. M6.2 UI state consumes summary/root/scan contracts only.
- The native dialog is the official Tauri dialog plugin with only `dialog:allow-open`. Managed-folder
  opening accepts no frontend path and passes one validated canonical directory to the host-resolved
  platform opener, never a shell. Absolute executable resolution remains an accepted follow-up.
- No copyrighted fixture, commercial ROM, BIOS, runtime binary, secret, or generated build artifact
  was added. The one new dependency and capability change are documented in section D.
- No ADR was added: this is a focused UI integration using existing application boundaries; the
  narrow plugin/capability change is recorded here.

## J. Design Coverage

M6.2 retains the shell framing from B1, empty/setup guidance from A1/B4, scan states, and root
surfaces. M6.3 adds the real B1 bounded grid, B2 search, supported B3 filter subset, B5 filtered-empty
state, C4 cover fallback, and A6 focus language. M6.4 adds the B6 detail hierarchy: back/library
context, larger cover/fallback, title/system identity, normalized metadata, separate local content
units, and a requirements/readiness panel. C1 supplies readiness/error language and C4's secure
opaque-cover fallback is shared between card and detail. The token file remains authoritative across
dark and light themes. Prototype-only genre/region discovery without a bounded option contract,
unplayed, controller footer/keyboard, metadata candidate/editor workflows, launch, and save states
were intentionally omitted.

Manual inspection was performed against code/DOM/CSS at the 960×640 minimum, 1280×800, and larger
desktop layouts, including dark/light tokens, populated cards, a long title, missing/failed cover,
unavailable local content, no-results, active filters, focus, and page controls. The repository has
no visual runner or fixture-capable native browser harness, so no screenshot or automated visual
claim is made. The Impeccable detector was run once over the changed UI and reported only the
existing B1 3 px black structural sidebar divider as a side-tab heuristic; it was retained.

## K. Test Coverage

The frontend suite has 117 synthetic/local tests across 12 files. M6.3 coverage remains intact for
initial/error/retry query state, multiple/final pages, filter reset, total shrink, literal debounced
search, stale result/error/loading races, active-request unmount, favorites, duplicate writes,
held-write/current-filter ownership, filtered-page removal, visible metadata invalidation cadence/
lifecycle, terminal scan identity, real grid integration, system IDs/counts, search/no-results/
clear, card semantics, title/cover fallbacks, availability, and stale metadata. M6.4 adds route and
history tests, semantic card activation, detail deep-link/not-found/focus/context tests, bounded
local/metadata loads with independent errors/retries, all metadata states, stale cached display,
multi-unit content, normalized readiness states, favorite authority, targeted/coalesced metadata
events, terminal scan refresh cadence, listener/timer cleanup, and accessibility semantics. Existing
M6.2/root/IPC tests and all Rust tests remain passing. No live ScreenScraper or copyrighted fixture
is used.

## L. Deferred Beyond M6

- M6.5 detailed metadata states, metadata/provider settings, and settings workflows.
- M6.6 final cross-screen hardening and accessibility consistency.
- M7 launch/process/play-session and per-game core behavior.
- M8 controller abstraction, focus graph, and on-screen keyboard.
- M9 save-game and save-state management.
- M10 packaging, signing, and release work.
- Existing documented non-blocking M5/M4 observations that are not correctness dependencies of M6.1.

## M. Final Verification

Final verification is recorded here after the M6.4 implementation and documentation changes:

- `pnpm vitest run src/hooks/useGameDetail.test.tsx src/app/AppShell.test.tsx` — PASS — 2 files,
  40 tests.
- `pnpm vitest run src/features/library/readiness.test.ts src/features/library/GameDetailPage.test.tsx`
  — PASS — 2 files, 18 tests.
- `pnpm vitest run src/hooks/useLibraryQuery.test.tsx` — PASS — 1 file, 23 tests.
- `pnpm typecheck` — PASS — `tsc -b --pretty false`, exit 0.
- `pnpm lint` — PASS — `eslint .`, exit 0.
- `pnpm format:check` — PASS — all configured files use Prettier formatting, exit 0.
- `pnpm test` — PASS — 12 files, 117 tests, exit 0.
- `pnpm build` — PASS — TypeScript and Vite build; 51 modules transformed, exit 0.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` — PASS — exit 0.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
  — PASS — release-free dev profile finished without warnings, exit 0.
- `cargo test --manifest-path src-tauri/Cargo.toml` — PASS — 305 passed, 0 failed, 1 ignored;
  frontend-independent Rust test command exit 0.
- `cargo build --manifest-path src-tauri/Cargo.toml --release` — PASS — optimized release profile,
  exit 0.
- `pnpm tauri:build` — PASS — frontend build and release application build completed at
  `src-tauri/target/release/retrofrontier`, exit 0.
- `git diff --check` — PASS — no whitespace errors, exit 0.

The Impeccable detector command was also run over the changed UI. It returned one warning for the
pre-existing B1 structural `border-right: 3px solid var(--border)` sidebar divider; no new detail or
readiness anti-pattern warning remained. Visual verification was static/source-level only because
the repository has no native screenshot harness. Verification was performed on Linux x86_64; no
Windows/macOS runtime or packaging validation was performed.

## N. Current M6 Verdict

`M6.4 IMPLEMENTATION COMPLETE — ready for review before M6.5`
