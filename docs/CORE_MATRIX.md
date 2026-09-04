# V1 Core Matrix

Authoritative core, platform, format and policy source for the eleven V1 systems. Closed by M10.2.

Companion documents:

- [`docs/BIOS_MATRIX.md`](BIOS_MATRIX.md) — required/optional firmware policy and authoritative identities.
- [`docs/SOURCE_PROVENANCE.md`](SOURCE_PROVENANCE.md) — licence, redistribution and corresponding-source closure.

## How to read this document

M10.2 exists because the previous matrix collapsed several different questions into one
"resolved/unresolved" column. They are now separate, and they do not imply one another.

| Concept | Column | Meaning |
|---|---|---|
| Policy status | Policy | Whether RetroFrontier has *decided* this core is the controlled default. |
| Redistribution | Redistribution | Whether RetroFrontier may lawfully ship the binary in a managed Runtime Release. |
| Provenance | Source revision | Whether the exact corresponding source of the redistributed binary is known. |
| Availability | Platform columns | Whether a binary exists, and whether it can be acquired from an *immutable* input. |
| Implementation | Implementation | Whether the application actually has this core in its catalog and Runtime Release. |
| Qualification | Qualification | Whether a real managed launch was measured on real hardware. |

Status vocabulary, used strictly:

- **Approved** — decided as the controlled default under ADR-009.
- **Candidate** — evidence gathered and a leading core identified, but the decision rule is not met.
- **Unresolved** — no core meets the rule; the system approves no core at all (DOMAIN rule 15).
- **Implemented** — present in `SystemCatalog` and in an authenticated Runtime Release.
- **Qualified** — a managed launch was actually measured.
- **Research-only** — evidence exists in this document and nowhere else in the product.
- **Blocked** — a specific, named obstacle prevents progress.
- **Missing** — no artefact exists.

**Research is not qualification, and a published binary is not redistribution approval.** No row below
may be read as a support claim.

## Decision rule (ADR-009, tightened by M10.2)

A system may be marked **Approved** only when *all* hold: technically suitable; licence identified
from the upstream licence file; redistribution path understood; corresponding-source path
understood; binary/source provenance pinnable; a credible binary/build path for the required V1
platforms; BIOS policy known; content formats compatible with the library model; no unresolved
architectural conflict.

If evidence is insufficient the system stays **Unresolved**. A visible V1 blocker is preferred over a
false green matrix.

## 1. Policy and core identity

| System | Core | Policy | Core upstream | Licence (verified) | Exact source revision | Implementation | Qualification |
|---|---|---|---|---|---|---|---|
| NES | Nestopia UE (`nestopia_libretro`) | **Approved** (M7) | https://github.com/libretro/nestopia | `GPL-2.0-or-later` | **Unknown — unrecoverable** | Implemented (Release 002) | Qualified, Linux x86_64 only |
| SNES | bsnes-mercury Balanced (`bsnes_mercury_balanced_libretro`) | **Approved** (M7) | https://github.com/libretro/bsnes-mercury | `GPL-3.0-only` | **Unknown — unrecoverable** | Implemented (Release 002) | Qualified, Linux x86_64 only |
| Nintendo 64 | Mupen64Plus-Next (`mupen64plus_next_libretro`) | **Candidate** | https://github.com/libretro/mupen64plus-libretro-nx | **Conflicting** — see §5.1 | Unknown | Not implemented | Research-only |
| Game Boy | mGBA (`mgba_libretro`) | **Candidate** | https://github.com/mgba-emu/mgba (libretro fork: libretro/mgba) | `MPL-2.0` | Unknown | Not implemented | Research-only |
| Game Boy Color | mGBA (`mgba_libretro`) | **Candidate** | https://github.com/mgba-emu/mgba | `MPL-2.0` | Unknown | Not implemented | Research-only |
| Game Boy Advance | mGBA (`mgba_libretro`) | **Candidate** | https://github.com/mgba-emu/mgba | `MPL-2.0` | Unknown | Not implemented | Research-only |
| Mega Drive / Genesis | *none* | **Unresolved — blocked** | — | see §5.2 | — | Not implemented | Research-only |
| PlayStation | Beetle PSX (`mednafen_psx_libretro`) | **Approved** (M7) | https://github.com/libretro/beetle-psx-libretro | `GPL-2.0` (aggregate; see §5.3) | **Unknown — unrecoverable** | Implemented (Release 002) | Qualified, Linux x86_64 only |
| Saturn | Beetle Saturn (`mednafen_saturn_libretro`) | **Candidate** | https://github.com/libretro/beetle-saturn-libretro | `GPL-2.0-or-later` | Unknown | Not implemented | Research-only |
| Dreamcast | Flycast (`flycast_libretro`) | **Candidate** | https://github.com/flyinghead/flycast | `GPL-2.0-or-later` | Unknown | Not implemented | Research-only |
| GameCube | Dolphin (`dolphin_libretro`) | **Approved** (M7) | https://github.com/libretro/dolphin (upstream dolphin-emu/dolphin) | `GPL-2.0-or-later` | **Unknown — unrecoverable** | Implemented (Release 002) | Qualified, Linux x86_64 only |

