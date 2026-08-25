# RetroFrontier Domain Model

## Purpose
This document defines the core domain language for RetroFrontier. The model must avoid treating a game as a single filesystem path.

## Core Concepts

### System
An emulated platform supported by RetroFrontier.

Owns configuration such as:
- stable identifier
- display name
- managed ROM folder name
- supported content formats
- BIOS requirements
- default core policy
- launch capabilities

### Game
A logical library entry presented to the user.

May contain:
- title
- sort title
- system
- release year
- developer
- publisher
- genre
- region information
- description
- favorite state
- metadata-source identifiers

A Game is not a filesystem file.

### Content Unit
A playable representation associated with a Game.

May represent:
- a single ROM file
- a CHD image
- a CUE/BIN set
- an M3U playlist
- a multi-disc set
- another system-specific launchable content representation

A Game may have more than one Content Unit, for example different regions or revisions.

### Content File
A physical file on disk that belongs to a Content Unit.

Examples:
- `.sfc`
- `.gba`
- `.cue`
- `.bin`
- `.chd`
- `.m3u`
- `.iso`
- `.rvz`

Conceptual fields:
- content unit
- content root
- relative/canonical path
- size
- modified timestamp
- hashes where available
- file role
- availability status

### Content Root
A directory scanned by RetroFrontier.

Kinds:
- managed ROM root
- external ROM root

Conceptual fields:
- path
- kind
- enabled
- optional system hint
- last scan timestamp

### Disc
An ordered disc within a multi-disc Content Unit.

The persistence model may use playlist ordering where appropriate, but the domain must preserve disc ordering.

### Metadata Record
Metadata obtained from a provider.

Provider-specific payloads must not leak throughout the application.

### Media Asset
Local cached artwork/media associated with a Game, such as cover, screenshot, logo, or background.

### Core
A RetroArch/libretro core available to the managed runtime.

Tracks:
- stable identifier
- display name
- license
- supported systems
- platform/architecture support
- installed version
- runtime component reference

### BIOS Requirement
A known firmware requirement for a System or Core.

Tracks:
- expected filename
- accepted hashes
- required/optional status
- user-facing description
- internal runtime destination

### BIOS File
A user-supplied local firmware file discovered by RetroFrontier.

RetroFrontier must not download copyrighted BIOS files.

### Runtime Release
A RetroFrontier-approved set of emulation-runtime components.

May include:
- RetroArch version
- core versions
- support assets
- platform/architecture
- download locations
- integrity metadata
- compatibility constraints

### Runtime Installation
The locally installed managed runtime.

Possible states:
- not installed
- installing
- ready
- update available
- updating
- damaged
- repairing
- rollback available

### Play Session
A recorded game execution with game/content/core/runtime/time/exit information.

### Save Data
Normal emulator-managed persistent save data such as SRAM or memory-card data.

Save Data is user data and must survive runtime replacement.

### Save State
An emulator snapshot associated with a game.

Track core and runtime versions because compatibility is not guaranteed across versions.

### Game Override
Optional per-game launch configuration such as:
- core override
- video behavior
- aspect/integer scaling
- shader choice
- input behavior

Overrides must not mutate unrelated user RetroArch configuration.

## Relationships

```text
System
  ├── Core mappings
  └── BIOS requirements

ContentRoot
  └── ContentFile
        └── ContentUnit
              └── Game

Game
  ├── MetadataRecord
  ├── MediaAsset
  ├── PlaySession
  ├── SaveData
  ├── SaveState
  └── GameOverride

RuntimeRelease
  └── RuntimeInstallation
        └── installed Core components
```

## Domain Rules
1. A Game must never be identified solely by an absolute file path.
2. Removing a file must not automatically delete user metadata without reconciliation.
3. Runtime replacement must not delete ROMs, BIOS files, saves, states, metadata, or the database.
4. External ROM roots are read-only from the perspective of automatic V1 maintenance.
5. A scan must be safe to repeat.
6. Filesystem discovery and metadata identification are separate concerns.
7. A game may exist before metadata lookup succeeds.
8. BIOS validation should produce a user-actionable state rather than a cryptic launch failure.
9. Launching resolves an explicit managed runtime, core, content unit, and configuration.
10. Provider-specific metadata identifiers remain behind a provider abstraction.

## Identification Inputs
May combine:
- managed-folder system context
- extension/content-format knowledge
- filename
- file size
- CRC32
- MD5
- SHA-1
- playlist relationships
- provider results

Exact matching rules remain an implementation task.

## Persistence
SQLite is the intended store. The final schema must be introduced through migrations and should follow this domain rather than mirror UI components.
