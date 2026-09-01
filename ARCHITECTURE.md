# RetroFrontier Architecture

## Goals

Support:

- Windows, macOS, and Linux desktop UI
- fully isolated managed RetroArch runtime
- safe local library scanning
- metadata enrichment
- controller-oriented navigation
- multi-file and multi-disc content
- runtime updates and repair
- strict separation of user data from replaceable runtime files

## Technology Direction

### Desktop

Tauri 2

### Frontend

- React
- TypeScript
- Vite
- project-owned design-system components
- CSS driven by the existing RetroFrontier design tokens

### Backend

Rust

### Persistence

SQLite via `sqlx`

### Package manager

pnpm

## Architectural Boundary

React is the presentation layer. Rust is the application/native boundary.

```text
React / TypeScript
  Library, details, settings, runtime status, scan UI
            |
            | typed Tauri IPC
            v
Rust Application Layer
  commands/use cases/orchestration
            |
     Domain / Services / Repositories
            |
            v
Adapters
  SQLite / Filesystem / RetroArch / Runtime / ScreenScraper / OS
```

React must not query SQLite directly.

## Suggested Repository Structure

```text
retrofrontier/
├── src/
│   ├── app/
│   ├── components/ui/
│   ├── features/
│   ├── hooks/
│   ├── platform/
│   └── styles/
├── src-tauri/
│   ├── migrations/
│   └── src/
│       ├── commands/
│       ├── application/
│       ├── domain/
│       ├── services/
│       ├── repositories/
│       └── adapters/
├── docs/adr/
├── PROJECT_CONTEXT.md
├── PRODUCT.md
├── DOMAIN.md
├── ARCHITECTURE.md
├── BACKLOG.md
└── AGENTS.md
```

This is directional; do not create empty folders without need.

## Key Services

### LibraryService

- library queries
- game/content reconciliation
- favorites
- game detail aggregation

### ScanService

- file discovery
- managed-folder system hints
- content relationship resolution
- hashing
- persistence
- removal/move reconciliation
- progress
- watcher signals

Scanning should be idempotent. M4 implements this service in Rust with a distinct discovery →
relationship-resolution → hashing → transactional per-root reconciliation pipeline. Discovery
produces an explicit authority snapshot of enumerated directories and protected prefixes, so
absence reconciliation is granular and cannot infer deletion from an unreadable subtree. `Game`,
`ContentUnit`, `ContentFile`, and `ContentRoot` are separate domain concepts backed by the
`content_roots`, `games`, `content_units`, `content_files`, `content_unit_files`, `scan_runs`, and
`scan_issues` tables. SQL remains behind `LibraryRepository`; Tauri commands expose only typed
application use cases and snapshots.

Reconciliation preserves `GameId` across ownership changes only when persisted content-file
memberships and exact fingerprints identify one logical predecessor. A new M3U may therefore
replace standalone surfacing under that predecessor game, but multiple predecessor games produce
an explicit ambiguity and a new game rather than a guessed merge. Move candidates are evaluated as
a complete one-to-one relationship before mutation, so filesystem, insertion, and SQLite row order
cannot decide identity.

The managed root is bootstrapped from the OS document directory and the static catalog's explicit
managed-folder names. External roots are read-only to automatic maintenance. Filesystem watchers
only schedule a debounced real scan; they never mutate rows directly. See
[`docs/LIBRARY_SCANNER.md`](docs/LIBRARY_SCANNER.md) for the reconciliation and relationship
contract.

M6.1 adds a separate UI read boundary to `LibraryApplicationService`: `query_library` returns a
hard-bounded page of list projections, `get_library_summary` returns aggregate counts, and
`get_library_game_detail` returns one local-content projection. These queries join normalized
metadata, provider state, cached-media identity, and user-owned favorite state without joining
physical content files, hashes, or fingerprints. The existing full `get_library_snapshot` remains
the M4 diagnostic contract and is not the UI list path. Favorites live in the separate
`game_user_state` table, so scanner reconciliation cannot overwrite them.

