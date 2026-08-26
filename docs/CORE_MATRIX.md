# V1 Core Matrix

## Status

**M3 model complete; core decisions remain unresolved.** `SystemCatalog` carries an explicit
unresolved decision for every V1 system. No default or alternative is implied until the research
below has produced an approved managed core, source, license, and platform review.

Each resolved default must be checked against:

- license compatibility
- acceptable distribution source
- Windows x86_64
- macOS arm64
- macOS x86_64
- Linux x86_64
- libretro availability
- BIOS requirements
- content formats
- performance
- accuracy
- maintainability

“Filename candidates” below are discovery inputs only. They are not authoritative BIOS identities;
the M3 catalog intentionally has no BIOS hashes because this repository does not yet contain an
authoritative identity source for these dumps.

| Stable system ID | System | Default approved core | Approved alternatives | BIOS policy | Filename candidates / requirements | Identity or research status |
|---|---|---|---|---|---|---|
| `nes` | Nintendo Entertainment System | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `snes` | Super Nintendo Entertainment System | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `nintendo_64` | Nintendo 64 | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `game_boy` | Game Boy | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `game_boy_color` | Game Boy Color | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `game_boy_advance` | Game Boy Advance | Unresolved | None selected | Optional | `gba_bios.bin` | BIOS identity unresolved; core research remains |
| `mega_drive` | Sega Mega Drive / Genesis | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `playstation` | PlayStation | Unresolved | None selected | Required | `scph1001.bin`, `scph5500.bin`, `scph5501.bin`, `scph5502.bin` | BIOS identity unresolved; core research remains |
| `sega_saturn` | Sega Saturn | Unresolved | None selected | Required | `sega_101.bin`, `mpr-17933.bin` | BIOS identity unresolved; core research remains |
| `sega_dreamcast` | Sega Dreamcast | Unresolved | None selected | Required | `dc_boot.bin` and `dc_flash.bin` | BIOS identities unresolved; core research remains |
| `nintendo_gamecube` | Nintendo GameCube | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |

## M3 implementation notes

- The application-owned catalog uses stable IDs, display names, aliases, normalized extensions,
  BIOS requirements, and typed core policy. Static policy is not copied into SQLite.
- A `CoreDefinition` can later describe an approved managed core, its compatible systems,
  platform/architecture targets, and managed component ID. RuntimeManager remains the authority on
  whether that component is authenticated, installed, and verified.
- A BIOS file with a candidate filename but no catalog identity is reported as
  `notCoveredByCatalog`, not valid. A wrong hash or known size is reported as invalid once an
  authoritative requirement is added.

## Policy

The product should choose defaults so new users do not need to understand core selection.

Open-source licensing alone does not guarantee a core is appropriate for automated
distribution/installation. Record exact licenses and sources before resolving a row.

Alternative cores are not a V1 requirement. Per-game overrides may exist later, but only from
installed approved managed cores.

The runtime spikes provide runtime/core-loading evidence, but this matrix is finalized only after
each default core passes the platform, license, content-format, BIOS, and maintainability checks.
