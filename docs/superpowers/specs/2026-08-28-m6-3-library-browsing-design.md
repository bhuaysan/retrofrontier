# M6.3 Library Browsing Design

## Scope

M6.3 replaces the M6.2 populated-library placeholder with RetroFrontier's real, local-first
browsing experience. It consumes the accepted M6.1 bounded contracts and retains the existing
shell, routes, system catalog, library summary, content-root state, and scan UX. It does not add a
game-detail screen, metadata-management workflow, launch/readiness UI, controller architecture, or
later-milestone behavior.

Before this work is reused for browsing requests, M6.2 DELTA-LOW-1 is corrected locally in
`useScanState`: request results and errors remain version-authoritative, while each loading flag can
only be cleared by the newest request of the operation that owns that flag.

## Data and State Architecture

`useLibraryQuery` is the focused browsing state boundary. It owns:

- the raw search input and a 200 ms debounced backend search value;
- the selected backend `SystemId` and favorites-only filter;
- the current bounded page offset and the backend-returned page;
- initial, refresh, and page-navigation loading presentation;
- query and favorite errors;
- request generations, query identity, and operation-scoped loading ownership;
- authoritative favorite mutations with one in-flight mutation per game;
- metadata invalidation subscription, visible-ID filtering, coalescing, and cleanup;
- scan-completion refresh input.

The hook invokes only `query_library` and `set_game_favorite`. It never calls the full library
snapshot, a per-card detail command, or a per-card metadata command. Query identity contains every
implemented backend input: debounced search, system, favorites-only, title-ascending sort, and
offset. Changing search or a filter resets the offset to zero before querying.

Every request receives a monotonically increasing generation. Only the current generation may
replace page data or errors, and only the request that still owns a loading channel may clear that
channel. Effects use local disposal state and clear pending search/invalidation timers during
cleanup. An unmounted hook cannot update state.

## Paging and Refresh

The UI uses bounded previous/next page controls. It relies on the M6.1 default page size of 60 by
omitting a separate frontend limit; subsequent requests use the effective `limit` returned by the
backend. Pages are replaced, never accumulated, so the browser remains bounded and cannot mix rows
from different query identities.

Initial load shows design-consistent skeleton blocks with no fabricated titles. Search, filter,
metadata, scan, and favorite refreshes preserve the current page with a subtle busy indication.
Page navigation also preserves current content until the requested page arrives. Query failures
leave the shell and any prior page available and provide a retry action. If an authoritative total
shrinks below the current offset, the hook requests the last valid bounded page.

`useScanState` already deduplicates command-result and completion-event handling by run. `AppShell`
passes that one terminal-run signal to `useLibraryQuery`; scan progress never refreshes browsing.

## Search and Filters

The header contains a semantic `input type="search"` with a visible label, clear button, and 200 ms
debounce. The literal user value is passed unchanged to the backend after debounce; the frontend
does not reproduce SQL wildcard or normalization rules.

The existing sidebar becomes the single-select system filter. “All systems” clears the filter,
and catalog rows use backend identities/display names with summary-owned counts. Unknown future
system IDs use the established fallback accent and do not crash rendering. Selecting a system also
navigates to Library when needed.

The M6-compatible filter bar includes favorites-only plus reset. Genre and region query parameters
exist, but M6.1 exposes no bounded aggregate facet options. M6.3 therefore does not fabricate
options, download all games, or add an analytics endpoint. Genre/region facet discovery is recorded
as deferred instead of adding unbounded or low-value backend work.

## Cards and Covers

`GameCard` consumes one `LibraryListItem`. It presents effective display title, system label,
optional release year, favorite state, local availability, coarse metadata state, and the opaque
`coverRef`. It never exposes physical content, hashes, provider jobs, candidates, runtime status,
BIOS status, or detail-only data.

Cards are semantic articles with a labelled heading and no fake detail navigation. The favorite is
a separate button, preventing nested interactive controls. The button exposes an accessible
pressed state and label, is disabled while its game mutation is in flight, and reports a mutation
failure without allowing local state to diverge from Rust.