The four Approved rows are **unchanged by M10.2**. M10.2 did not replace them and found no evidence
requiring their replacement. Their *policy* approval survives; their *redistribution* status is
newly and separately blocked (§3, §5.3).

No system was moved to Approved by M10.2. See §6 for why.

## 2. Platform availability

Availability is recorded per platform. It is never generalised from one platform to the others.

Two different things are recorded: whether a binary is **published** at all, and whether it can be
acquired from an **immutable, version-addressed** input as ADR-004 and ADR-012 require. A rolling
`…/latest/` nightly URL is *not* an acceptable release input.

| Platform | Immutable core acquisition path | Evidence |
|---|---|---|
| **Linux x86_64** | **Available.** `buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch_cores.7z`, version-addressed, pinned `sha256:4b7ed8dc97d4bf035fce182c64b5658c7662e2e9e5d42129538afbd4b6096307`. | Bundle verified locally against the Release 002 pin; 199 core members enumerated. |
| **Windows x86_64** | **Exists, membership unverified.** `…/stable/1.22.2/windows/x86_64/RetroArch_cores.7z` (229,761 KB) is published and version-addressed. Per-core membership has **not** been verified. | Official buildbot index for 1.22.2. |
| **macOS arm64** | **MISSING — no immutable path exists.** | See below. |
| **macOS x86_64** | **MISSING — no immutable path exists.** | See below. |

### 2.1 macOS is a platform-level blocker, not a core-level one

`buildbot.libretro.com/stable/1.22.2/apple/osx/` publishes **only** `universal/RetroArch_Metal.dmg`
and `x86_64/RetroArch.dmg`. There is **no `RetroArch_cores.7z` for macOS at any architecture**.
libretro publishes macOS cores only as per-core `*.dylib.zip` under the rolling paths
`nightly/apple/osx/arm64/latest/` and `nightly/apple/osx/x86_64/latest/`.

Consequently the version-addressed bundle strategy Release 002 depends on **does not exist for
macOS**, and the only published macOS core source is precisely the moving-nightly form ADR-004 and
this milestone reject.

This affects **every core equally, including the four already Approved**. It is a packaging and
acquisition-strategy blocker for M10, not a discriminator between candidate cores, and it cannot be
closed by choosing a different core.

### 2.2 Per-core published-binary availability

"Published" only. It carries no redistribution or immutability claim. Cell meanings:

- **In pinned bundle** — the member was enumerated inside the digest-verified Release 002 bundle.
  This is the only cell type that represents an immutable acquisition path.
- **Nightly only** — a per-core archive exists under a rolling `…/latest/` path. Not acceptable as a
  release input (§2.1).
- **Bundle unverified** — a version-addressed bundle exists for the platform, but this core's
  membership in it was not checked.

