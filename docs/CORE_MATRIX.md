# V1 Core Matrix

## Status

**M7 resolves four reference systems; the remaining seven stay unresolved.** `SystemCatalog`
carries an approved default core for NES, SNES, PlayStation, and Nintendo GameCube, and an explicit
unresolved decision for every other V1 system. No default or alternative is implied for an
unresolved row until the research below has produced an approved managed core, source, license, and
platform review.

Resolved policy is *not* the same as an available core, but as of M7.5 all four resolved rows are
**built, installable, and verified**: the active `rf-runtime-1.22.2-linux-x86_64-002` ships every one
of them as an authenticated component, and readiness reports the approved default core as available
once RuntimeManager has verified that installation. See
[`docs/M7_5_RUNTIME_QUALIFICATION.md`](M7_5_RUNTIME_QUALIFICATION.md) for the original Release 001
artefacts and licences, and
[`docs/M8_FINAL_HARDWARE_INPUT_REPORT.md`](M8_FINAL_HARDWARE_INPUT_REPORT.md) for the Release 002
digests and the immutable core source they now come from.

RetroFrontier still has no *public production* Runtime Release source, TUF root, or hosting
decision (ADR-012); those remain M10 gates. A build without a configured release source reports the
runtime as uninstallable rather than pretending.

Approved core identities, libretro core names, licences, and upstream sources below were verified
against the libretro core documentation (`github.com/libretro/docs`, `docs.libretro.com`) while
preparing `docs/superpowers/specs/2026-08-30-m7-retroarch-launch-design.md`.

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
| `nes` | Nintendo Entertainment System | `nestopia` (Nestopia UE) | None selected | Not required | None | **Resolved (M7).** `nestopia_libretro`, GPL-2.0, https://github.com/libretro/nestopia (Nestopia JG upstream). `disksys.rom` is FDS-only and `.fds` is not a V1 extension. |
| `snes` | Super Nintendo Entertainment System | `bsnes-mercury-balanced` (bsnes-mercury Balanced) | None selected | Not required | None | **Resolved (M7).** `bsnes_mercury_balanced_libretro`, GPL-3.0, https://github.com/libretro/bsnes-mercury, Balanced profile. Explicitly selected as the qualified Balanced-profile artifact for M7; upstream treats `bsnes` and `bsnes-mercury` as separate core families and no equivalence is claimed. Coprocessor firmware is title-specific; see the note below. |
| `nintendo_64` | Nintendo 64 | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `game_boy` | Game Boy | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `game_boy_color` | Game Boy Color | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `game_boy_advance` | Game Boy Advance | Unresolved | None selected | Optional | `gba_bios.bin` | BIOS identity unresolved; core research remains |
| `mega_drive` | Sega Mega Drive / Genesis | Unresolved | None selected | Not required | None | Core research: license/source/platform/content review |
| `playstation` | PlayStation | `beetle-psx` (Beetle PSX) | None selected | Required | `scph5500.bin`, `scph5501.bin`, `scph5502.bin` | **Resolved (M7).** `mednafen_psx_libretro`, GPL-2.0, https://github.com/libretro/beetle-psx-libretro. BIOS identities are authoritative; see below. |
| `sega_saturn` | Sega Saturn | Unresolved | None selected | Required | `sega_101.bin`, `mpr-17933.bin` | BIOS identity unresolved; core research remains |
| `sega_dreamcast` | Sega Dreamcast | Unresolved | None selected | Required | `dc_boot.bin` and `dc_flash.bin` | BIOS identities unresolved; core research remains |
| `nintendo_gamecube` | Nintendo GameCube | `dolphin` (Dolphin) | None selected | Not required | None | **Resolved (M7).** `dolphin_libretro`, GPL-2.0, https://github.com/libretro/dolphin. Requires the managed Dolphin `Sys` support component; the optional GameCube IPL is deferred. |

## M7.5 managed component identities

Every resolved core is a separately identified TUF target in the Linux x86_64 release. Exact
upstream sources, artefact digests, sizes, and licences are recorded in
[`release/linux-x86_64/runtime-release.json`](../release/linux-x86_64/runtime-release.json).

As of Release 002 all four cores are derived from the single version-addressed
`buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch_cores.7z` bundle rather than from four
rolling nightly URLs, so the release stays reconstructable. No core was substituted and every
upstream project listed above is unchanged, but the **binaries did change**: the stable 1.22.2 build
of a core is not byte-identical to the nightly build Release 001 shipped. The old and new digests are
compared in [`docs/M8_FINAL_HARDWARE_INPUT_REPORT.md`](M8_FINAL_HARDWARE_INPUT_REPORT.md).

| Managed component | Installed at | Executable |
|---|---|---|
| `nestopia` | `cores/nestopia` | `nestopia_libretro.so` |
| `bsnes-mercury-balanced` | `cores/bsnes-mercury-balanced` | `bsnes_mercury_balanced_libretro.so` |
| `beetle-psx` | `cores/beetle-psx` | `mednafen_psx_libretro.so` |
| `dolphin` | `cores/dolphin` | `dolphin_libretro.so` |
| `dolphin-sys` | `runtime/support/dolphin-sys` | support data only |

The managed `dolphin-sys` component comes from libretro's own system-assets buildbot, never from a
user's Dolphin installation, and is linked into the composed system directory as
`dolphin-emu/Sys`.

## Resolved PlayStation BIOS identities

The approved core documents exactly which BIOS dumps it loads, and by which filename. RetroFrontier
records those identities per file, so a genuine dump under the wrong filename is reported invalid
rather than valid-but-unloadable.

| Filename | Description | MD5 |
|---|---|---|
| `scph5500.bin` | PS1 JP BIOS | `8dd7d5296a650fac7319bce665a6a53c` |
| `scph5501.bin` | PS1 US BIOS | `490f666e1afb15b7362b406ed1cea246` |
| `scph5502.bin` | PS1 EU BIOS | `32736f17079d0b2b7024407c39bd3050` |

Source: libretro Beetle PSX core documentation. Consequences:

- `scph1001.bin` was removed from the candidate list because the approved core does not look that
  filename up.
- The published identities are MD5, so `BiosHashAlgorithm` supports MD5 in addition to SHA-256
  rather than recording unverifiable SHA-256 values. Discovery still reports the observed SHA-256.
- No expected size is asserted; the digest already pins identity exactly.
- Region enforcement is deferred: at least one of the three documented dumps satisfies the
  requirement.
- Beetle PSX can fall back to a bundled OpenBIOS. RetroFrontier keeps PlayStation BIOS **required**,
  validates before spawn, and never enables the core's BIOS override.

## Deferred: SNES coprocessor firmware

bsnes-mercury documents optional coprocessor firmware (`dsp1*`, `dsp2*`, `dsp3*`, `dsp4*`,
`cx4.data.rom`, `st010*`, `st011*`, `st018*`, `sgb.boot.rom`) needed only by a small number of
enhancement-chip titles, and it ships HLE options for many of them. Marking every SNES title as
BIOS-required would be false, so SNES stays `Not required`. Per-title firmware detection needs
cartridge-level identification RetroFrontier does not have yet and is explicitly deferred.

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
