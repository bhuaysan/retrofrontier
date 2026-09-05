# Runtime Source and Provenance Matrix

Licence, redistribution and corresponding-source closure for every redistributed Runtime component.
Produced by M10.2 alongside [`docs/CORE_MATRIX.md`](CORE_MATRIX.md) and
[`docs/BIOS_MATRIX.md`](BIOS_MATRIX.md).

**Scope.** This document produces the *data* a public source/provenance bundle will need. It does
not perform production legal distribution, and it is not legal advice. Points requiring professional
review are marked **Legal review required**.

> **Amended by M10.3.** [`docs/CORE_BUILD_PROVENANCE.md`](CORE_BUILD_PROVENANCE.md) recovered
> **Dolphin's exact top-level source revision** — `libretro/dolphin` @
> `fd1aca3af7db75504ed7512406d8a4cf4187110a` — from the shipped binary itself, which embeds the full
> 40-character `SCM_REV_STR` its build emits from `git rev-parse HEAD`. The repository is identified
> by libretro's historical `libretro-super` build recipe.
>
> **The headline finding and the §1 component table below have been updated to that position and are
> the current, authoritative statement.** Two sections are retained as **historical** and are marked
> as such where they appear: §2 records M10.2's four checks as performed, and its check 2 ("the
> shipped binaries carry no embedded upstream revision") is wrong for Dolphin and is corrected in
> place there.
>
> Nothing here is a compliance claim. Proving a revision is neither producing corresponding source
> nor clearing a system for distribution: GameCube still carries the separate `dolphin-sys`
> provenance/immutability gap (§4 below) and unconfirmed content execution.

## Headline finding (current, as amended by M10.3)

> **RetroFrontier cannot currently satisfy GPL corresponding-source obligations for any of the four
> redistributed libretro cores in Release 002.** For Dolphin the *top-level source revision* is now
> proven (`fd1aca3af7db75504ed7512406d8a4cf4187110a`, recovered by M10.3 from the shipped binary);
> complete corresponding source for it has still not been materialised, archived or published. For
> Nestopia, bsnes-mercury Balanced and Beetle PSX the exact source revision remains **unknown** — a
> public-CI **candidate** is identified for each and awaits libretro confirmation.

Three layers, stated separately because collapsing them is the likely misreading:

| Layer | Dolphin | Other three cores |
|---|---|---|
| **Top-level revision provenance** | **CLOSED / PROVEN** by M10.3 | **OPEN** — candidate identified, unconfirmed |
| **Complete corresponding-source materialisation and archive** | **OPEN** — 33 submodule pins determined by the commit but not archived; notices and source bundle not produced | **OPEN**, necessarily |
| **Public redistribution** | **OPEN / BLOCKED** (§4 `dolphin-sys`, hosting, key ceremony) | **OPEN / BLOCKED**; PlayStation carries an additional legal gate (§3.3) |

Scope of the revision claim, stated precisely because the difference matters:

- For the three cores it is a statement about **evidence available to RetroFrontier from public
  sources**, established by the checks in §2 — not a claim that the information has ceased to exist.
- libretro's own build infrastructure may well still be able to identify which revision produced a
  given binary. That is precisely why asking for it is a live strategy (§5, strategy B) rather than
  a dead end.
- For Dolphin the revision was recoverable after all, from the binary itself. §2 check 2 below is the
  M10.2 finding that M10.3 corrected.
- What *is* established is the operative conclusion: RetroFrontier still has no complete
  corresponding source for any core, so it cannot meet the obligation today.

This blocks **public redistribution of the managed runtime** — including for the four
already-approved systems — until one of the strategies in §5 is adopted.

**Exactly three** top-level core revisions remain unproven: `nestopia`, `bsnes-mercury-balanced` and
`beetle-psx`. Details in [`docs/CORE_BUILD_PROVENANCE.md`](CORE_BUILD_PROVENANCE.md).

Release 002 is otherwise sound: it is version-addressed, digest-pinned, and reconstructable. The
problem is corresponding *source*, not binary integrity.

## 1. Release 002 components

Release `rf-runtime-1.22.2-linux-x86_64-002`, manifest `rf-runtime-linux-x86_64-002`. **The release
itself is unchanged by M10.2 and by M10.3** — it remains byte-identical, and every `source_revision`
below is the value the immutable manifest actually records. Values read from
`release/linux-x86_64/runtime-release.json` and from the installed, authenticated manifest of the
active installation.

The `source_revision` column is what the **immutable release** records. The next column is what
RetroFrontier has since **recovered externally**; recovering a revision does not and must not
retroactively change a published release.