| Core | Linux x86_64 | Windows x86_64 | macOS arm64 | macOS x86_64 |
|---|---|---|---|---|
| `nestopia_libretro` | **In pinned bundle** | Bundle unverified | Nightly only | Nightly only |
| `bsnes_mercury_balanced_libretro` | **In pinned bundle** | Bundle unverified | Nightly only | Nightly only |
| `mednafen_psx_libretro` | **In pinned bundle** | Bundle unverified | Nightly only | Nightly only |
| `dolphin_libretro` | **In pinned bundle** | Bundle unverified | Nightly only | Nightly only |
| `mupen64plus_next_libretro` | **In pinned bundle** | Bundle unverified; nightly `.dll.zip` confirmed | Nightly `.dylib.zip` confirmed | Nightly only, not enumerated |
| `mgba_libretro` | **In pinned bundle** | Bundle unverified; nightly `.dll.zip` confirmed | Nightly `.dylib.zip` confirmed | Nightly only, not enumerated |
| `mednafen_saturn_libretro` | **In pinned bundle** | Bundle unverified; nightly `.dll.zip` confirmed | Nightly `.dylib.zip` confirmed | Nightly only, not enumerated |
| `flycast_libretro` | **In pinned bundle** | Bundle unverified; nightly `.dll.zip` confirmed | Nightly `.dylib.zip` confirmed | Nightly only, not enumerated |
| `blastem_libretro` | **In pinned bundle** | Bundle unverified; nightly `.dll.zip` confirmed | Nightly `.dylib.zip` confirmed | Nightly only, not enumerated |
| `genesis_plus_gx_libretro` | In pinned bundle, but **redistribution prohibited** (§5.2) | not assessed | not assessed | not assessed |
| `picodrive_libretro` | In pinned bundle, but **redistribution prohibited** (§5.2) | not assessed | not assessed | not assessed |

Two limits of this table, stated rather than hidden:

- Windows per-core membership of `stable/1.22.2/windows/x86_64/RetroArch_cores.7z` was **not**
  verified; that would require downloading a 229 MB archive. The nightly `.dll.zip` presence proves
  each core *builds* for Windows, which is a weaker claim than bundle membership.
- macOS **x86_64** was not enumerated per core; only the arm64 nightly index was inspected. Since
  §2.1 already establishes that no immutable macOS path exists at any architecture, per-core
  enumeration would not change any conclusion.

The four approved cores were not re-checked against the nightly Windows/macOS indexes, because
their Linux bundle membership is what Release 002 actually depends on.

## 3. Redistribution and blockers

| System | Redistribution status | Remaining blocker |
|---|---|---|
| NES | **Blocked** | Corresponding source for the redistributed binary is unknown (§5.3). |
| SNES | **Blocked** | Corresponding source unknown (§5.3). |
| Nintendo 64 | Blocked | Licence identity conflict (§5.1) + corresponding source + not implemented. |
| Game Boy | Blocked | Corresponding source strategy undecided; not implemented; not qualified. |
| Game Boy Color | Blocked | As Game Boy. |
| Game Boy Advance | Blocked | As Game Boy. |
| Mega Drive / Genesis | **Prohibited** | Both mainstream cores forbid commercial redistribution (§5.2). No approved alternative is qualified. |
| PlayStation | **Blocked** | Corresponding source unknown (§5.3). |
| Saturn | Blocked | Corresponding source; content-format mismatch (§4.1); not implemented. |
| Dreamcast | Blocked | Corresponding source; BIOS policy currently contradicts the core documentation (§4.2); not implemented. |
| GameCube | **Blocked** | Corresponding source unknown (§5.3). |

Every V1 system is currently blocked from public redistribution. For the four Approved systems the
blocker is provenance, not policy.

## 4. Content formats

Catalog extensions are RetroFrontier's library model. Core extensions are what the core accepts.
A mismatch is a real defect, not a formality.

| System | Catalog extensions | Candidate/approved core accepts | Compatible? |
|---|---|---|---|
| NES | `.nes` | `nes\|fds\|unf\|unif` | Yes |
| SNES | `.sfc`, `.smc` | `sfc\|smc\|bml\|gb\|gbc\|st\|bs` | Yes |
| Nintendo 64 | `.n64`, `.z64`, `.v64` | `n64\|v64\|z64\|ndd\|bin\|u1` | Yes |
| Game Boy | `.gb` | `gb\|gbc\|gba` (mGBA) | Yes |
| Game Boy Color | `.gbc` | `gb\|gbc\|gba` (mGBA) | Yes |
| Game Boy Advance | `.gba` | `gb\|gbc\|gba` (mGBA) | Yes |
| Mega Drive | `.md`, `.gen`, `.smd`, `.bin` | BlastEm: `md\|gen\|smd\|…\|bin\|…` | Yes (but core is not approved, §5.2) |
| PlayStation | `.cue`, `.chd`, `.pbp`, `.bin`, `.iso`, `.m3u` | `cue\|toc\|m3u\|ccd\|exe\|pbp\|chd\|bin` | **No — see §4.3** |
| Saturn | `.cue`, `.chd`, `.iso`, `.bin`, `.m3u` | `ccd\|chd\|cue\|toc\|m3u\|zip` | **No — see §4.1** |
| Dreamcast | `.gdi`, `.cdi`, `.chd`, `.m3u` | `chd\|cdi\|elf\|bin\|cue\|gdi\|lst\|zip\|dat\|7z\|m3u` | Yes |
| GameCube | `.iso`, `.gcm`, `.rvz` | `gcm\|iso\|wbfs\|ciso\|gcz\|elf\|dol\|dff\|tgc\|wad\|rvz\|m3u\|wia` | Yes |

