# V1 Core Matrix

Authoritative core, platform, format and policy source for the eleven V1 systems. Closed by M10.2.

Companion documents:

- [`docs/BIOS_MATRIX.md`](BIOS_MATRIX.md) — adopted firmware policy for Approved cores, kept separate
  from candidate-core firmware evidence.
- [`docs/SOURCE_PROVENANCE.md`](SOURCE_PROVENANCE.md) — licence, redistribution and corresponding-source closure.

## How to read this document

M10.2 exists because the previous matrix collapsed several different questions into one
"resolved/unresolved" column. They are now separate, and they do not imply one another.

| Concept | Column | Meaning |
|---|---|---|
| Policy status | Policy | Whether RetroFrontier has *decided* this core is the controlled default. |
| Redistribution | Redistribution | Whether RetroFrontier is willing to ship the binary in a managed Runtime Release under current policy. An engineering status, never a legal conclusion. |
| Provenance | Source revision | Whether the exact corresponding source of the redistributed binary is known. |
| Availability | Platform columns | Whether a binary exists, and whether it can be acquired from an *immutable* input. |
| Implementation | Implementation | Whether the application actually has this core in its catalog and Runtime Release. |
| Qualification | Qualification | Whether a real managed launch was measured on real hardware. |

Status vocabulary, used strictly:

- **Approved** — decided as the controlled default under ADR-009.
- **Candidate** — evidence gathered and a leading core identified, but the decision rule is not met.
- **Unresolved** — no core meets the rule; the system approves no core at all (DOMAIN rule 15).
- **Implemented** — present in `SystemCatalog` and in an authenticated Runtime Release.
- **Qualified end-to-end** — a real managed launch ran the system's content and rendering was
  observed.
- **Partially qualified** — some of the managed launch path was measured, with a named part that was
  **not** confirmed.
- **Not qualified** — no successful managed launch of this system's content has been measured.
- **Research-only** — evidence exists in this document and nowhere else in the product.
- **Blocked** — a specific, named obstacle prevents progress.
- **Missing** — no artefact exists.

**Research is not qualification, and a published binary is not redistribution approval.** No row below
may be read as a support claim.

### Qualification is inherited from M7.5 and was not advanced by M10.2

M10.2 measured nothing. It ran no managed launch, so it **cannot** raise any system's qualification
status, and the qualification column below simply restates
[`docs/M7_5_RUNTIME_QUALIFICATION.md`](M7_5_RUNTIME_QUALIFICATION.md) §Qualification status:

| System | M7.5 result | Open gate |
|---|---|---|
| NES | Qualified end-to-end — real launch, *Over Horizon (Europe)* | none on Linux x86_64 |
| SNES | Qualified end-to-end — *Super Mario World* rendering confirmed | none on Linux x86_64 |
| GameCube | **Partially qualified** — core resolved and managed `dolphin-emu/Sys` link verified; **no rendered frame observed** | `confirmed GameCube content execution` |
| PlayStation | **Not qualified** — core installs and resolves, and readiness correctly reports `MissingRequiredBios` | `PlayStation qualification (needs an approved BIOS dump and legal content)` |

Both open gates remain open in `BACKLOG.md`. GameCube content execution and PlayStation content
execution are explicitly **not claimed** by M7.5, and are explicitly **not claimed** here.