The M6.1 UI list receives an opaque cached-cover reference when the durable cover row is eligible:
`rfmedia://localhost/cover/<game-id>` on Linux/macOS desktop and
`http://rfmedia.localhost/cover/<game-id>` on Windows. Rust generates the target-correct reference,
resolves that identity through a narrow Tauri custom protocol, checks the app-owned metadata-media
cache and image signature, and serves the bytes; React never receives or resolves a cache path.
Metadata changes emit a minimal invalidation event only after their durable write, and the bounded
scan-issue page is limited to one resolved persisted scan run.

### MetadataService

- provider abstraction
- lookup queue
- normalized metadata
- media downloads
- retry/backoff
- cache
- failed/deferred state

Initial adapter: `ScreenScraperProvider`.

M5 implements this as `MetadataApplicationService` plus a provider-neutral `MetadataProvider` trait,
a `MetadataRepository`, a persistent `metadata_jobs` queue with provider-aware scheduling, an
app-owned cover cache, and thin metadata commands. Deterministic matching requires an unambiguous
system mapping and returned provider content evidence that agrees with the current M4 hashes, size,
and unit fingerprint; heuristic title results stay candidates. Provider state is bound to a versioned
evidence snapshot, so same-path content replacement marks a match stale instead of silently keeping
it trusted. The metadata repository writes to no M4 table. See
[`docs/METADATA.md`](docs/METADATA.md) for the implementation contract.

M8.5 adds a persistent orchestration layer above that queue, not a second one beside it.
`MetadataScrapeApplicationService` owns which user-initiated batch operation is in progress, its
fixed target set, bounded feeding, stop semantics and restart semantics; the M5 queue keeps
provider requests, quota, deferral, retry, matching and persistence. There is still exactly one
worker, one scheduler and one provider adapter.

Library discovery is local and does not automatically trigger first-time metadata scraping: a scan
finds content on this machine and does not decide to spend the user's provider budget on it. First
contact with the provider for a newly discovered game happens only through a scrape run the user
starts in Settings. Accepted metadata relationships are still automatically revalidated for evidence
integrity — that sweep protects relationships the user already has and is a different mechanism from
going out to fetch new ones.

### RuntimeManager

- detect managed runtime
- install approved runtime
- verify files
- repair
- stage updates
- activate safely
- rollback
- expose explicit executable/core paths

Never use a system `retroarch` from `PATH`.

### RetroArchService

- build controlled launch context
- select core
- validate prerequisites
- generate/select RetroFrontier-owned config
- launch managed executable
- monitor process
- normalize result

M7 implements this for Linux x86_64 as `RetroArchService` plus a `LaunchApplicationService`
orchestration layer. React calls one semantic `launch_game(gameId, contentUnitId?)` command; it
never supplies or receives an executable, core, BIOS, save, system, or content path. Content
resolution launches the ordinal-zero descriptor, playlist, or standalone file of an available
content unit, never a member track, and re-checks containment after canonicalization. Core
resolution takes a valid per-game override, otherwise the approved system default, and requires an
authenticated installed component whose release-declared systems include the launching system; an
invalid override never falls through. Play sessions and per-game core overrides are persisted in
their own user/product-owned tables.

`RuntimeManager` keeps every existing responsibility and exposes only `verified_launch_runtime()`
(absolute AppRun, core, and support-asset paths from the same single verification that produces
runtime status) and `lock_for_launch()` (the existing runtime mutation lock). See
[`docs/RETROARCH_LAUNCH.md`](docs/RETROARCH_LAUNCH.md).

### BiosService

- discover user BIOS files under the OS-resolved `Documents/RetroFrontier/BIOS` root
- hash and validate against catalog identities when authoritative values exist
- report missing, invalid, optional, and not-covered BIOS states
- never modify, move, rename, delete, download, or execute user BIOS files

### Systems, core policy, and readiness

`SystemCatalog` is application-owned static product knowledge. It defines stable `SystemId`
values, display metadata, aliases, normalized content extensions, BIOS requirements, and core
policy. Display names and aliases are lookup conveniences only; they are never database/domain
identifiers. The catalog is validated at startup and in unit tests, and static system/core policy
is not duplicated in SQLite.