| Component | Kind | Declared licence | Binary identity strategy | Manifest `source_revision` | Externally recovered revision | Revision provenance | Complete corresponding source / archive |
|---|---|---|---|---|---|---|---|
| `retroarch` | runtime | `GPL-3.0-only` | Digest-pinned member of a version-addressed 7z | **`69a4f0e`** | — | **Abbreviated** — 7 hex characters, not a full 40-character commit id | **Open** — reconstructable in principle (1.22.2 is a tagged release), not archived |
| `nestopia` | core | `GPL-2.0-or-later` | Digest-pinned member of the pinned core bundle | **`null`** | none — candidate only: `5deada54077fae87e2873f5ad9ef77e3ab7af5e1` (unconfirmed) | **Open — unknown** | **Open** |
| `bsnes-mercury-balanced` | core | `GPL-3.0-only` | as above | **`null`** | none — candidate only: `0f35d044bf2f2b879018a0500e676447e93a1db1` (unconfirmed) | **Open — unknown** | **Open** |
| `beetle-psx` | core | `GPL-2.0-only` | as above | **`null`** | none — candidate only: `d6383bff89a93e02aad10a586e804829861c3de1` (unconfirmed) | **Open — unknown** | **Open**; `only`/`or-later` disposition unaudited (§3.2); **and** the separate-work question against the GPLv3 host is open (§3.3) |
| `dolphin` | core | `GPL-2.0-or-later` | as above | **`null`** (unchanged in Release 002) | **`fd1aca3af7db75504ed7512406d8a4cf4187110a`** — proven from the shipped binary by M10.3 | **CLOSED — top-level revision proven** | **Open** — 33 submodule pins determined by that commit but not materialised or archived; notices and source bundle not produced |
| `dolphin-sys` | support asset | `GPL-2.0-or-later` | Digest-pinned subtree of a **non-version-addressed** zip | **`null`** | — | **Open — unknown.** M10.3 proved nothing here | **Open**, plus a non-immutable upstream URL (§4) |
| `joypad-autoconfig` | support asset | `MIT` | Digest-pinned subtree of a commit-addressed zip | **`38cf938bba0adbde375972053068f10d955a9d14`** | — | **Closed** — full commit id in the manifest | **Closed** — permissive licence, no corresponding-source obligation |

Reading the table:

- **Three** core revisions remain unproven — `nestopia`, `bsnes-mercury-balanced`, `beetle-psx`. A
  candidate is not provenance and must never be written into `source_revision` or a notice file.
- Dolphin is the one core whose revision is settled, and it is settled in the *right-hand* column
  only: the immutable manifest still records `null`, and its corresponding source is still not
  archived. **Do not read the Dolphin row as compliance.**
- `joypad-autoconfig` remains the only component with fully closed provenance in the release itself.
  It is also the only permissively licensed one, and the only one acquired from a commit-addressed
  URL. That is not a coincidence; it is the model the others need.

## 2. How the corresponding-source gap was established (M10.2 — check 2 superseded)

> **Historical section.** This records the four checks **as M10.2 performed them**. Check 2 was
> subsequently **found wrong for Dolphin** by M10.3 and is corrected in place below. The other three
> checks stand. For the current position, read the headline finding and §1 table above.

The claim "the revision is not recoverable from public sources" is itself evidence-backed rather than
assumed. Four checks, three of them negative and one since corrected. Each is a statement about a
specific source that was examined, and together they bound what RetroFrontier can obtain — they do
not bound what libretro knows internally:

1. **The authenticated manifest.** Every core component records
   `"source_revision": null` and pins only `source_pinning: "sha256:4b7ed8dc…"` — the *bundle*
   digest, which identifies the aggregate download, not any core's source.
2. **The shipped binaries.** ~~They carry a GNU build-id and no embedded upstream revision, version
   control string, or commit identifier.~~ **Superseded by M10.3.** The correct statement is:

   - **Dolphin does embed its revision.** `dolphin_libretro.so` carries the full 40-character
     `SCM_REV_STR` its own build emits from `git rev-parse HEAD`, assembled at one construction site
     from `.rodata` and an inline immediate. M10.2's contiguous `strings` scan could not see it.
     Recovered value: `fd1aca3af7db75504ed7512406d8a4cf4187110a`.
   - **For the other three, no embedded revision was identified by the inspection performed** — and
     that is the whole of the finding. No universal negative is asserted: an inspection that finds
     nothing does not prove nothing is there. Their documented candidate revisions were specifically
     checked for and are absent as embedded identifiers.

   The binary identity values M10.2 recorded are unaffected and were re-derived unchanged by M10.3:

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
4. **The buildbot.** The `buildbot.libretro.com/stable/1.22.2/` indexes examined publish archives
   only. No per-core build manifest, build log, or revision record accompanies the stable bundle in
   the paths M10.2 inspected.

