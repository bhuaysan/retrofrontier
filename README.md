# RetroFrontier

RetroFrontier is an open-source desktop frontend, game library, and ROM management application for RetroArch.

The project aims for a zero-configuration experience: install RetroFrontier, add your own ROMs and required BIOS files, and play without manually configuring a separate RetroArch installation.

## Status
New greenfield project in planning and foundation setup.

## Planned Stack
- Tauri 2
- Rust
- React
- TypeScript
- Vite
- SQLite + sqlx
- pnpm

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

External ROM directories will also be supported.

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