`CoreDefinition` and `CorePolicy` model approved managed cores separately from runtime state. A
policy may name one default and approved alternatives. M7 resolves policy for the four reference
systems — NES/Nestopia, SNES/bsnes-mercury Balanced, PlayStation/Beetle PSX, and GameCube/Dolphin —
and the remaining seven V1 systems stay explicitly unresolved, so no fallback can make them
launchable. A `CoreDefinition` records its libretro name, licence, upstream source, supported
platform targets, the authenticated managed component that installs it, and any managed support
component it requires. Verified runtime component identifiers are translated through the catalog, so
an installed but unapproved component is never reported as an available core. RetroFrontier never
treats a catalog entry as installed and never accepts system RetroArch cores, arbitrary user core
paths, or user-downloaded cores.

`RuntimeManager::verified_snapshot()` is the read-only boundary for runtime availability. It
returns the effective runtime status and core component IDs from one active-installation
verification. `RuntimeApplicationService` exposes that coherent snapshot to systems/readiness;
the query does not apply system policy or trust static catalog data. The older
`current_verified_core_ids()` compatibility method delegates to the same snapshot boundary.

`BiosService` owns discovery orchestration but not the BIOS files. Its default root is constructed
from Tauri's OS-specific document directory and `RetroFrontier/BIOS`; it does not hard-code a home
directory and it never creates or repairs that user-data folder. Production discovery examines
only explicitly declared filenames directly below the selected root. A development/test caller may
pass one explicit absolute root override; release IPC rejects that override. The service rejects
relative roots and symlink roots, tolerates unrelated files, and hashes candidates read-only.
This flat layout is also the current user-facing policy: system-specific nested BIOS folders are not
automatically searched. Supporting nested BIOS layouts remains a follow-up and is not part of M3.

`SystemReadiness` combines the resolved core policy, verified runtime availability, and required
BIOS status into inspectable reasons. ROM/game availability is now owned by the M4 library service,
not readiness. The M3/M4 IPC surfaces expose one coherent systems response, a development-only BIOS
status query, typed library root/snapshot queries, and scan progress/completion events; React does
not inspect the filesystem, hash content, or query RuntimeManager.

### SaveService

- resolve save directories
- track save states
- preserve user save/state data across runtime updates
- attach runtime/core compatibility metadata

## Managed Paths

### User-visible

```text
Documents/RetroFrontier/
├── ROMs/
│   ├── NES/
│   ├── SNES/
│   ├── Nintendo 64/
│   ├── Game Boy/
│   ├── Game Boy Color/
│   ├── Game Boy Advance/
│   ├── Mega Drive/
│   ├── PlayStation/
│   ├── Sega Saturn/
│   ├── Sega Dreamcast/
│   └── Nintendo GameCube/
└── BIOS/
```

### Application data

Use OS-appropriate app-data paths:

```text
RetroFrontier/
├── runtime/
│   ├── versions/
│   ├── staging/
│   ├── locks/
│   ├── active.json
│   └── game-process.json
├── runtime-trust/
├── runtime-user/
├── database/
├── metadata/
│   ├── media/
│   └── tmp/
├── saves/
├── states/
├── screenshots/
└── logs/
```

Use platform path APIs; do not hard-code OS paths.

## Runtime Architecture

RetroArch is downloaded after installation.

Conceptual layout:

```text
runtime/
├── versions/
│   ├── <installation-a>/
│   └── <installation-b>/
├── staging/
├── locks/
├── active.json
└── game-process.json
```

Trusted TUF metadata and the highest observed anti-rollback floors live in the sibling `runtime-trust/` subtree. Runtime uninstall, repair, rollback, and ordinary cache cleanup do not remove that security state; only an explicit whole-application-data reset may do so.

The managed runtime, activation metadata, and trust state require a local application-data filesystem with supported locking and same-directory replacement semantics. V1 does not place them on a network share or cloud-synchronized root; external ROM roots remain a separate concern.

