# RetroFrontier

RetroFrontier is an open-source desktop frontend, game library, and ROM management application for RetroArch.

The project aims for a zero-configuration experience: install RetroFrontier, add your own ROMs and required BIOS files, and play without manually configuring a separate RetroArch installation.

## Status
M1 application foundation, M2 managed-runtime foundations, M3 systems/cores/BIOS
foundations, M4 local library scanning, and M5 metadata enrichment are in place.
The Rust scanner owns content-root bootstrap, recursive discovery, CUE/BIN, GDI,
CHD, and M3U relationship resolution, hashing, durable reconciliation, and typed
scan IPC. Rust also owns metadata: a provider-neutral provider boundary with a
ScreenScraper adapter, evidence-validated matching, a restart-safe job queue with
dynamic quota handling, an offline-capable local cache, and one cached cover per
game. M6.2 adds the library shell, empty/setup state, scan UX, and root-management
entry points. M6.3 adds bounded local library browsing with debounced search,
system and favorite filters, page controls, authoritative favorite mutations,
cached covers, and offline-safe cover fallbacks. Game detail and launching remain
later milestones. M6.1 provides the bounded library-query, summary, local-detail,
favorite, scan-issue, and cached-cover IPC foundations consumed by the current UI.

## Stack
- Tauri 2
- Rust
- React
- TypeScript
- Vite
- SQLite + sqlx
- pnpm

## Development

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for prerequisites and the
short development/check workflow.

## Runtime Model
RetroArch is not bundled in the RetroFrontier installer.

RetroFrontier will download and manage its own isolated RetroArch runtime after installation. Existing local RetroArch installations/configurations should not influence it.

## V1 Platforms
- Windows x86_64
- macOS arm64
- macOS x86_64
- Linux x86_64

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

## User Content
RetroFrontier does not provide ROMs or copyrighted BIOS files.

Default managed folders:
```text
Documents/RetroFrontier/
├── ROMs/
└── BIOS/
```

External ROM directories can be added as read-only content roots.

The M4 scanner contract and IPC event names are documented in
[`docs/LIBRARY_SCANNER.md`](docs/LIBRARY_SCANNER.md), and the M5 metadata
architecture in [`docs/METADATA.md`](docs/METADATA.md).

The M6 implementation record is maintained in [`docs/M6_REPORT.md`](docs/M6_REPORT.md).

Cached provider covers live in the application data directory under
`metadata/media/`, never beside your ROM or BIOS files.

## Project Documentation
- [`PROJECT_CONTEXT.md`](PROJECT_CONTEXT.md)
- [`PRODUCT.md`](PRODUCT.md)
- [`DOMAIN.md`](DOMAIN.md)
- [`ARCHITECTURE.md`](ARCHITECTURE.md)
- [`BACKLOG.md`](BACKLOG.md)
- [`AGENTS.md`](AGENTS.md)
- [`docs/adr/`](docs/adr/)

## Contributing
See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Security
See [`SECURITY.md`](SECURITY.md).

## License
RetroFrontier is intended to be licensed under `GPL-3.0-or-later`.

Add the standard repository `LICENSE` file before public distribution.
