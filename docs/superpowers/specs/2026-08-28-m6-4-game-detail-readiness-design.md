# M6.4 Game Detail and Readiness Design

## Scope

M6.4 adds an informational game-detail route to the existing M6 library shell. A user can open one
local game, inspect its normalized metadata and cover, understand its bounded local content-unit
projection, and see the current pre-launch emulation requirements for that game's system. The page
does not launch an emulator, manage save states, expose controller controls, or provide metadata
editing/candidate-selection workflows.

The work stays on `feat/m6-library-ui` and consumes the completed M6.1/M6.2/M6.3 contracts. The
existing M6.3 corrective pass at `622614298c010ca6aa3a38c8f80624019f338bd8` is the starting point;
its historical review artifacts remain immutable.

## Readiness authority decision

The current backend already provides the required normalized readiness information through
`get_systems`. `SystemsApplicationService::get_systems` takes one
`VerifiedRuntimeSnapshot`, discovers BIOS state through `BiosService`, applies the static
`SystemCatalog` core/BIOS policy, and serializes one `SystemStatus` per catalog system. Each status
contains:

- runtime-derived core availability;
- static core policy and its resolved/unresolved decision;
- BIOS policy and per-requirement normalized status;
- Rust-computed `SystemReadiness` and normalized reason codes.

The detail screen maps the local detail's `systemId` to this already-loaded response. It does not
add `get_game_readiness`, call runtime/core/BIOS APIs separately, infer readiness from paths, or
reproduce policy in React. The `useSystemCatalog` hook retains the full `SystemStatus` objects in
addition to its existing sidebar labels. A readiness retry refreshes this same catalog contract.

This is a system-level pre-launch requirements snapshot, not launch validation and not a promise
that a future M7 launch will succeed. Local content availability remains a separate prerequisite
and is presented before emulation readiness.

## Routing and browsing context

The lightweight history layer is extended from `/library` and `/settings` to a discriminated game
route. Valid game IDs are positive safe integers. A path under `/games/` whose ID is missing,
non-numeric, non-positive, or outside JavaScript's safe integer range becomes an invalid detail
state rather than throwing or silently becoming a library page. Other unknown paths retain the
existing normalization to `/library`.

`routePath` emits canonical paths for valid routes and preserves an encoded invalid game segment
long enough to render the safe invalid-detail state. `pushState` and `popstate` remain the only
navigation mechanisms; no router dependency is introduced. A valid direct deep link loads detail
without a preceding library visit. A nonexistent valid ID is distinguished from a malformed ID by
the null result of the bounded local detail command.

Each card gets a dedicated title/detail anchor with an `/games/:id` href. A normal unmodified click
is intercepted to call the existing history navigation callback; modified clicks retain normal
browser link behavior. The Favorite button remains a sibling of the anchor, so there are no nested
interactive elements. The library query hook stays mounted across route changes, and re-enabling it
refreshes its latest committed offset instead of unconditionally returning to page one. A small
return-focus ID lets the library restore the originating card link when it is still visible, with
the library heading as fallback.

## Detail data architecture

`useGameDetail` is a focused local state boundary for the active valid game ID. It invokes only:

1. `get_library_game_detail({ gameId })` for the bounded M6.1 local projection;
2. `get_game_metadata({ gameId })` for the authoritative M5 normalized metadata state.

The two channels have independent loading flags, request generations, errors, retries, and
retained successful data. A metadata rejection never blanks local content or readiness. A
readiness/catalog rejection never blanks local content or metadata. An initial local rejection
shows a page-level retryable error; a local refresh rejection retains the last successful detail
while exposing a refresh error. A successful null local detail becomes a stable game-not-found
state and supplies a path back to Library.

The hook projects only presentation-relevant metadata and media fields to the page: status,
normalized metadata, safe provenance/credit, and opaque `mediaRef`/asset state. The component does
not render provider IDs, candidate lists, job queues, failure internals, authenticated URLs, raw
provider payloads, or any local hashes/fingerprints.

Favorite on detail uses the existing `set_game_favorite` command. The returned backend state is
the only committed detail state; there is no optimistic toggle. A successful mutation triggers the
existing summary refresh callback, and the library query re-reads its authoritative page when the
user returns.

## Local content presentation

The detail page uses `LibraryGameDetail.contentUnits` without joining or fetching physical files.
For every unit it shows the bounded unit ID/context, kind, local title, primary relative path, file
count, and unit availability. Unit kinds retain their semantics: single-file, CHD, CUE/BIN, GDI,
and M3U are labelled distinctly, and a multi-file or playlist unit remains one content unit rather
than being flattened into an arbitrary ROM path. Root context is represented by the opaque bounded
root identity, not a resolved filesystem path.

The local game availability row is independent from both metadata and readiness. Missing or
unavailable content is visibly prioritized in the overall summary even if the system requirements
snapshot is otherwise satisfied.

## Metadata and covers