For Linux x86_64, the managed runtime artifact is an extracted RetroArch AppDir. The launch path is the authenticated AppDir-defined `AppRun` entry point, for example `versions/<installation-id>/runtime/<appdir>/AppRun`; production code must not infer or substitute an inner path such as `usr/bin/retroarch`. The tested RetroArch artifact uses an `AppRun` symlink and an ELF `$ORIGIN/../lib` runpath, but other authenticated AppDirs may use a script or another executable and may establish environment variables.

The extracted AppDir is relocatable for its bundled libraries, not self-contained for Linux host services. The Linux launch adapter must validate host prerequisites and preserve the user's display/session environment while explicitly controlling RetroFrontier paths. glibc/ELF loader, libstdc++, desktop graphics (OpenGL/EGL/GBM/DRM and optionally Vulkan), display libraries, audio services, and udev/input device permissions remain host responsibilities. The Linux distribution and device matrix is a release gate; see `docs/spikes/LINUX_RUNTIME_QUALIFICATION.md`.

Safe update:

1. Resolve a Runtime Release through trusted update metadata.
2. Download into a private, operation-specific staging directory.
3. Verify trusted metadata, exact size, and SHA-256 before extraction.
4. Extract safely and validate the complete authenticated file inventory, including any format-approved internal link targets.
5. Finalize a uniquely named candidate directory, run a bounded smoke test from that exact path, and revalidate the tree.
6. Write and flush its completion marker last; the version is immutable from that point.
7. Acquire the runtime mutation lock, ensure no managed game process is running, and reduce complete installations to the current selection plus the candidate.
8. Atomically replace the small active pointer; the former current installation is then the sole rollback candidate.
9. Clean only owned, incomplete or inactive runtime paths according to policy.

Runtime versions are immutable after their completion marker is committed. Runtime-user configuration, core options, cache, logs, saves, states, screenshots, metadata, and the database remain outside replaceable versions. The cross-platform activation authority is an app-owned pointer file containing only a schema version, safe installation-directory identifier, and canonical release-manifest SHA-256. Symlinks and junctions are not required. Exact replacement is platform-specific and must use same-directory temporary files, file and directory durability primitives where available, and startup validation.

No authoritative update transaction journal is required for V1. Incomplete staging directories, incomplete version directories, complete inactive versions, and the active pointer are distinguishable from filesystem structure and completion markers. Resumable downloads may keep disposable metadata inside their own staging directory.

The M2 Linux implementation is in the Rust `RuntimeManager` application boundary and its runtime
adapters. It derives status from the authoritative pointer, persisted trust state, completion
markers, and strict installed-file inventory; it does not infer a runtime from directory order. The
default composition root deliberately has no production release source configured yet. Explicit
synthetic sources are available for tests, while the TUF-backed source adapter is ready to receive
an approved root and repository configuration. See `docs/RUNTIME_MANAGER.md` for the implementation
contract.

RetroFrontier is single-instance per OS user in V1. An OS-backed runtime mutation lock protects install, activation, rollback, repair, and cleanup even from an accidentally started second or older process. A durable game-process identity record plus liveness validation prevents activation after RetroFrontier crashes while its managed RetroArch process remains alive.

M7 holds that same lock across the launch sequence, from before runtime verification until the
durable `running` process record is committed, so an activation cannot interleave with the
verification-to-spawn window. The record is written in a conservative pre-spawn `launching` phase
first, closing the crash window between `exec` and persisting a PID. Only one managed game process
may be active per user instance; an in-process launch mutex, in-process active-game state, the
durable record, and the mutation lock cooperate, and a second attempt returns `gameAlreadyRunning`.
Play-session history is product data and is never the authority on whether a process is alive.

The Linux spike found that explicit core, save, and system directories alone are insufficient: core-info cache and core options also need explicit managed paths or disabling.

## Runtime Manifest

Do not blindly download "latest".

A RetroFrontier-controlled canonical release manifest defines approved components per platform/architecture, including:

- runtime release
- platform
- architecture
- RetroArch version
- component source
- component version
- size
- hash
- license metadata
- compatibility