Core extensions above are taken from `libretro/libretro-core-info` `.info` metadata, which is the
machine-readable form RetroArch itself consumes.

### 4.1 Saturn format mismatch (open defect, not yet corrected)

The catalog offers Saturn `.iso` and `.bin`. Beetle Saturn's extensions are `ccd|chd|cue|toc|m3u|zip`
— **neither `.iso` nor `.bin` is accepted as a launch target**. If Saturn is ever approved with this
core, the catalog must drop `.iso` and consider `.toc`/`.ccd`. `.bin` may remain only as a member
track, never as a launch target, which the M7 launch contract already requires.

This is *not* corrected in code by M10.2, because Saturn approves no core and is therefore not
launchable (DOMAIN rule 15). It is a precondition of any future Saturn approval.

### 4.3 PlayStation `.iso` is advertised but not loadable (live defect)

Unlike §4.1, this affects a system that is **Approved, Implemented and Qualified today**.

The catalog offers PlayStation `.iso`. Beetle PSX's `.info` extensions are
`cue|toc|m3u|ccd|exe|pbp|chd|bin` — **`.iso` is not among them**. A user who places a PlayStation
`.iso` in the managed library therefore gets a scanned, displayed, apparently-launchable game whose
launch the core will refuse.

Severity is limited: this is a launch-time failure on one container format, not a trust-boundary or
data-safety issue, and `.cue`/`.chd`/`.pbp`/`.m3u` — the formats the library model actually
recommends — are unaffected. It is nonetheless a real mismatch between advertised and achievable
support.

M10.2 does **not** change the catalog, because narrowing a shipped system's accepted extensions
changes scanner behaviour and library reconciliation for existing users and is outside a
documentation milestone's scope. It is recorded here as a defect requiring a product owner decision
(§7 / the completion report): either drop `.iso` from the PlayStation catalog entry, or accept it
and surface a clearer launch-time error. Beetle Saturn shows the same pattern, so the two should be
decided together.

### 4.2 Dreamcast BIOS layout conflict

Flycast documents that its firmware lives in a **`dc/` subdirectory** of RetroArch's system
directory, not at the top level. RetroFrontier's BIOS discovery has no notion of a core-required
internal layout. This is the already-open backlog item *"map user BIOS folders to any future
core-required internal layout"* and it is a hard precondition of Dreamcast approval. See
[`docs/BIOS_MATRIX.md`](BIOS_MATRIX.md) §Dreamcast.

## 5. Licence findings

### 5.1 Nintendo 64 — conflicting licence statements

Authoritative sources disagree:

- `libretro/mupen64plus-libretro-nx/LICENSE` contains the **GPL version 2** text.
- `mupen64plus-core/LICENSES` states "licensed under the GNU General Public License version 2".
- `GLideN64/LICENSE` states "GLideN64 is licensed under the GNU General Public License version 2".
- `mupen64plus-core/src/main/main.c` headers state "version 2 … or (at your option) any later version".
- **`docs.libretro.com/library/mupen64plus/` states the licence is "GPLv3".**

A repository-level `LICENSE` of GPLv2, a component claiming GPLv2, per-file headers saying
GPLv2-or-later, and official documentation saying GPLv3 cannot all be correct. The effective
licence of a redistributed `mupen64plus_next_libretro` binary is therefore **not established**, and
N64 cannot be approved on this evidence. Closing it requires a per-file licence audit of the core
and its bundled video plugins. **Legal review required.**

