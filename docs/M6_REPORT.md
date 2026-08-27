# M6 Library UI Implementation Report

## A. Repository State

- Starting corrective-pass HEAD: `2708746847e7ad7086f33c5b88e127ce5b1fda23`
- Starting M6.2 HEAD: `59b10effa6afd80addf5b53ef7a684bfc4e3bccf`
- Branch: `feat/m6-library-ui`
- Main comparison at start: local `main` and `origin/main` were both
  `9a1a1e3d8c38633c1c82bc95293c6a6024e94e93`; no rebase was required.
- Original M6.1 implementation commits remain `1cdf5a2` and `e7f3df8`; they were not rewritten.
- Final corrective-pass HEAD: the single corrective commit created from the starting corrective-pass HEAD; see the final repository handoff for its exact ID.
- M6.2 implementation commit: `8a74438158f221464a972fb89c7774aa2e48f2c3`
- M6.2 corrective commit: `fix(ui): address M6.2 adversarial review findings`
- Merged to main: No
- Pushed: No
- Pre-existing untracked files: `M3_REVIEW.md`, `M4_REVIEW.md`, `M4_REVIEW_2.md`,
  `M4_REVIEW_3.md`, `M5_REVIEW.md`, `M6_1_REVIEW.md`, `M6_1_DELTA_REVIEW.md`, `M6_2_REVIEW.md`,
  `docs/M5_IMPLEMENTATION_REPORT.md`

## B. Overall M6 Status

- [x] M6.1 Backend Enablement
- [x] M6.2 Shell / Empty Library / Scan UX
- [ ] M6.3 Library Browsing — active
- [ ] M6.4 Game Detail / Readiness
- [ ] M6.5 Metadata UX / Settings
- [ ] M6.6 Hardening / Accessibility / Documentation

Current phase: M6.1 and M6.2 are complete and reviewed; M6.3 implementation is active from
`28e20dab7c5d68e100555ac94f7f610b2583c728`.
Overall status: M6.1 and M6.2 are accepted as READY. M6.3 design and implementation work is in
progress; it is not complete and M6.4 has not started.

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
- The metadata event is an invalidation signal emitted after durable persistence; the future UI must
  refetch bounded authoritative state. Bulk enrichment may emit many per-game events; M6.3 must
  debounce/coalesce affected IDs and must not refetch the entire library once per event.
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
  protocol returns a safe 404 and the future UI should treat `coverRef` as an availability hint.
- SQLite `lower()` is ASCII-oriented in the current configuration. Full Unicode case folding is an
  accepted search-quality deferral; this pass adds no persisted folded-search columns or replacement
  search subsystem.
- `metadata-state-changed` remains per-game by contract. M6.3 must debounce/coalesce bulk invalidations
  and refetch bounded visible/current state instead of the entire library per event; that logic is
  intentionally not implemented here.
- M6.3 and later UI work is intentionally not started.

### Review status

Adversarial review occurred in `M6_1_REVIEW.md`. Accepted findings were corrected in this pass; the
accepted Unicode case-fold and metadata-event-volume items remain documented deferrals/consumer
contracts. `M6_1_DELTA_REVIEW.md` completed the follow-up review and records M6.1 as READY for
M6.2. Not merged or pushed.

## D. M6.2 — Shell / Empty Library / Scan UX

Implementation started from `59b10effa6afd80addf5b53ef7a684bfc4e3bccf`, was committed as
`8a74438158f221464a972fb89c7774aa2e48f2c3`, and is complete within the M6.2 boundary. M6.3
remains explicitly out of scope.

### Application shell and routes

- Replaced the M3 foundation placeholder with a real shell in `src/app/AppShell.tsx`: header and
  wordmark, desktop sidebar, narrow-window primary navigation, theme toggle, main region, and
  local-library footer status.
- Added the small typed route/history layer in `src/app/routes.ts`. `/library` and `/settings`
  use `pushState`/`popstate`, normalize unknown initial paths to `/library`, and preserve browser
  back/forward behavior. No routing dependency was added.