The manifest and every downloadable component are immutable targets authenticated by a TUF 1.0-compatible metadata repository. Per-component target names, lengths and hashes, extracted executable/native-library hashes or an authenticated file-inventory digest, format-specific link policy, extraction policy, launch paths, core allowlist, and OS code-signing requirements are inside the authenticated release description. V1 uses SHA-256 and Ed25519. The application ships a trusted TUF root; root and targets roles use offline threshold keys, while snapshot and timestamp roles provide consistency and freshness. HTTPS remains required but is not the authenticity root. ADR-012 defines the trust and anti-rollback policy.

Installed runtimes do not expire merely because the device is offline. Expiration applies when discovering or downloading updates. Persisted trusted metadata versions, a monotonic release sequence, authenticated revocations, and an authenticated minimum-safe release sequence prevent replay and vulnerable rollback to the extent of the freshest metadata the client has received.

## RetroArch Isolation

Every launch must use explicit RetroFrontier-controlled paths.

Do not rely on:

- system `PATH`
- system config discovery
- existing core directories
- existing save directories

The Rust launch contract must resolve an absolute managed executable and pass an explicit config, log path, core path, and content path. It must explicitly control libretro, core-info, system/BIOS, save, state, screenshot, assets, shader, playlist, cache, history, remap, autoconfig, core-option, and runtime-log paths as applicable. The child environment is constructed rather than blindly inherited. This isolates configuration and data paths; it is not a sandbox for native cores.

M7 implements this as `AppRun --config <generated config> -L <managed core> <content target>`. There
is exactly one generated configuration file, rewritten atomically before every launch, so no
per-game configuration files exist. Every writable RetroArch directory lives under `runtime-user/`,
`saves/`, `states/`, `screenshots/`, or `logs/retroarch/`; `libretro_directory` is the only value
pointing into the verified immutable version tree because RetroArch only reads it. `system_directory`
is a RetroFrontier-owned directory into which validated user BIOS files and verified managed support
data such as Dolphin's `Sys` are linked, so user data never enters an authenticated runtime tree and
user BIOS files are never modified, moved, renamed, or copied. The child environment is an allowlist
of the desktop session variables the Linux qualification proved necessary plus a fixed minimal
`PATH` and RetroFrontier-owned `XDG_*` base directories; `LD_PRELOAD`, `LD_LIBRARY_PATH`, and any
`RETROARCH*`/`LIBRETRO*` variable are absent by construction. Missing host graphics, audio, or input
capabilities are normalized into launch diagnostics rather than treated as a damaged runtime; only a
missing display session blocks a launch.

## Scanner Pipeline

```text
Content root
  -> filesystem discovery
  -> system/content hints
  -> relationship resolution
  -> hashing
  -> local reconciliation
  -> metadata queue
  -> media caching
  -> library update
```

Filesystem discovery and metadata enrichment are separate.

## Metadata Architecture

Define a provider interface at the application/domain boundary. Provider-specific responses are normalized before reaching the UI.

ScreenScraper V1 is a direct Rust adapter with no RetroFrontier cloud proxy. Release application
credentials are build-time injected outside source control and are treated as recoverable
application credentials. Optional personal credentials remain Rust-owned in the OS vault/keychain;
SQLite and ordinary read IPC contain no secret.

M5 persists replaceable normalized metadata, provider/source identity, evidence-bound match state,
provider-aware jobs/quota deferrals, and one primary local cover in `metadata/media/`. It avoids raw authenticated
response and credential-bearing URL persistence, broad media scraping, bundling, and redistribution.
Automatic matching requires agreeing returned ROM evidence; heuristic name results remain
candidates. Unsupported container representations are deferred. Provider failures and changed M4
evidence affect provider state only and never mutate local library ownership or availability. See
[`docs/SCREENSCRAPER_SPIKE.md`](docs/SCREENSCRAPER_SPIKE.md) and ADR-007.