**Conclusion, as corrected.** For **three** of the four cores taken from libretro's stable bundle,
the corresponding source revision is not derivable from any public artefact RetroFrontier examined.
For **Dolphin it was**, from the binary itself (check 2 above). For the three, the revision plausibly
remains identifiable
*inside* libretro's build infrastructure — the recorded GNU build-ids would likely let libretro
resolve it — but RetroFrontier has no access to that record today. Any revision RetroFrontier
asserted from what it can see would be a guess, and a guess is exactly what GPL §3 does not accept.

The negative result is therefore about **reach, not existence**, and it points at strategy B (§5) as
a genuinely open route rather than a formality.

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

The disposition also feeds §3.3: if the aggregate really is `GPL-2.0-only`, then `beetle-psx` is the
component for which the separate-work question bites hardest.

### 3.3 The core/host separate-work question is open

RetroArch is GPLv3; libretro cores are separately built libraries it loads with `dlopen` across a
stable C ABI. Whether such a core is a separate work merely aggregated with the host, or a combined
work with it, is **unresolved**, and M10.2 asserts no answer.

It matters concretely for the GPL-2.0-only components, since GPL-2.0-only and GPL-3.0 cannot be
combined into a single work:

| Component | Licence | Affected by the question? |
|---|---|---|
| `beetle-psx` | `GPL-2.0-only` (aggregate, §3.2) | **Yes — the main open case** |
| `nestopia` | `GPL-2.0-or-later` | No — the `or-later` grant reaches GPLv3 |
| `dolphin` | `GPL-2.0-or-later` | No |
| `bsnes-mercury-balanced` | `GPL-3.0-only` | No |

So **PlayStation carries two independent legal gates**, not one: corresponding source (§2) *and*
this. Closing the first would not by itself make PlayStation redistributable, and the core matrix
records both. **Legal review required.**

### 3.4 Non-commercial licences discovered (not approvable for V1)

Neither core is in Release 002 and neither is approved. Full analysis in
[`docs/CORE_MATRIX.md`](CORE_MATRIX.md) §5.2.

| Core | Licence file | Term |
|---|---|---|
| `genesis_plus_gx` | `LICENSE.txt` | "Redistributions may not be sold, nor may they be used in a commercial product or activity." |
| `picodrive` | `COPYING` | Identical non-commercial term |

Both are non-free by the usual definition and have no SPDX identifier. Their field-of-use restriction
conflicts with the freedoms a `GPL-3.0-or-later` work grants downstream (ADR-010), which is enough to
settle the engineering question.

**Engineering status: Blocked for V1 under the current licensing/distribution policy — legal
compatibility review required.** Neither core may be approved or added to a Runtime Release.

M10.2 stops there deliberately. It does **not** assert that these licences forbid redistribution
outright — they permit it under conditions — nor does it pre-judge the §3.3 separate-work analysis,
which would also bear on how a non-GPL-compatible core relates to a GPLv3 host.

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
gap completely and cheaply if granted**, and it is a realistic ask rather than a formality: §2
establishes only that the data is absent from *public* artefacts, and the recorded GNU build-ids give
libretro an exact handle on which builds are being asked about. Depends entirely on a third party,
and gives RetroFrontier no ability to reproduce the build itself. **Worth attempting before
committing to strategy C's cost.**

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
   or a valid written offer. This is **currently unsatisfied for all four Release 002 cores**. For
   three, the revision itself is still unknown. For Dolphin the revision is now proven, but the
   source at that revision — including its 33 submodules — has not been materialised, archived or
   published, so the obligation is equally unsatisfied there. Strategy A cannot close this gap;
   strategy B closes it only if libretro supplies the exact build provenance; strategy C closes it by
   construction.
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
- **No legal conclusion is asserted anywhere in this document.** Statuses such as "Blocked for V1"
  are engineering decisions under current policy, not findings about what the law permits.
- **Legal review remains necessary** on: the `only`/`or-later` audit (§3.2); the separate-work
  question for `dlopen`-loaded cores against the GPLv3 host (§3.3), which gates PlayStation
  independently of corresponding source and would also apply to Gambatte if adopted; the
  non-commercial cores' relationship to a GPLv3 host (§3.4); the exact form of the written offer;
  and MPL-2.0 notice obligations if mGBA is adopted.
- **"Not recoverable" means not recoverable by RetroFrontier from public sources** (§2), and it now
  applies to **three** cores, not four — Dolphin's top-level revision was recovered from the binary
  itself. It is not a claim that the remaining revisions are lost, and strategy B (§5) remains
  genuinely open.
- **A proven revision is not corresponding source.** Dolphin's revision being known does not mean its
  corresponding source is available, archived or published. That work is open, and nothing in this
  document reports it as done.

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