The cover frame permanently reserves a 3:4 aspect ratio. Native covers use `loading="lazy"` and the
opaque `coverRef`. Missing references and image load failures render C4: the system accent fills the
frame and the effective/local title appears in centered Press Start 2P text. A protocol 404 is a
normal per-card fallback and never becomes a library query error.

Metadata state is concise. Matched items need no warning; pending, no-match, ambiguous, deferred,
failed, and stale receive short browsing-level labels. Stale items keep the list DTO's
last-known-good title and cover. Metadata state never changes the independent local availability
label; unavailable local content receives its own explicit treatment.

## Favorites

Favorite state is backend-authoritative. A toggle calls `set_game_favorite` once while the game is
locked, then refetches bounded query state and the library summary. Failure keeps the prior
authoritative card state and exposes retryable copy. When favorites-only is active, unfavoriting a
card resets to the first page before refetch so the removed item disappears coherently and no empty
out-of-range page is retained.

## Metadata Invalidation

The hook subscribes to `metadata-state-changed` through the existing IPC wrapper. The payload is
used only as an invalidation signal. IDs are retained in an effect-owned `Set`; only events for IDs
on the current visible page schedule work. A short debounce coalesces repeated and multi-game
events into one bounded current-page refetch. Off-page IDs cause no immediate query. Listener
registration handles late resolution safely, and listener/timer cleanup is effect-scoped.

## Empty, No-Results, and Error States

The M6.2 empty-library flow remains authoritative when the summary total is zero. A populated
library whose current query returns zero renders B5, echoing the active search/filter context and
offering one clear/reset action. It never shows first-run folder onboarding and never suggests a
rescan solely because filters found nothing.

Initial and retryable query errors are distinct from both empty states. Existing scan progress,
completion, and issues remain visible below the browsing surface without causing query churn.

## Visual and Accessibility Design

The implementation follows B1/B2/B3/B5/C4 and A6 using the existing token file: hard edges,
3:4 covers, pixel shadows, label/meta/body typography, system accents, scanlines, and dark/light
themes. Cards use A6 image-card hover/focus scaling and larger pixel shadows. Standalone filters and
buttons use the established inverted focus treatment. Sidebar rows keep the cursor-column language.

At the 960×640 minimum, the 232 px sidebar remains and the grid uses a minimum card width that
keeps covers legible. Controls wrap rather than overflow. At narrower unsupported/mobile-like
widths, the existing compact navigation remains usable, but desktop sizes are the design target.
Long titles are clamped in card text and wrapped within placeholders.

Search, filters, favorite actions, reset, retry, and pagination are native controls with accessible
names and visible focus. Filter buttons expose `aria-pressed`; paging announces position and uses
real buttons. Covers and placeholders have useful image semantics without duplicating the adjacent
title excessively. No button contains another interactive control.

## Testing and Documentation

Tests exercise real components/hooks with IPC and listener boundaries mocked. They cover the M6.2
loading-owner regression; initial/error/retry and multi-page query behavior; search debounce and
literal forwarding; filter resets; stale request/result/error/loading races; unmount safety;
system identities/counts/unknown systems/catalog errors; favorites including duplicate suppression
and favorites-only removal; cover success/missing/error fallback; title fallback; availability and
stale metadata; invalidation coalescing/deduplication/off-page behavior/lifecycle cleanup; one
scan-completion refresh and no progress refresh; no-results behavior; and practical semantics.

`docs/M6_REPORT.md` records the architecture, correction, tests, visual decisions, manual inspection,
verification, deferrals, and final M6.3 status. `BACKLOG.md`, `README.md`, and
`docs/DEVELOPMENT.md` are updated only where M6.3 changes current reality. No ADR is needed because
the design consumes existing application/IPC boundaries and introduces no difficult-to-reverse
cross-cutting decision.