The M6.1 metadata/UI boundary preserves the relative cache path only inside Rust persistence and
native services. Serialized metadata assets expose an opaque media reference instead. The strongest
media boundary is that the WebView supplies only an opaque game identity; path containment checks
remain defence-in-depth against corrupted persisted cache paths. The `metadata-state-changed` event
carries only `gameId` and `providerId`; it is a per-game invalidation signal, not a metadata data
channel. Bulk consumers must debounce/coalesce events and refetch bounded visible/current state
rather than refetching the whole library once per event.

## UI Architecture

Implement the existing design system with project-owned primitives rather than replacing it with a large generic component framework.

Likely primitives include:

- PixelButton
- PixelToggle
- PixelSelect
- PixelDialog
- PixelProgress
- GameCard
- FocusRow
- ControllerFooter
- EmptyState

Names are not contractual.

## Input Architecture

Map hardware input to semantic actions:

- NavigateUp
- NavigateDown
- NavigateLeft
- NavigateRight
- Confirm
- Back
- Context
- Search
- Menu

M8 implements the navigation subset of this as `InputAction`
(`moveUp`/`moveDown`/`moveLeft`/`moveRight`/`confirm`/`back`/`context`). Physical input is acquired
by two adapters — a keyboard adapter and a browser Gamepad API adapter — behind one replaceable
acquisition boundary; ADR-014 records why the browser API was chosen and what would justify a native
adapter. Focus and navigation code consumes semantic actions only, and physical key names and
gamepad button indices exist in exactly one module each.

Above the boundary sit a focus registry keyed by stable semantic identities (`GameId`, system id,
route, Game Detail action, `ContentUnitId`, Settings root action), geometry-derived spatial
navigation that reads the rendered layout rather than assuming a column count, temporary focus
scopes for transient surfaces, and footer hints derived from the focused node's declared actions.
Focus restoration uses semantic identity and a settle signal from the owning surface; it never uses
a DOM query with a timeout, and it never polls.

Controller actions are delivered only while the RetroFrontier window owns focus and the backend
reports no running or uncertain managed game. While RetroArch is authoritative RetroFrontier
consumes nothing and does not fight the window manager; when the backend reports the game ended it
asks for the foreground once through the Tauri window API and restores DOM focus only after the
window is really focused. See [`docs/CONTROLLER_AND_FOCUS.md`](docs/CONTROLLER_AND_FOCUS.md).

## Database

SQLite migrations are authoritative. Repositories hide SQL from the rest of the application.

Avoid giant repository/command modules.

Because M5 adds background metadata writes alongside interactive library operations, the pool opens
with WAL journaling, `NORMAL` synchronous mode, a busy timeout, and enforced foreign keys, and every
writer stays short. No provider request is issued while a transaction is open. ADR-013 records the
decision and its trade-offs.

## Error Handling

Normalize errors into actionable states:

- runtime unavailable/damaged
- download unavailable
- unsupported content
- BIOS missing/invalid
- metadata deferred
- launch failed
- external content missing

## Testing

Rust unit tests:

- domain rules
- parsers
- runtime manifest validation
- BIOS matching
- content relationships

Rust integration tests:

- SQLite repositories
- migrations
- scanner reconciliation
- runtime staging/verification using synthetic fixtures

Frontend tests:

- focus/navigation
- state rendering
- key library interactions

Cross-platform smoke tests before V1:

- runtime install
- runtime launch
- core load
- controller
- audio/video
- saves
- return after emulator exit

## Security Boundaries

Security-sensitive:

- runtime downloads
- manifest authenticity
- integrity verification
- archive extraction
- executable launch
- updater logic
- credentials

These areas trigger focused Sol Max review.

## Open Architecture Decisions

1. Portable/runtime distribution per OS.
   (SQLite write concurrency is resolved by ADR-013.)
2. TUF client implementation and production key-custody ceremony.
3. Runtime Release hosting/control and redistribution model.
4. Final default-core matrix/version policy.
5. Filesystem watcher implementation.
6. Save-state rollback/compatibility behavior.
7. macOS Developer ID, notarization, quarantine, and core library-validation proof.
8. Windows Authenticode/Smart App Control and pointer-durability proof.
9. Linux cross-distribution/device release matrix and packaging validation after the extracted-AppImage/AppRun qualification.