An Approved policy plus a shipped Release 002 component is *not* qualification: PlayStation is
Approved, Implemented and **not qualified** at the same time, and that combination is correct rather
than contradictory.

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
| NES | Nestopia UE (`nestopia_libretro`) | **Approved** (M7) | https://github.com/libretro/nestopia | `GPL-2.0-or-later` | **Unknown** (§5.3) | Implemented (Release 002) | **Qualified end-to-end**, Linux x86_64 only |
| SNES | bsnes-mercury Balanced (`bsnes_mercury_balanced_libretro`) | **Approved** (M7) | https://github.com/libretro/bsnes-mercury | `GPL-3.0-only` | **Unknown** (§5.3) | Implemented (Release 002) | **Qualified end-to-end**, Linux x86_64 only |
| Nintendo 64 | Mupen64Plus-Next (`mupen64plus_next_libretro`) | **Candidate** | https://github.com/libretro/mupen64plus-libretro-nx | **Conflicting** — see §5.1 | Unknown | Not implemented | Research-only |
| Game Boy | mGBA (`mgba_libretro`) | **Candidate** | https://github.com/mgba-emu/mgba (libretro fork: libretro/mgba) | `MPL-2.0` | Unknown | Not implemented | Research-only |
| Game Boy Color | mGBA (`mgba_libretro`) | **Candidate** | https://github.com/mgba-emu/mgba | `MPL-2.0` | Unknown | Not implemented | Research-only |
| Game Boy Advance | mGBA (`mgba_libretro`) | **Candidate** | https://github.com/mgba-emu/mgba | `MPL-2.0` | Unknown | Not implemented | Research-only |
| Mega Drive / Genesis | *none* | **Unresolved — blocked** | — | see §5.2 | — | Not implemented | Research-only |
| PlayStation | Beetle PSX (`mednafen_psx_libretro`) | **Approved** (M7) | https://github.com/libretro/beetle-psx-libretro | `GPL-2.0` (aggregate; see §5.3) | **Unknown** (§5.3) | Implemented (Release 002) | **Not qualified** — blocked on an approved BIOS dump and legal test content |
| Saturn | Beetle Saturn (`mednafen_saturn_libretro`) | **Candidate** | https://github.com/libretro/beetle-saturn-libretro | `GPL-2.0-or-later` | Unknown | Not implemented | Research-only |
| Dreamcast | Flycast (`flycast_libretro`) | **Candidate** | https://github.com/flyinghead/flycast | `GPL-2.0-or-later` | Unknown | Not implemented | Research-only |
| GameCube | Dolphin (`dolphin_libretro`) | **Approved** (M7) | https://github.com/libretro/dolphin (upstream dolphin-emu/dolphin) | `GPL-2.0-or-later` | **`fd1aca3af7db75504ed7512406d8a4cf4187110a`** — top-level revision proven by M10.3 from the full `SCM_REV_STR` embedded in the shipped binary; submodule closure not independently verified | Implemented (Release 002) | **Partially qualified** — runtime/core/`Sys` path verified; content execution **not confirmed** |

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
| `genesis_plus_gx_libretro` | In pinned bundle, but **blocked for V1** (§5.2) | not assessed | not assessed | not assessed |
| `picodrive_libretro` | In pinned bundle, but **blocked for V1** (§5.2) | not assessed | not assessed | not assessed |

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

All statuses below are **engineering** statuses under RetroFrontier's current licensing and
distribution policy. None is a legal conclusion, and none authorises distribution.

| System | Redistribution status | Remaining blocker |
|---|---|---|
| NES | **Blocked for V1** | Corresponding source for the redistributed binary is unknown (§5.3). |
| SNES | **Blocked for V1** | Corresponding source unknown (§5.3). |
| Nintendo 64 | Blocked for V1 | Licence identity conflict (§5.1) + corresponding source + not implemented. |
| Game Boy | Blocked for V1 | Corresponding source strategy undecided; not implemented; not qualified. |
| Game Boy Color | Blocked for V1 | As Game Boy. |
| Game Boy Advance | Blocked for V1 | As Game Boy. |
| Mega Drive / Genesis | **Blocked for V1** | Both mainstream cores carry a non-commercial licence term (§5.2); legal compatibility review required. BlastEm is the only GPL-compatible candidate and is unqualified. |
| PlayStation | **Blocked for V1** | Corresponding source unknown (§5.3); **and** the GPL-2.0-only / GPLv3-host separate-work question is open (§5.4). |
| Saturn | Blocked for V1 | Corresponding source; content-format mismatch (§4.2); not implemented. |
| Dreamcast | Blocked for V1 | Corresponding source; the catalog BIOS entry contradicts the *candidate* core's documentation (§4.3); not implemented. |
| GameCube | **Blocked for V1** | The *core's* top-level revision is **proven** (M10.3: `fd1aca3a…`), but three independent gates remain: the source checkout is not yet archived or published; the **`dolphin-sys` support asset** still has `source_revision: null`, a non-version-addressed upstream and no immutable mirror; and **content execution is still not confirmed**. Obtaining the other cores' revisions from libretro would not affect any of these. See [`docs/CORE_BUILD_PROVENANCE.md`](CORE_BUILD_PROVENANCE.md) §3.3. |

Every V1 system is currently blocked from public redistribution. For NES, SNES and GameCube the
blocker is provenance, not policy. PlayStation carries a second, independent legal-review gate on
top of provenance (§5.4).

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

### 4.2 PlayStation `.iso` is advertised but not loadable (live defect)

Unlike §4.1, this affects a system that is **Approved and Implemented today** — and that is shipped
to users despite not being qualified end-to-end.

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

### 4.3 Dreamcast BIOS layout conflict

Flycast — a **Candidate**, not an approved core — documents that its firmware lives in a **`dc/`
subdirectory** of RetroArch's system directory, not at the top level. RetroFrontier's BIOS discovery
has no notion of a core-required internal layout. This is the already-open backlog item *"map user
BIOS folders to any future core-required internal layout"* and it is a hard precondition of
Dreamcast approval. See [`docs/BIOS_MATRIX.md`](BIOS_MATRIX.md) §4.

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

What this establishes as fact:

- Neither licence is an SPDX standard licence (GitHub reports `NOASSERTION` for both).
- Both are **non-free** by the usual definition: a field-of-use restriction on commercial activity.
- Both terms therefore conflict with the freedoms a `GPL-3.0-or-later` work grants downstream, which
  is why neither core is suitable for automatic V1 approval under ADR-009.

