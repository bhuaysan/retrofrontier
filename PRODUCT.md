# RetroFrontier Product Definition

## Vision
RetroFrontier makes a local retro-game library feel like a cohesive modern console library without requiring the user to understand RetroArch configuration, cores, command lines, or emulator directory layouts.

Desired experience:

> Install RetroFrontier, add your own games and required BIOS files, and play.

## Problem
RetroArch is powerful but exposes configuration concepts that many users do not want to manage directly.

Common friction:
- installing and updating RetroArch
- selecting and maintaining cores
- managing configuration files
- locating BIOS directories
- organizing ROM libraries
- identifying games and downloading metadata
- handling multi-disc content
- dealing with conflicting pre-existing RetroArch configuration

## Product Principles

### Zero configuration by default
The default path should work without the user manually configuring RetroArch.

### Managed but isolated emulation runtime
RetroFrontier downloads and manages its own RetroArch runtime. Existing RetroArch installations must not affect it.

### Local first
V1 is local-first and accountless. No cloud account is required to use the library or launch games.

### User-owned content
RetroFrontier does not provide ROMs or copyrighted BIOS files. The user supplies their own content.

### Non-destructive V1
V1 catalogs and launches ROM content but does not automatically move, rename, convert, or delete it.

### Controller-friendly primary experience
The main library and launch experience must be usable with controller, keyboard, and mouse.

### Progressive power-user capability
Power-user features may exist, but must not make first-run setup harder.

## V1 Target Platforms
- Windows x86_64
- macOS arm64
- macOS x86_64
- Linux x86_64

Linux x86_64 is the primary development and validation platform. Windows and
macOS remain V1 targets, but must not block the initial Linux implementation.

## V1 Systems
- NES
- SNES
- Nintendo 64
- Game Boy
- Game Boy Color
- Game Boy Advance
- Sega Mega Drive / Genesis
- PlayStation
- Sega Saturn
- Sega Dreamcast
- Nintendo GameCube

## First-Run Experience
1. Start RetroFrontier.
2. Create `Documents/RetroFrontier/ROMs`.
3. Create `Documents/RetroFrontier/BIOS`.
4. Detect whether the managed emulation runtime is installed.
5. Download and verify the pinned runtime if required.
6. Prepare RetroFrontier-controlled configuration.
7. Show the library.
8. If empty, explain how to add games and offer:
   - Open ROM Folder
   - Add External ROM Folder
9. Detect newly added games.
10. Enrich metadata when online.

A traditional multi-step setup wizard is not required for V1 unless technical constraints later prove it necessary.

## Managed Library
Default ROM root: `Documents/RetroFrontier/ROMs`

Default BIOS root: `Documents/RetroFrontier/BIOS`

ROM organization may use system-specific subdirectories. BIOS discovery
currently uses a flat layout: expected BIOS filenames must be placed directly
inside `Documents/RetroFrontier/BIOS`. Discovery is non-recursive, so
system-specific nested folders such as `BIOS/PlayStation/` are not automatically
searched. Users may additionally add external ROM roots. External content stays
where it is.

## Core V1 Capabilities

### Library
- discover supported game content
- automatically detect filesystem changes
- manual rescan
- show the library according to the design
- search
- system filtering
- favorites
- game details
- launch readiness status

### Metadata
- ScreenScraper as first provider
- automatic enrichment when online
- cached local metadata
- local library remains usable offline
- provider extensibility

### Runtime
- download managed RetroArch runtime
- verify runtime
- configure isolated paths
- download/manage selected cores
- repair missing/damaged components
- update managed runtime
- rollback failed update
- never update while a game is running

### Launching
- choose system/core mapping
- validate prerequisites
- validate known BIOS requirements
- build controlled launch configuration
- launch RetroArch as a child process
- detect process exit
- return cleanly to RetroFrontier
- record play sessions

### Saves
- preserve normal game saves
- support save-state discovery/management
- record enough runtime/core information to reason about save-state compatibility later

### Input
- controller navigation for the primary UI
- keyboard navigation
- mouse interaction
- semantic actions rather than hardware-specific UI logic

## Explicitly Out of V1
Unless later promoted through a product decision:
- ROM downloads
- BIOS downloads
- cloud accounts
- cloud sync
- achievements
- netplay
- ROM conversion tools
- automatic ROM renaming/moving/deletion
- automatic duplicate cleanup
- custom system RetroArch integration
- arbitrary core-store UX
- mobile
- Wii
- PlayStation 2
- Switch
- Xbox emulation
- arcade as a first-class V1 system

## Success Criteria
V1 is successful when a new user can:
1. Install RetroFrontier.
2. Let RetroFrontier prepare its managed runtime.
3. Put supported ROMs into the expected folder.
4. See games appear in the library.
5. Receive useful metadata where available.
6. Understand when a BIOS requirement is missing.
7. Launch without manually configuring RetroArch.
8. Exit and return to RetroFrontier.
9. Repeat this reliably across supported platforms.

## Open Product Questions
Not blockers for initial architecture:
- exact scope of collections
- exact scope of statistics
- advanced shader/video overrides
- metadata editing depth
- long-term cloud/sync strategy
- long-term ROM maintenance/conversion tools
- custom RetroArch support after V1
