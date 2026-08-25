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

Scanning should be idempotent.

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
- discover user BIOS files
- hash/validate
- map user-friendly BIOS storage to core-required layout
- report actionable missing/invalid states

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
└── BIOS/
```

### Application data
Use OS-appropriate app-data paths:

```text
RetroFrontier/
├── runtime/
│   ├── versions/
│   ├── staging/
│   ├── transactions/
│   ├── active.json
│   └── previous.json
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
│   ├── <release-a>/
│   └── <release-b>/
├── staging/
├── transactions/
├── active.json
└── previous.json
```

Safe update:
1. Resolve approved Runtime Release.
2. Download into staging.
3. Verify integrity/authenticity according to final security design.
4. Validate required components.
5. Ensure no game is running.
6. Atomically replace the small active pointer.
7. Run a bounded smoke test and record candidate health.
8. Retain rollback capability and reconcile interrupted transactions at startup.
9. Clean old releases according to retention policy.

Runtime versions are immutable after validation. Runtime-user configuration, core options, cache, logs, saves, states, screenshots, metadata, and the database remain outside replaceable versions. The cross-platform activation authority is an app-owned pointer file containing a safe release ID, generation, and manifest identity; symlinks and junctions are not required. Exact replacement is platform-specific and must use same-filesystem temporary files plus startup recovery.

The Linux spike found that explicit core, save, and system directories alone are insufficient: core-info cache and core options also need explicit managed paths or disabling.

## Runtime Manifest
Do not blindly download "latest".

A RetroFrontier-controlled manifest should define approved components per platform/architecture, including:
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

Signing/authenticity details remain open.

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
2. Runtime manifest authenticity/signing.
3. Runtime Release hosting/control model.
4. ScreenScraper credential strategy.
5. Final default-core matrix/version policy.
6. Filesystem watcher implementation.
7. Save-state rollback/compatibility behavior.