### 5.2 Mega Drive / Genesis — both mainstream cores are non-free

This is M10.2's most consequential finding.

`libretro/Genesis-Plus-GX/LICENSE.txt` and `libretro/picodrive/COPYING` both contain, verbatim:

> Redistributions may not be sold, nor may they be used in a commercial product or activity.

Consequences:

- Neither licence is an SPDX standard licence (GitHub reports `NOASSERTION` for both).
- Both are **non-free**: a field-of-use restriction on commercial activity.
- Both are **GPL-incompatible**, so neither can be combined with or distributed as part of a
  GPL-3.0-or-later work.
- RetroFrontier is `GPL-3.0-or-later` (ADR-010). It **must not** redistribute either core in a
  managed Runtime Release, and the restriction survives any future commercial use of RetroFrontier.

Genesis Plus GX additionally vendors an LGPL-2.1-or-later Nuked OPN2 core, which does not cure the
outer non-commercial term.

The remaining GPL-compatible candidate is **BlastEm** (`libretro/blastem`, GPLv3-or-later per
`blastem.c` and `COPYING`). It is not approved because:

- its true upstream is `https://www.retrodev.com/blastem/` (Mercurial), not GitHub — the
  `libretro/blastem` repository is an *upstream tracking repo with libretro-specific changes*, so
  corresponding source spans two hosts, one of which is not immutably archivable today;
- its libretro maturity relative to the two non-free cores has not been measured, and no managed
  launch has been attempted.

BlastEm's content formats *are* compatible: its `.info` extensions include `md`, `gen`, `smd` and
`bin`, covering the catalog's Mega Drive entry. Format is not the obstacle here — provenance and
unmeasured maturity are.

**Mega Drive is therefore an Unresolved V1 blocker.** It must not receive a fallback core. Product
owner decision required (§7).

### 5.3 The four redistributed cores have no recoverable corresponding source

Release 002 records `"source_revision": null` for `nestopia`, `bsnes-mercury-balanced`,
`beetle-psx` and `dolphin`. M10.2 established that this is **not an oversight that can be filled in
by looking harder** — the data does not exist upstream. Full analysis in
[`docs/SOURCE_PROVENANCE.md`](SOURCE_PROVENANCE.md).

Beetle PSX is additionally recorded as `GPL-2.0-only` in Release 002 while
`mednafen/psx/gpu.c` carries a "version 2 … or any later version" header. The declared identifier is
the conservative reading and is safe to distribute under, but the aggregate `only`/`or-later`
disposition has not been established by a per-file audit. Recorded as `GPL-2.0 (aggregate)` above.

## 6. Why no system was approved by M10.2

Every remaining system fails the decision rule on the *same* clause: the corresponding-source path
for a redistributed libretro binary is not merely undecided but currently unavailable from the
approved acquisition strategy (§5.3). Approving four more cores would multiply an unresolved
redistribution obligation across four more systems while closing nothing.

Two systems fail on further, independent grounds: Nintendo 64 on licence identity (§5.1), and Mega
Drive on licence compatibility with no qualified alternative (§5.2).

The productive M10.2 outcome is therefore that seven systems moved from *"unresearched"* to
*Candidate with recorded primary-source evidence and a named blocker*, and that the blocker itself
is now precisely identified and shared.

## 7. Runtime impact — what a successor Runtime Release may contain

**No Runtime Release is created by M10.2. Release 002 is unchanged.**

Which systems could be added to a successor Linux x86_64 release *purely on technical availability*:

| System | Technically addable to a successor release? | Gate |
|---|---|---|
| Nintendo 64 | Yes, binary is in the pinned bundle | **Blocked** — licence identity unresolved |
| Game Boy / Color / Advance | Yes, one `mgba_libretro` component covers all three | **Blocked** — policy not approved; MPL-2.0 notice obligations undefined |
| Saturn | Yes | **Blocked** — policy not approved; catalog format mismatch |
| Dreamcast | Yes | **Blocked** — policy not approved; `dc/` BIOS layout unsupported |
| Mega Drive | No acceptable core | **Blocked** — licence |

For an **internal, non-public, non-distributed qualification release**, the lowest-risk expansion is
`mgba_libretro`, because MPL-2.0 imposes the lightest source obligation and one component resolves
three systems. That is a *qualification* step, not an approval, and it still requires a product
owner decision.

