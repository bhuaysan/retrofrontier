# Runtime Source and Provenance Matrix

Licence, redistribution and corresponding-source closure for every redistributed Runtime component.
Produced by M10.2 alongside [`docs/CORE_MATRIX.md`](CORE_MATRIX.md) and
[`docs/BIOS_MATRIX.md`](BIOS_MATRIX.md).

**Scope.** This document produces the *data* a public source/provenance bundle will need. It does
not perform production legal distribution, and it is not legal advice. Points requiring professional
review are marked **Legal review required**.

## Headline finding

> RetroFrontier cannot currently satisfy GPL corresponding-source obligations for the four
> redistributed libretro cores in Release 002, because the exact source revision of each shipped
> binary is **not published by the upstream that produced it** and is **not recoverable from the
> artefacts themselves**.

This is not a gap in RetroFrontier's record-keeping that can be closed by looking harder. It is a
property of libretro's stable core-bundle distribution. It blocks **public redistribution of the
managed runtime** — including for the four already-approved systems — until one of the strategies in
§5 is adopted.

Release 002 is otherwise sound: it is version-addressed, digest-pinned, and reconstructable. The
problem is corresponding *source*, not binary integrity.

## 1. Release 002 components

Release `rf-runtime-1.22.2-linux-x86_64-002`, manifest `rf-runtime-linux-x86_64-002`. **Unchanged by
M10.2.** Values read from `release/linux-x86_64/runtime-release.json` and from the installed,
authenticated manifest of the active installation.

| Component | Kind | Declared licence | Binary identity strategy | `source_revision` | Corresponding source | Gap |
|---|---|---|---|---|---|---|
| `retroarch` | runtime | `GPL-3.0-only` | Digest-pinned member of a version-addressed 7z | **`69a4f0e`** | Reconstructable: RetroArch 1.22.2 is a tagged upstream release and the revision is recorded | **Abbreviated revision** — 7 hex characters, not a full 40-character commit id |
| `nestopia` | core | `GPL-2.0-or-later` | Digest-pinned member of the pinned core bundle | **`null`** | **Unknown** | Full corresponding-source gap |
| `bsnes-mercury-balanced` | core | `GPL-3.0-only` | as above | **`null`** | **Unknown** | Full corresponding-source gap |
| `beetle-psx` | core | `GPL-2.0-only` | as above | **`null`** | **Unknown** | Full gap, plus `only`/`or-later` disposition unaudited (§3.2) |
| `dolphin` | core | `GPL-2.0-or-later` | as above | **`null`** | **Unknown** | Full corresponding-source gap |
| `dolphin-sys` | support asset | `GPL-2.0-or-later` | Digest-pinned subtree of a **non-version-addressed** zip | **`null`** | Unknown | Gap, plus a non-immutable upstream URL (§4) |
| `joypad-autoconfig` | support asset | `MIT` | Digest-pinned subtree of a commit-addressed zip | **`38cf938bba0adbde375972053068f10d955a9d14`** | **Closed** — full commit id, permissive licence | **None** |

Exactly one component — `joypad-autoconfig` — has fully closed provenance. It is also the only
permissively licensed one, and the only one acquired from a commit-addressed URL. That is not a
coincidence; it is the model the others need.

## 2. How the corresponding-source gap was established

The claim "the revision is unrecoverable" is itself evidence-backed rather than assumed. Four
independent checks, all negative:

1. **The authenticated manifest.** Every core component records
   `"source_revision": null` and pins only `source_pinning: "sha256:4b7ed8dc…"` — the *bundle*
   digest, which identifies the aggregate download, not any core's source.
2. **The shipped binaries.** The four Release 002 core `.so` files in the active installation were
   inspected directly. They carry a GNU build-id and no embedded upstream revision, version control
   string, or commit identifier:

   | Component | Installed `.so` SHA-256 | GNU build-id |
   |---|---|---|
   | `nestopia` | `3f1b76f6d8e68c149a3269c314b406d15f806597333466b1f6a0af01afab52c7` | `8f18c1eed82244fe24d89783f7c3c6c7ba31f4ab` |
   | `bsnes-mercury-balanced` | `06fe34874cf8fdec00801a2d22c497c477721a23a87a6e7b5cae82dc1770c5be` | `3843d7c2ecdd0ba55f3bda9819437801cc47aa73` |
   | `beetle-psx` | `ffc1c18a1fc41bf1f28cccaaa7e30e6677ec2aeda91c39b2d8f72d3bd4e2e641` | `4a982e5ed3f47f4a0e1635c0e87479f90fa16ec6` |
   | `dolphin` | `c28dc9a2207ffed938810abf3e24df23dc39ef58c6a16c036fc2c58c2240ef10` | `0c693b7863fb713d45c41b54e7715111d77da1fb` |

   A GNU build-id is a hash of the linked output. It **is not a source revision** and cannot be
   mapped to a commit without the builder's own records. It is recorded here because it exactly
   pins the binary and would let libretro identify the build if asked.
