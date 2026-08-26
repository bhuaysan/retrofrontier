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
relationship-resolution → hashing → transactional per-root reconciliation pipeline. `Game`,
`ContentUnit`, `ContentFile`, and `ContentRoot` are separate domain concepts backed by the
`content_roots`, `games`, `content_units`, `content_files`, `content_unit_files`, `scan_runs`, and
`scan_issues` tables. SQL remains behind `LibraryRepository`; Tauri commands expose only typed
application use cases and snapshots.

The managed root is bootstrapped from the OS document directory and the static catalog's explicit
managed-folder names. External roots are read-only to automatic maintenance. Filesystem watchers
only schedule a debounced real scan; they never mutate rows directly. See
[`docs/LIBRARY_SCANNER.md`](docs/LIBRARY_SCANNER.md) for the reconciliation and relationship
contract.

### MetadataService
- provider abstraction
- lookup queue
- normalized metadata
- media downloads
- retry/backoff
- cache
- failed/deferred state

Initial adapter: `ScreenScraperProvider`.

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
policy may name one default and approved alternatives, but the current matrix keeps each V1 choice
explicitly unresolved where documentation has not approved a core. RetroFrontier never treats a
catalog entry as installed and never accepts system RetroArch cores, arbitrary user core paths, or
user-downloaded cores.

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

Credential handling for ScreenScraper remains a research item.

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

## Database
SQLite migrations are authoritative. Repositories hide SQL from the rest of the application.

Avoid giant repository/command modules.

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
2. TUF client implementation and production key-custody ceremony.
3. Runtime Release hosting/control and redistribution model.
4. ScreenScraper credential strategy.
5. Final default-core matrix/version policy.
6. Filesystem watcher implementation.
7. Save-state rollback/compatibility behavior.
8. macOS Developer ID, notarization, quarantine, and core library-validation proof.
9. Windows Authenticode/Smart App Control and pointer-durability proof.
10. Linux cross-distribution/device release matrix and packaging validation after the extracted-AppImage/AppRun qualification.