- System rows use the backend-owned systems catalog for labels and the bounded summary for counts.
  Catalog loading and failure are shown honestly, with retry and no fabricated fallback rows. The
  rows remain visibly inert until M6.3 owns system filtering; no fake filter behavior is present.

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
- A non-empty summary renders a restrained `LIBRARY READY` transitional state with the real total
  and system counts. It does not query the full M4 snapshot and does not fabricate GameCards,
  covers, search, filters, favorites, or pagination.

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
  settings, core/video/controller/save settings, full browsing, and game details remain deferred to
  their milestones.
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

Active. The approved design is recorded in
`docs/superpowers/specs/2026-08-28-m6-3-library-browsing-design.md`. Implementation starts with the
carried M6.2 DELTA-LOW-1 loading-ownership correction, then adds bounded query state, search,
system/favorite filters, page controls, list-DTO GameCards, cached-cover fallback, coalesced visible
metadata invalidation, and one refresh per completed scan run. Genre/region facet discovery is
deferred because M6.1 exposes exact filters but no bounded aggregate option contract; M6.3 will not
derive facets by downloading the library or add an unbounded analytics endpoint.

## F. M6.4 — Game Detail / Readiness

Not started. No implementation details are inferred here.

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

M6.2 implements the shell framing from B1, empty/setup guidance from A1 and B4, active scan from
A2, completion from A3, focus language from A6, the root-management subset of B9, native folder
selection for B11, and the scan/root issue presentation needed from C1/C7/C8. The existing token
file remains authoritative. Unsupported prototype-only affordances, such as B11's fake browser,
A2 ETA/current-item details, settings controls owned by M6.5, and all M6.3 library browsing
controls, were intentionally omitted.

## K. Test Coverage

The frontend suite has 32 synthetic/local tests across four files and covers the M6.2 shell, route
history, summary-driven empty/populated states, root actions and taxonomy fallbacks, native picker
shape/cancellation, scan event races/progress/completion/refetch cadence/listener cleanup, superseded
issue loading teardown, late listener registration, post-completion progress, live-region semantics,
system catalog loading/failure/retry/unknown-ID behavior, removal-confirmation focus, bounded issue
paging and terminal-run context, and issue-page failure. Rust adds three native opener tests and all
existing M4/M5/M6.1 tests remain passing. No live ScreenScraper or copyrighted fixture is used.

## L. Deferred Beyond M6

- M7 launch/process/play-session and per-game core behavior.
- M8 controller abstraction, focus graph, and on-screen keyboard.
- M9 save-game and save-state management.
- M10 packaging, signing, and release work.
- Existing documented non-blocking M5/M4 observations that are not correctness dependencies of M6.1.

## M. Final Verification

- `pnpm typecheck` — PASS (`tsc -b --pretty false`).
- `pnpm lint` — PASS (`eslint .`).
- `pnpm format:check` — PASS (all checked files use Prettier code style).
- `pnpm test` — PASS (4 test files, 32 tests).
- `pnpm build` — PASS (`vite build`, 44 modules transformed).
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` — PASS.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` —
  PASS.
- `cargo test --manifest-path src-tauri/Cargo.toml` — PASS (306 library tests run: 305 passed,
  1 ignored, 0 failed; main and doc-test binaries had 0 tests).
- `cargo build --manifest-path src-tauri/Cargo.toml --release` — PASS (release application built;
  26.29 seconds).
- `pnpm tauri:build` — PASS (Tauri release application built at
  `src-tauri/target/release/retrofrontier`; frontend transformed 44 modules).
- `git diff --check` — PASS.
- Manual design inspection — PASS against the relevant M6 design handoff artifacts; the existing
  structural sidebar border was retained because it is part of the B1 shell treatment. No automated
  anti-pattern detector is claimed.

## N. Current M6 Verdict

`M6.2 status: implementation complete, corrective pass complete, awaiting delta review`