3. **libretro core metadata.** `libretro/libretro-core-info` `.info` files carry only a
   human-readable `display_version` (for example `1.53.1` for Nestopia, `0.10-dev` for mGBA). No
   field for a source revision exists in the format.
4. **The buildbot.** `buildbot.libretro.com/stable/1.22.2/` publishes archives only. No per-core
   build manifest, build log, or revision record accompanies the stable bundle.

**Conclusion.** For a core taken from libretro's stable bundle, the corresponding source revision is
knowable only to the libretro build infrastructure. Any claim RetroFrontier made about it would be a
guess, and a guess is exactly what GPL §3 does not accept.

## 3. Licence verification

Every identifier below was verified against the **upstream licence file and per-file headers**, not
against GitHub's detected-licence field (which is heuristic, and reports `NOASSERTION` for three of
these repositories).

### 3.1 Verified identifiers

| Component | Declared in Release 002 | Verified | Verdict |
|---|---|---|---|
| `retroarch` | `GPL-3.0-only` | RetroArch is GPLv3 | Consistent |
| `nestopia` | `GPL-2.0-or-later` | `source/core/NstBase.hpp`: "version 2 … or (at your option) any later version" | **Confirmed exact** |
| `bsnes-mercury-balanced` | `GPL-3.0-only` | `LICENSE` is the GPLv3 text; no "or later" grant found | **Confirmed** (conservative and defensible) |
| `beetle-psx` | `GPL-2.0-only` | `COPYING` is GPLv2; but `mednafen/psx/gpu.c` says "version 2 … or any later version" | **Inconsistent — see §3.2** |
| `dolphin` | `GPL-2.0-or-later` | `Source/Core/Core/Core.cpp`: `SPDX-License-Identifier: GPL-2.0-or-later` | **Confirmed exact** |
| `dolphin-sys` | `GPL-2.0-or-later` | Ships Dolphin's GPL-2.0 licence text | Consistent |
| `joypad-autoconfig` | `MIT` | MIT | Confirmed |

### 3.2 Beetle PSX `only` versus `or-later`

Release 002 declares `GPL-2.0-only`, while at least one Mednafen file in the tree grants "or later".
Mednafen's project-level position is GPLv2, and a mixed tree is distributed under the most
restrictive applicable term, so `GPL-2.0-only` is the **safe and conservative** declaration and is
not a compliance defect.

It is nonetheless not *established*: no per-file audit has been done. Recorded as
`GPL-2.0 (aggregate)` in the core matrix. **Legal review required** before the notice file is
published.

### 3.3 Non-free licences discovered (not redistributable)

Neither is in Release 002 and neither may ever enter one. Full analysis in
[`docs/CORE_MATRIX.md`](CORE_MATRIX.md) §5.2.

| Core | Licence file | Term |
|---|---|---|
| `genesis_plus_gx` | `LICENSE.txt` | "Redistributions may not be sold, nor may they be used in a commercial product or activity." |
| `picodrive` | `COPYING` | Identical non-commercial term |

Both are non-free, have no SPDX identifier, and are **GPL-incompatible**, so neither can be
distributed as part of RetroFrontier's `GPL-3.0-or-later` work (ADR-010).

## 4. `dolphin-sys` has a non-immutable upstream

`https://buildbot.libretro.com/assets/system/Dolphin.zip` is **not version-addressed**. Release 002's
own provenance note already concedes this: "Upstream publishes no version-addressed path for this
asset, so the pinned digest and the maintainer input cache are what keep it reconstructable."

That is honest but weak. The pinned digest guarantees RetroFrontier will *detect* a change; it does
not guarantee the bytes remain *obtainable*. If upstream replaces the asset, Release 002 becomes
unreconstructable from public sources — the same failure that forced the Release 001 → 002 migration
away from rolling nightly core URLs.