The detail page uses the M5 normalized fields when present: title, synopsis, release date,
developer, publisher, genre, players, and region. The effective title falls back to the M6.1 local
title. Provider/source credit is shown only from the normalized provenance fields. Metadata status
has explicit copy for pending, matched, stale, no match, ambiguous, deferred, and failed.

Stale state continues to render its cached normalized record and cached cover while identifying the
need for revalidation. Pending is not presented as failure. No match, ambiguous, deferred, and
failed remain metadata states on a valid local game and never change the local availability row.

The card and detail page share one cover component. It accepts only an opaque `coverRef`, uses a
lazy card image and controlled detail image, and renders the C4 system-accent title placeholder
when the reference is absent or the image fails. An image failure is local to that cover and does
not trigger retries or metadata reloads. A new authoritative metadata object or changed cover
reference can clear the prior failure so an invalidated cover can recover.

## Readiness presentation

The readiness panel renders four independent rows:

- `LOCAL CONTENT`: available or unavailable from `LibraryGameDetail`;
- `RUNTIME`: available only for the runtime states that the existing Rust contract treats as
  usable, otherwise a truthful state such as unavailable or not installed;
- `CORE`: resolved/default core available, missing, or policy unresolved according to
  `SystemStatus.core` and Rust reason codes;
- `BIOS`: not required, present/verified, missing, invalid, or identity not covered according to
  `SystemStatus.bios` and its normalized requirement states.

The overall panel uses `EMULATION REQUIREMENTS SATISFIED` only when local content is available and
Rust's `SystemReadiness.ready` is true. Otherwise it prioritizes local unavailability, then maps
Rust reason codes to user-readable missing/unavailable/unknown copy. It never says `READY TO PLAY`
and never claims that a launch has been validated. Status is conveyed with text as well as color.

If the system catalog is loading, the readiness section says it is checking. If it fails or the
game's system is not in the catalog, the readiness section reports unknown/unavailable while the
local and metadata sections remain usable. A missing BIOS offers navigation to existing Settings
only; M6.4 does not scan, copy, delete, download, or open arbitrary filesystem paths.

## Event and refresh behavior

The detail metadata listener subscribes to the existing `metadata-state-changed` event only while
the valid detail page is active. It ignores events for other games, retains no provider payload,
coalesces repeated current-game invalidations behind the existing short trailing debounce, and
refetches only `get_game_metadata` after the timer. Cleanup clears the timer and set, unregisters a
resolved listener, and unregisters immediately if registration resolves after unmount.

The existing scan state reports one deduplicated terminal run ID. AppShell refreshes the system
catalog once for that terminal run, while `useGameDetail` refreshes the active game's bounded local
detail and metadata once. Scan progress does not trigger detail work. If the refreshed local detail
returns null, the page moves to its stable not-found/unavailable state.

No polling is added. BIOS/runtime changes are reflected by the existing bounded readiness refresh
on page entry, a terminal scan refresh, or an explicit retry.

## Visual and accessibility design

The detail layout follows B6's existing RetroFrontier composition: the established shell and
tokens, semantic back context, a large 3:4 cover, prominent title, system/status tags, readable
synopsis, metadata panel, readiness rows, and local content-unit section. B6 save states and
screenshots are omitted. The B6 launch action is omitted rather than represented by a fake disabled
CTA. A6 V5 is applied: card/detail image surfaces zoom on focus, standalone controls use the
existing inverted focus treatment, and list navigation keeps its cursor language.

C4 supplies the larger title placeholder. New light-theme surfaces use existing token combinations
with readable text and do not introduce accent text known to be low contrast. Long titles,
synopses, and relative paths wrap or ellipsize without horizontal overflow. The layout collapses
the hero at the existing desktop breakpoints and remains usable at 960×640 and at compact widths.

The detail main heading is focusable with `tabIndex={-1}` and receives focus after route entry and
after the first local result/error state is committed. Back is a real link with an explicit
`/library` destination. Cover fallback has useful image semantics, readiness uses visible status
text, Favorite has a game-specific accessible name and pressed state, and retry controls are native
buttons. No M8 controller focus graph or footer is added.

## Testing and documentation

Frontend tests cover route parsing/canonicalization, card links and non-nested semantics, direct
deep links, malformed/nonexistent games, browser and explicit Back, browsing-context/focus
restoration, local/metadata/readiness partial failures and retries, metadata status copy, stale
cover retention and recovery, all readiness states exposed by the current DTO, authoritative
Favorite mutation, targeted/coalesced metadata events including late cleanup, scan progress versus
terminal refresh, and accessible headings/statuses.

No Rust production change is planned and therefore no new backend readiness test matrix is needed;
existing Rust system/readiness tests remain part of the full verification suite. `docs/M6_REPORT.md`
records the readiness reuse decision, exact IPC sources, state separation, events, design coverage,
tests, verification, and deferrals. Historical review artifacts are not modified.