What this does **not** establish, and what M10.2 deliberately does not assert:

- These licences are not "no redistribution" licences. They permit redistribution under conditions —
  notably non-commercial use, complete source for modified redistributions, and notice reproduction.
  M10.2 does not claim that shipping either core is categorically unlawful.
- Whether a separately built, `dlopen`-loaded libretro core forms a combined work with GPLv3
  RetroArch, or is an aggregate of separate works, is an **open legal question** this repository
  already flags (§5.4). The answer changes the analysis, and RetroFrontier is not the right party to
  decide it.

**Engineering status: Blocked for V1 under the current licensing/distribution policy — non-commercial
licence; legal compatibility review required.**

That status is sufficient to decide the engineering question, and it is deliberately narrower than a
legal conclusion. In practice it means: do not approve Genesis Plus GX, do not approve PicoDrive, do
not add either to a Runtime Release, and do not give Mega Drive a fallback core.

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

### 5.3 The four redistributed cores have no corresponding source available to RetroFrontier

> **Partially superseded by M10.3** ([`docs/CORE_BUILD_PROVENANCE.md`](CORE_BUILD_PROVENANCE.md)).
> Dolphin's exact top-level revision — `fd1aca3af7db75504ed7512406d8a4cf4187110a` — **was**
> recovered, from the full `SCM_REV_STR` embedded in the shipped binary, so the "shipped binaries"
> check below is wrong for Dolphin. For the other three, **the exact source revision remains
> unknown**; a specific public-CI candidate revision has been identified for each and awaits libretro
> confirmation. A candidate is not corresponding source. The operative conclusion in this section is
> unchanged, and **exactly three** core revisions remain unproven.

Release 002 records `"source_revision": null` for `nestopia`, `bsnes-mercury-balanced`,
`beetle-psx` and `dolphin`.

The exact revision is **unknown — not recoverable from the currently published bundle, the shipped
binaries, libretro's core-info metadata, or the public buildbot metadata M10.2 examined.** Four
independent checks were made and all were negative; the full analysis is in
[`docs/SOURCE_PROVENANCE.md`](SOURCE_PROVENANCE.md) §2.

This is scoped to what RetroFrontier can obtain from public sources. libretro's own build
infrastructure may still be able to identify the revision that produced each binary, which is
exactly why "ask libretro for build provenance" is a live strategy rather than a dead end
([`docs/SOURCE_PROVENANCE.md`](SOURCE_PROVENANCE.md) §5, strategy B).

The operative conclusion is unchanged: **RetroFrontier cannot currently satisfy corresponding-source
requirements from the evidence available to it**, so public redistribution stays blocked.

Beetle PSX is additionally recorded as `GPL-2.0-only` in Release 002 while
`mednafen/psx/gpu.c` carries a "version 2 … or any later version" header. The declared identifier is
the conservative reading and is safe to distribute under, but the aggregate `only`/`or-later`
disposition has not been established by a per-file audit. Recorded as `GPL-2.0 (aggregate)` above.

### 5.4 The core/host separate-work question (affects PlayStation in particular)

RetroArch is GPLv3. libretro cores are separately built native libraries that RetroArch loads with
`dlopen` across a stable C ABI. Whether such a core is a **separate work merely aggregated** with the
host, or forms a **combined work** with it, is an unresolved legal question — and it is not one this
milestone can answer.

It matters most for **`beetle-psx`**, whose Release 002 licence is `GPL-2.0-only`. GPL-2.0-only and
GPL-3.0 are mutually incompatible *for combining into one work*. If the separate-work reading holds,
shipping both in one Runtime Release is aggregation and raises no conflict; if it does not, the
combination needs a different answer.

The same question would apply to Gambatte (GPL-2.0-only) if it were ever adopted, and it does not
arise for `nestopia` or `dolphin`, whose `or-later` grants reach GPLv3.

Consequently PlayStation's redistribution row carries **two** independent gates — corresponding
source (§5.3) *and* this one — and closing the first would not by itself unblock it.
**Legal review required. M10.2 asserts no answer.**

## 6. Why no system was approved by M10.2

Every remaining system fails the decision rule on the *same* clause: the corresponding-source path
for a redistributed libretro binary is not merely undecided but currently unavailable from the
approved acquisition strategy (§5.3). Approving four more cores would multiply an unresolved
redistribution obligation across four more systems while closing nothing.

Two systems fail on further, independent grounds: Nintendo 64 on licence identity (§5.1), and Mega
Drive on non-commercial licence terms with no qualified GPL-compatible alternative (§5.2).

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
| Saturn | Yes | **Blocked** — policy not approved; catalog format mismatch (§4.1) |
| Dreamcast | Yes | **Blocked** — policy not approved; `dc/` BIOS layout unsupported (§4.3) |
| Mega Drive | No GPL-compatible core is qualified | **Blocked** — non-commercial licence terms; legal review required (§5.2) |

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