**This is a second, independent immutability gap** and must be closed by mirroring the exact pinned
bytes into RetroFrontier-controlled immutable storage (§5, strategy A), not by re-pinning.

## 5. Strategies for closing the corresponding-source gap

Recorded for decision. **M10.2 selects none of them** — the choice is a product owner and legal
decision with real cost implications.

### A. Mirror inputs, publish an offer for what we have
Archive the exact pinned input bytes in RetroFrontier-controlled immutable storage and publish a
written offer plus notices. **Closes** availability of the *binaries* and fixes §4. **Does not
close** the corresponding-source obligation, because we still cannot supply source matching the
binary. Necessary but insufficient on its own.

### B. Obtain build provenance from libretro
Ask libretro to publish, or supply, the per-core source revisions for a stable bundle. **Closes the
gap completely and cheaply if granted.** Depends entirely on a third party, and gives RetroFrontier
no ability to reproduce the build itself.

### C. Build the cores from pinned revisions (recommended for a public release)
RetroFrontier builds each approved core from an explicit upstream commit and ships its own binaries.

- Corresponding source becomes exact **by construction** — the commit is chosen, not discovered.
- `source_revision` becomes a real 40-character commit id for every core.
- Enables reproducible builds, per-platform control, and closes macOS, where no immutable bundle
  exists at all ([`docs/CORE_MATRIX.md`](CORE_MATRIX.md) §2.1).
- Cost is substantial: a four-platform build and signing pipeline, and RetroFrontier becomes the
  distributor of binaries it compiled, with the review burden that implies.

This is the only strategy that closes both the corresponding-source gap **and** the macOS
acquisition gap, which are otherwise two separate blockers.

### D. Ship no cores; have the user supply them
Rejected on sight. It contradicts ADR-003, ADR-009 and ADR-012: cores are native code executing in
the RetroArch process and must be authenticated components of an approved release.

## 6. What a source/provenance bundle must contain

Derived from this analysis, for whichever strategy is chosen:

1. For every redistributed component: exact licence identifier and full licence text.
2. Complete copyright notices for each upstream project.
3. For every GPL component: corresponding source at the **exact** revision that produced the binary,
   or a valid written offer — currently unsatisfiable for the four cores under strategies A/B.
4. Build recipes and any RetroFrontier-applied patches sufficient to reproduce each binary.
   RetroFrontier applies **no** patches today: every component is an unmodified digest-pinned
   extraction, which is worth stating explicitly in the notices.
5. The exact input URLs and SHA-256 digests — already present and complete in Release 002.
6. Mirrored immutable copies of every input, including the `dolphin-sys` asset (§4).

Items 1, 2, 5 and 6 are achievable now. Item 3 is the blocker. Item 4 is trivially satisfiable today
and becomes substantive under strategy C.

## 7. What is *not* claimed

Stated plainly, because the point of this document is to prevent optimistic reading:

- **No GPL compliance claim is made.** An upstream Git repository existing is not corresponding
  source for a binary built from an unknown revision.
- **No public distribution has occurred**, and none is authorised by this document.
- **Binary availability is not redistribution approval.** A core being in the pinned bundle says
  nothing about whether it may be shipped.
- **Digest pinning is not source provenance.** Release 002 is reconstructable and still cannot
  satisfy GPL §3.
- **Legal review remains necessary** on: the `only`/`or-later` audit (§3.2); whether libretro cores
  loaded via `dlopen` are a separate work from GPL-3.0 RetroArch — which matters for the
  GPL-2.0-only cores (`beetle-psx`, and Gambatte if ever adopted); the exact form of the written
  offer; and MPL-2.0 notice obligations if mGBA is adopted.

## 8. Evidence

Primary sources, 2026-09-04. Community lists were not used for licensing or redistribution.

- `release/linux-x86_64/runtime-release.json` and the installed authenticated release manifest of
  the active installation (`rf-runtime-linux-x86_64-002`).
- The Release 002 core bundle, verified locally as
  `sha256:4b7ed8dc97d4bf035fce182c64b5658c7662e2e9e5d42129538afbd4b6096307`.
- Direct ELF inspection of the four installed Release 002 core binaries.
- Upstream licence files and per-file headers listed in [`docs/CORE_MATRIX.md`](CORE_MATRIX.md) §10.
- `libretro/libretro-core-info` `.info` metadata.
- Official libretro buildbot indexes for stable 1.22.2 and the nightly Apple/Windows paths.