**Nothing may be added to a publicly distributed release until §5.3 is closed.**

## 8. Preserved M7 material

### 8.1 Managed component identities (Release 002, unchanged)

| Managed component | Installed at | Executable |
|---|---|---|
| `nestopia` | `cores/nestopia` | `nestopia_libretro.so` |
| `bsnes-mercury-balanced` | `cores/bsnes-mercury-balanced` | `bsnes_mercury_balanced_libretro.so` |
| `beetle-psx` | `cores/beetle-psx` | `mednafen_psx_libretro.so` |
| `dolphin` | `cores/dolphin` | `dolphin_libretro.so` |
| `dolphin-sys` | `runtime/support/dolphin-sys` | support data only |

The managed `dolphin-sys` component comes from libretro's own system-assets buildbot, never from a
user's Dolphin installation, and is linked into the composed system directory as `dolphin-emu/Sys`.

### 8.2 Catalog licence strings are imprecise (open, uncorrected)

`SystemCatalog::v1_cores()` records `"GPL-2.0"` / `"GPL-3.0"` for the four approved cores. The
verified identifiers are `GPL-2.0-or-later`, `GPL-3.0-only`, `GPL-2.0` (aggregate) and
`GPL-2.0-or-later` respectively, and Release 002's manifest already carries the precise forms.

M10.2 deliberately does **not** change this code: the redistribution authority is the authenticated
release manifest, not the catalog string, and M10.2 is a documentation/policy milestone. Correcting
the four catalog strings is a required pre-release follow-up.

## 9. Policy

The product should choose defaults so new users do not need to understand core selection.

Open-source licensing alone does not guarantee a core is appropriate for automated
distribution/installation. Record exact licences and sources before resolving a row.

A core filename existing on the libretro buildbot is not enough to approve redistribution. A GitHub
repository existing is not enough to satisfy GPL corresponding-source obligations. A moving nightly
URL is not an acceptable immutable release source.

Alternative cores are not a V1 requirement. Per-game overrides may exist later, but only from
installed approved managed cores.

## 10. Evidence

All findings were taken from primary sources on 2026-09-04. Community lists were not used as
evidence for licensing or redistribution.

- Upstream licence files read verbatim via the GitHub contents API: `libretro/Genesis-Plus-GX`
  `LICENSE.txt`, `libretro/picodrive` `COPYING`, `libretro/bsnes-mercury` `LICENSE`,
  `libretro/mupen64plus-libretro-nx` `LICENSE` + `mupen64plus-core/LICENSES` + `GLideN64/LICENSE`,
  `libretro/beetle-saturn-libretro` `COPYING`, `flyinghead/flycast` `LICENSE`, `libretro/mgba`
  `LICENSE`, `libretro/beetle-psx-libretro` `COPYING`, `libretro/blastem` `COPYING`.
- Per-file licence headers read for exact `only`/`or-later` disposition:
  `nestopia/source/core/NstBase.hpp`, `dolphin Source/Core/Core/Core.cpp` (SPDX
  `GPL-2.0-or-later`), `gambatte libgambatte/src/gambatte.cpp` (v2 only),
  `mupen64plus-core/src/main/main.c`, `flycast core/emulator.cpp`,
  `beetle-saturn mednafen/ss/ss.h`, `beetle-psx mednafen/psx/gpu.c`, `blastem blastem.c`.
- Official libretro core documentation: `docs.libretro.com/library/` for `beetle_saturn`,
  `flycast`, `mgba`, `gambatte`, `mupen64plus`, `blastem`.
- Official libretro buildbot indexes: `stable/1.22.2/`, `stable/1.22.2/windows/x86_64/`,
  `stable/1.22.2/apple/`, `stable/1.22.2/apple/osx/`, `.../osx/universal/`, `.../osx/x86_64/`,
  `nightly/apple/osx/`, `nightly/apple/osx/arm64/latest/`, `nightly/windows/x86_64/latest/`.
- The pinned Release 002 core bundle itself, verified locally as
  `sha256:4b7ed8dc97d4bf035fce182c64b5658c7662e2e9e5d42129538afbd4b6096307` and enumerated
  (199 core members).
- The installed Release 002 runtime and its authenticated manifest.
- `libretro/libretro-core-info` `.info` metadata.
