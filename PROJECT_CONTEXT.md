# RetroFrontier Project Context

## Status
RetroFrontier is a new greenfield project. Previous RetroFrontier implementations, experiments, repositories, and old RF stories are abandoned and must not be treated as current project state. The current Git repository and the planning documents in it are the source of truth.

## Product
RetroFrontier is a desktop frontend, game library, and ROM management application for RetroArch.

The primary product goal is a zero-configuration experience:
1. The user installs and starts RetroFrontier.
2. RetroFrontier creates its managed user folders.
3. RetroFrontier downloads and manages its own isolated RetroArch runtime.
4. The user copies ROMs and, where legally required, BIOS files into the provided folders.
5. RetroFrontier discovers, identifies, enriches, and launches games.

An existing local RetroArch installation or configuration must not influence RetroFrontier.

## V1 Target Platforms
- Windows x86_64
- macOS arm64
- macOS x86_64
- Linux x86_64

Linux x86_64 is the primary development and validation platform. Windows and
macOS remain V1 targets, but must not block the initial Linux implementation.

## V1 Systems
- Nintendo Entertainment System (NES)
- Super Nintendo Entertainment System (SNES)
- Nintendo 64
- Game Boy
- Game Boy Color
- Game Boy Advance
- Sega Mega Drive / Genesis
- Sony PlayStation
- Sega Saturn
- Sega Dreamcast
- Nintendo GameCube

## Managed User Library
Default user-visible folders:
- `Documents/RetroFrontier/ROMs`
- `Documents/RetroFrontier/BIOS`

Users may additionally configure external ROM directories.

V1 must not automatically rename, move, convert, or delete ROM files.

## Emulation Runtime
RetroArch is **not bundled** in the RetroFrontier installer.

RetroFrontier downloads, installs, verifies, updates, repairs, and launches its own isolated RetroArch runtime. Runtime files and user data must remain strictly separated.

Runtime updates must support:
- version pinning
- integrity verification
- staging
- safe activation
- repair
- rollback
- recovery from interrupted updates

Exact download sources, component packaging, and signing details require a technical spike.

## Metadata
ScreenScraper is the first metadata provider. The architecture must allow additional providers later. Games discovered while offline must remain usable and may be enriched later.

ScreenScraper developer-credential handling in an open-source desktop application remains an explicit research item.

## Preferred Technology Stack
- Tauri 2
- Rust
- React
- TypeScript
- Vite
- SQLite
- sqlx
- pnpm

React owns presentation and user interaction.

Rust owns filesystem access, ROM discovery and hashing, database persistence, metadata-provider integration, RetroArch integration, runtime management, process management, and OS integration.

SQLite must be accessed through the Rust application layer rather than directly from React.

## Domain Principles
A `Game` is not the same entity as a ROM file.

RetroFrontier manages logical games and playable content units, not just a flat list of files.

The domain must support:
- one game with multiple physical files
- multi-disc games
- CUE/BIN
- CHD
- M3U playlists
- different regions and revisions
- managed and external ROM locations

## UX Principles
RetroFrontier should require as little configuration as possible.

The main application experience must support controller, keyboard, and mouse navigation. Native operating-system dialogs may be used for filesystem selection and other complex OS interactions.

The existing RetroFrontier design system is a product constraint and should be implemented rather than replaced by a generic component library.

## Open Source
RetroFrontier is an open-source project.

Intended project license: `GPL-3.0-or-later`.

ROMs, copyrighted BIOS files, credentials, secrets, and downloaded RetroArch/runtime binaries must never be committed to the repository.

## Git Workflow
After the initial repository bootstrap:
- `main` represents the stable development line.
- Do not work directly on `main`.
- Use focused branches.
- Prefer one focused task per branch.
- Use pull requests.
- Use squash merging.
- Do not mix unrelated cleanup into feature branches.

## Codex Model Strategy
Default model: **GPT Luna Max**.

Use Luna Max for normal planning, implementation, UI work, Rust development, tests, refactoring, and routine debugging.

Escalate to **GPT Sol Max** only when justified, especially for:
- difficult-to-reverse architecture decisions
- runtime updater security or signing
- integrity and rollback design
- migrations that may endanger user data
- difficult cross-platform failures
- difficult concurrency or race-condition failures
- problems Luna Max repeatedly fails to resolve
- release-readiness architecture reviews

Prefer using Sol as a focused senior reviewer when Luna can implement the work safely.

## Current Phase
The project is in planning and foundation setup.

Do not begin broad feature implementation until product, domain, architecture, ADRs, and the initial backlog are sufficiently defined.

The next technical investigations are:
1. Managed portable RetroArch/runtime distribution across all V1 platforms.
2. ScreenScraper authentication and credential strategy for an open-source desktop client.
