# Core Build Provenance and Build Strategy

Authoritative M10.3 record: public provenance recovery for the four Release 002 core binaries, the
libretro outreach payload, Strategy B acceptance criteria, Strategy C four-platform feasibility, the
self-build archive model, and the resulting recommendation.

Companion documents:

- [`docs/SOURCE_PROVENANCE.md`](SOURCE_PROVENANCE.md) — licence, redistribution and
  corresponding-source closure. M10.3 **narrows** its headline finding; it does not overturn it. Its
  headline and Release 002 component table have been updated to the position recorded here.
- [`docs/CORE_MATRIX.md`](CORE_MATRIX.md) — policy, availability, implementation, qualification.
- [`docs/RUNTIME_MANAGER.md`](RUNTIME_MANAGER.md), [`docs/adr/ADR-012-runtime-trust-model.md`](adr/ADR-012-runtime-trust-model.md)
  — the trust semantics this milestone must not weaken.

**Scope.** Research and design only. M10.3 created no Runtime Release, approved no core, changed no
trust semantics, and left Release 002 byte-identical. It is not legal advice.

## Headline finding

> **Dolphin's top-level revision gap is closed; three core revision gaps remain. Complete
> corresponding-source publication for the Runtime is still open.**
>
> M10.2 reported that the source revision of all four Release 002 cores was "not recoverable from
> public sources". That is now **false for one core and materially overstated for the other three.**
>
> - **Dolphin's exact top-level source revision is recovered and proven** from the distributed binary
>   itself: `libretro/dolphin` @ `fd1aca3af7db75504ed7512406d8a4cf4187110a`. The binary embeds the
>   full 40-character `SCM_REV_STR` that Dolphin's own build emits from `git rev-parse HEAD`. No
>   inference from dates or version strings is involved.
> - **For Nestopia, bsnes-mercury Balanced and Beetle PSX the exact source revision remains
>   unknown.** A specific public-CI **candidate revision has been identified for each and awaits
>   libretro confirmation.** A candidate is not provenance, and does not become provenance by having
>   stayed unchanged across the release window.
> - The missing link for those three is narrow and specific: nothing public binds the **bundle member
>   bytes** to a **specific CI job**. libretro's CI destroys build artefacts after **10 minutes**, and
>   job logs require authentication.

### Three layers, never collapsed

This document is about **one** of three distinct obligations, and it closes part of only that one.
Every status in it is stated against this table.

| Layer | Dolphin | Nestopia / bsnes-mercury / Beetle PSX |
|---|---|---|
| **1. Top-level source revision provenance** — which revision produced the shipped binary | **CLOSED / PROVEN** (§3.1) | **OPEN.** Candidate identified, unconfirmed (§3.2) |
| **2. Complete corresponding-source materialisation and archive** — the actual source, including every submodule, mirrored immutably, with notices and a source bundle | **OPEN.** The 33 gitlink pins are determined by the proven commit but are not recoverable from the binary; the checkout is not materialised or archived; notices and bundle work is not started (§3.1, §7) | **OPEN**, necessarily — layer 1 is not closed either |
| **3. Public redistribution** | **OPEN / BLOCKED** (`dolphin-sys`, content execution, hosting, key ceremony — §3.3) | **OPEN / BLOCKED**; PlayStation additionally carries a separate legal gate (§3.3) |

Layer 1 is what M10.3 worked on. **Closing layer 1 for Dolphin does not close layer 2 for Dolphin,
and no GPL compliance claim is made anywhere in this document.**

The operative consequence for the milestone is therefore: **complete corresponding-source publication
for the Runtime remains open for all four cores, and public redistribution of the managed runtime
remains blocked.** What changed is the *shape* of the remaining revision gap, and therefore the cost
of closing it. The outreach to libretro is no longer "please excavate unknown information" but
"please confirm three specific, already-identified candidate revisions and their job binding" — a far
cheaper and far more answerable request.

**Proving a core's source revision is not the same as clearing a system for distribution.** Even for
GameCube, where the core revision is now proven, distribution remains blocked by the separate
`dolphin-sys` provenance/immutability gap and by unconfirmed content execution. See §3.3.

## 1. Proof standard used in this document

M10.3 applies the standard the milestone set, deliberately and strictly:

> A revision is **recovered** only when there is an authenticated or otherwise unambiguous chain from
> the exact distributed binary to that exact source revision.

Three grades are used, and they are never conflated:

| Grade | Meaning | Admitted evidence |
|---|---|---|
| **Proven** | The chain runs from the distributed bytes themselves to one revision, with no unproven step. | Revision constants emitted by the source's own build system into the shipped binary; unique resolution of that identifier in the correct repository. |
| **Candidate (high confidence)** | A single named revision is strongly indicated by primary build-infrastructure records, but one link in the chain is not publicly verifiable. | Public CI pipeline records; a toolchain fingerprint compatible with the documented public CI image for that build path (corroboration only — it identifies no job, see §2.1); stability of the revision across the whole release window. |
| **Unknown** | No named revision. | — |

Explicitly **not** admitted as proof anywhere in this document:

- "the commit that was HEAD around the build date" (date correlation alone),
- mapping a `display_version` or product version string to a commit,
- substituting a branch head for a historical revision,
- a GNU build-id, which is a hash of the *linked output* and is not a source revision,
- a build recipe (`.gitlab-ci.yml`), which describes how a build *would* run and is not a record that
  a particular build *did* run,
- upstream source merely *existing*, which is availability, not corresponding source.

## 2. Verified binary identity of the four Release 002 cores

Every value below was **re-derived on `main` at `f280905`** from the authenticated active
installation (`rf-runtime-linux-x86_64-002`, installation `i-18d14638042bd789-1-51189`) and compared
against M10.2. **All eight SHA-256 and build-id values match M10.2 exactly.** No value was copied
without re-checking.

Release `rf-runtime-1.22.2-linux-x86_64-002`. All four cores derive from one input:
`https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch_cores.7z`,
`sha256:4b7ed8dc97d4bf035fce182c64b5658c7662e2e9e5d42129538afbd4b6096307`, 274,237,400 bytes,
published **2025-11-20 02:50** per the buildbot index.

| RetroFrontier id | libretro core filename | Installed `.so` SHA-256 | GNU build-id | Release 002 `.tar` target digest |
|---|---|---|---|---|
| `nestopia` | `nestopia_libretro.so` | `3f1b76f6d8e68c149a3269c314b406d15f806597333466b1f6a0af01afab52c7` | `8f18c1eed82244fe24d89783f7c3c6c7ba31f4ab` | `9ef74939752057dbf8aae167984d909a2053e03d76e145c1b5cf993e174fd0d6` |
| `bsnes-mercury-balanced` | `bsnes_mercury_balanced_libretro.so` | `06fe34874cf8fdec00801a2d22c497c477721a23a87a6e7b5cae82dc1770c5be` | `3843d7c2ecdd0ba55f3bda9819437801cc47aa73` | `3e13256e7f9f0bc73a9011460c2064644ac2f8e2d68461a97b7a2edbc2114f95` |
| `beetle-psx` | `mednafen_psx_libretro.so` | `ffc1c18a1fc41bf1f28cccaaa7e30e6677ec2aeda91c39b2d8f72d3bd4e2e641` | `4a982e5ed3f47f4a0e1635c0e87479f90fa16ec6` | `8112f600f7f69c861edb2c09e1389cdfaff9a3925bd86d06888147cfc1360251` |
| `dolphin` | `dolphin_libretro.so` | `c28dc9a2207ffed938810abf3e24df23dc39ef58c6a16c036fc2c58c2240ef10` | `0c693b7863fb713d45c41b54e7715111d77da1fb` | `42fab8f87403f32d71eeeeb29bb13f1eccffc347082dfb377b901a5c6144d3df` |

The two digest columns differ because Release 002 addresses each core as a single-entry `.tar`; the
installed `.so` digest is the raw member. Both are recorded because the outreach request needs the
`.so` digest, while the release definition pins the `.tar`.

### 2.1 Toolchain fingerprint recovered from the binaries

Not previously recorded. Read from each binary's `.comment` section and dynamic metadata:

| Component | Compiler recorded in the binary | Max `GLIBC_` symbol version |
|---|---|---|
| `nestopia` | `GCC: (Ubuntu 9.4.0-1ubuntu1~16.04) 9.4.0` | `GLIBC_2.14` |
| `bsnes-mercury-balanced` | `GCC: (Ubuntu 9.4.0-1ubuntu1~16.04) 9.4.0` | `GLIBC_2.14` |
| `beetle-psx` | `GCC: (Ubuntu 9.4.0-1ubuntu1~16.04) 9.4.0` | `GLIBC_2.14` |
| `dolphin` | `GCC: (Ubuntu 12.4.0-1ubuntu1~18.04.sav0) 12.4.0` (plus `7.5.0-3ubuntu1~18.04` objects) | `GLIBC_2.27` |

This is a genuine, checkable observation about the shipped bytes: the compiler each binary records is
exactly the compiler in the documented public libretro CI image for its build path (§4.2). It
**corroborates compatibility with the documented libretro CI toolchain and strengthens the candidate
attribution in §3.2.**

It does **not** identify or authenticate the producing CI job. Stated precisely, a `.comment` string
does not establish:

- **who** built the binary — anyone with the same compiler produces the same string,
- **which CI job or pipeline** built it — the string carries no job, pipeline or timestamp,
- **which container image uniquely** produced it — the string identifies a Ubuntu compiler package,
  which many images can contain, not a specific image digest.

It is corroboration of one link, not the missing binding described in §3.2, and it is never used as a
source revision.

## 3. Per-core recovery result

### 3.1 Dolphin — **PROVEN**

**Exact source revision: `libretro/dolphin` @ `fd1aca3af7db75504ed7512406d8a4cf4187110a`.**

#### The revision: emitted by Dolphin's own build machinery into the shipped binary

Dolphin's build produces the value directly from git and bakes it into the binary:

1. CMake runs `git rev-parse HEAD` into `DOLPHIN_WC_REVISION`.
2. `scmrev.h.in` emits that exact value as **`SCM_REV_STR`**.
3. `Version.cpp` exposes it through **`GetScmRevGitStr()`**.

The **full 40-character `SCM_REV_STR` is present in the authenticated Release 002 binary.** It is
materialised as a 40-character `std::string`, split by the compiler across two storage locations —
which is why a naive contiguous `strings` scan does not reveal it, and why M10.3's first pass
understated it as a 32-character prefix.

The evidence is **instruction-level, at one construction site**, at `.text` `0xe73c86`. This matters:
finding the 32-character fragment somewhere in the file and the 8-character fragment somewhere else
in the file would prove nothing, because two unrelated occurrences would satisfy it. What is claimed
here — and what `docs/research/m10-3/verify-core-provenance.sh` mechanically re-derives — is that a
single site assembles all 40 characters from its own operands:

| Instruction at the site | Effect |
|---|---|
| `mov $0x29,%edi` → `call operator new` | allocates **41** bytes = 40 characters + NUL |
| `movdqa 0x27db21(%rip),%xmm0` (→ `0x10f17c0`) → `movups %xmm0,(%rax)` | writes characters 0–15 from `.rodata` |
| `movdqa 0x27db07(%rip),%xmm0` (→ `0x10f17d0`) → `movups %xmm0,0x10(%rax)` | writes characters 16–31 from `.rodata` |
| `movabs $0x6130313137383134,%rcx` → `mov %rcx,0x20(%rax)` | writes characters 32–39 as an inline immediate |
| `movb $0x0,0x28(%rax)` | NUL-terminates at offset 40 |
| `movq $0x28,…` | stores the `std::string` length as **0x28 = 40** |

All six write through the **same base register** into the **same 41-byte allocation**. Reassembling
the operands that site itself names, in store order:

```
.rodata[0x10f17c0 .. +16]  = fd1aca3af7db7550
.rodata[0x10f17d0 .. +16]  =                 4ed7512406d8a4cf
movabs immediate, LE       =                                 4187110a
                             ----------------------------------------
full SCM_REV_STR (len 40)  = fd1aca3af7db75504ed7512406d8a4cf4187110a
```

The `.rodata` region is bounded by Dolphin's other scm_rev literals — the revision string
`Dolphin [HEAD] ` immediately precedes it and `SCM_DESC_STR` (`Dolphin/fd1aca3a`) immediately follows
it — so the 32 bytes lie inside the scm_rev literal pool rather than being 32 arbitrary bytes that
happen to be hexadecimal.

**How the script establishes this without assuming the answer.** It does not search for the expected
SHA. It enumerates every `operator new(41)` site in `.text` (four in this binary), disassembles each,
and keeps only those that write bytes 0–15 and 16–31 from `.rodata` loads and bytes 32–39 from a
`movabs` immediate through one base register (two sites qualify — the other builds an unrelated log
message). Of those it keeps the ones whose assembled 40 bytes are lowercase hexadecimal: **exactly
one**. Only then is that value compared with the expected revision, together with the NUL at offset
40, the stored length `0x28`, and the `.rodata` context above. If the instruction-level association
cannot be established, the script fails rather than falling back to a byte search.

The binary additionally carries the related constants, stored as `(pointer, length)` pairs in
`.data.rel.ro` so their boundaries are exact rather than inferred:

| Dolphin constant | Value in the shipped binary | Length field |
|---|---|---|
| `SCM_DESC_STR` (`git describe`) | `fd1aca3a` | 8 |
| `SCM_BRANCH_STR` | `HEAD` (detached checkout) | 4 |
| `SCM_DISTRIBUTOR_STR` | `None` | 4 |
| netplay version string | `fd1aca3a Lin` | 12 |

The commit is titled **"libretro: Add SCM Git revision to log"** and touches
`Source/Core/DolphinLibretro/Boot.cpp` — the change that makes a Dolphin libretro core report its SCM
revision is the very commit the binary reports. The binary is self-consistent with its own provenance
mechanism.

#### The repository: established by libretro's historical build recipe

**A commit id alone does not name a repository.** GitHub resolves the *same commit object* through
either `libretro/dolphin` or `dolphin-emu/dolphin`, because they share a fork network. Repository
identity therefore cannot be inferred from which GitHub path happens to resolve the SHA, and
**absence from upstream Dolphin's current `master` proves nothing** — it reflects only that upstream
`master` has moved on. M10.3 originally argued from that non-membership; **that argument is withdrawn
and is not used here.**

The repository is established instead by libretro's own build recipe, which names the source
repository per core. In `libretro/libretro-super`, `recipes/linux/cores-linux-x64-generic` at
revision `9f56d6248fe83ba1d88df71a7230fde7e1cf2083` (2025-10-15 — the last change to that file
**before** the 2025-11-20 stable 1.22.2 build, so it is the historical recipe in force at build time):

```
dolphin libretro-dolphin https://github.com/libretro/dolphin.git master YES CMAKE Makefile build \
  -DLIBRETRO=ON -DLIBRETRO_STATIC=1 -DENABLE_QT=0 -DCMAKE_BUILD_TYPE=Release \
  -DENABLE_ANALYTICS=OFF -DENABLE_LTO=ON
```

The recipe names the repository and the build flags; it names a **branch**, not a revision, and is
used here only for repository identity. This is consistent with §1's rule that a recipe is not
*revision* provenance — the revision comes from the binary, and only the repository comes from the
recipe.

#### The resulting chain

```
Release 002 Dolphin binary (sha256 c28dc9a2…, build-id 0c693b78…)
  → embedded full SCM_REV_STR emitted by Dolphin's own build from `git rev-parse HEAD`
    → fd1aca3af7db75504ed7512406d8a4cf4187110a
      → historical libretro-super recipe identifies libretro/dolphin as the repository used
        → that commit's tree, plus its gitlink pins, defines the source checkout
```

No step depends on a date, a product version string, or a branch head.

Independent corroboration (**corroboration only — not the basis of the claim**): libretro CI pipeline
`27084` first built this revision at 2025-11-19T15:04:35Z and pipeline `27186` rebuilt it at
2025-11-19T17:17:36Z; the stable bundle was published 2025-11-20 02:50.

#### Scope limit: this proves the top-level revision, not full submodule closure

What is proven is the **top-level Dolphin source revision**. The commit's tree records **33 gitlink
(submodule) entries**, so a recursive checkout at that revision yields a determinate source set, and
those pins are recorded in `docs/research/m10-3/dolphin-submodules-fd1aca3a.txt`.

That the shipped binary was linked from exactly that submodule set is **consistent with** the CI
configuration (`GIT_SUBMODULE_STRATEGY: recursive`) but is **not independently verified by M10.3**:
the verification script checks the top-level revision only, because the submodule revisions are not
recoverable from the binary. Full corresponding-source closure for Dolphin therefore still requires
materialising and archiving that checkout (§7), and must not be reported as already achieved.

**Corresponding source for Dolphin therefore also requires its submodules.** `.gitlab-ci.yml` sets
`GIT_SUBMODULE_STRATEGY: recursive`, and the tree at that commit contains **33 submodule gitlinks**.
Crucially, those revisions are *determined by* the recovered commit, so the corresponding source is
fully specified by it. The pinned set is recorded in [§7.2](#72-what-a-production-build-record-must-contain)
and reproduced in full in `docs/research/m10-3/dolphin-submodules-fd1aca3a.txt`.

### 3.2 Nestopia, bsnes-mercury Balanced, Beetle PSX — **exact revision unknown; candidate identified**

For all three, the status is precisely:

> **Exact source revision unknown; a specific public-CI candidate revision has been identified and
> awaits libretro confirmation.**

The *repository* for each is established the same way as Dolphin's — from the historical
`libretro-super` recipe at `9f56d6248fe83ba1d88df71a7230fde7e1cf2083`, which also records the build
variant actually shipped:

```
nestopia      … https://github.com/libretro/nestopia.git            master YES GENERIC Makefile libretro
bsnes_mercury … https://github.com/libretro/bsnes-mercury.git       master YES GENERIC Makefile . | … bsnes_mercury_balanced:profile=balanced …
mednafen_psx  … https://github.com/libretro/beetle-psx-libretro.git master YES GENERIC Makefile . HAVE_LIGHTREC=1
```

Repository identity is therefore **not** the gap. The gap is the revision.


**No embedded revision was identified by the inspection performed.** That is the finding, and it is
deliberately narrower than "these binaries embed no revision" — no universal negative is asserted,
because none was proved. An inspection that finds nothing does not establish that nothing is there.

What was actually checked, and holds:

- **The specific public-CI candidate revision below was not found in its binary as an embedded
  revision identifier** — not as the full 40 characters, not as a NUL-delimited 7- or 8-character
  `git describe` prefix, and not in `g<hex>` form. This is a narrow, falsifiable negative, and the
  verification script re-asserts it for all three cores.
- No `scm_rev` / `git_commit` / `git_version` symbol or string was found by the inspection.
- Every candidate 40-character hexadecimal run that the inspection surfaced was examined
  individually and rejected for a stated reason (below).
- All three are `stripped`, which the public CI template explains: `STRIP_CORE_LIB: 1` runs
  `strip --strip-unneeded` on every core before upload. This is a plausible *mechanism* for the
  absence, not proof of it.

**The exact source revision of each of these three cores therefore remains UNKNOWN**, and that
conclusion rests on the absence of any identified revision, not on a proof that none exists.

Near-misses were investigated and **rejected**, precisely because they are the kind of thing that
produces a false provenance claim:

- `bsnes_mercury_balanced_libretro.so` contains the byte sequence `0f35d04`, which is a 7-character
  prefix of its candidate revision `0f35d044…`. It occurs in **non-string binary data** with no
  surrounding NUL-delimited text. It is a coincidence and is **not** treated as evidence.
- The same binary contains `LR3590210Register` and similar. This is C++ symbol mangling for the
  Sharp **LR35902** CPU, not a hexadecimal revision.
- `bsnes_mercury_balanced_libretro.so` also contains the absolute path
  `/home/alcaro/Desktop/minir/cores/bsnes_v073/supergameboy/libsupergameboy.so`. This is a **string
  constant vendored in the upstream source**, not a build path of this build, and carries no
  provenance for the shipped binary.
- `mednafen_psx_libretro.so` contains four 40-character runs that fall in the ASCII hex range:
  `0000000000111111111122222222223333333333`, `0000011111222223333344444555556666677778`,
  `4444444444555555555666666666677777777778` and `aaaaaaaaabbbbbbbbccccccccddddddddeeeeeee`. Each
  lies inside a ~4 KB **byte-lookup table of ascending values** (`0x01…`, `0x02…`, … `0x61 'a'`,
  `0x62 'b'`, …), not inside any string, and each was inspected in its surrounding table. They are
  data, not identifiers. Note that these runs exist, so a claim that "no 40-character hex run is
  present in these binaries" would be false and is not made here. Nestopia and bsnes-mercury contain
  no such run at all.

  The verification script prints these runs as **diagnostic output only**. It draws no conclusion
  from them, in either direction. An earlier draft excluded them automatically when they had fewer
  than 20 adjacent character transitions; that threshold is a **research filter, not a proof rule** —
  a real Git object id is not logically required to exceed it — so it no longer decides anything, and
  the disposal of each run above is by inspection, recorded here.

Candidate revisions, from libretro's public GitLab CI pipeline records (§4.2):

| Component | Repository | Candidate revision | Commit date | A pipeline that built it before the bundle was published | Grade |
|---|---|---|---|---|---|
| `nestopia` | `libretro/nestopia` | `5deada54077fae87e2873f5ad9ef77e3ab7af5e1` | 2025-11-08 | `27232` @ 2025-11-19T17:18:28Z | Candidate |
| `bsnes-mercury-balanced` | `libretro/bsnes-mercury` | `0f35d044bf2f2b879018a0500e676447e93a1db1` | 2024-10-21 | `27189` @ 2025-11-19T17:17:39Z | Candidate |
| `beetle-psx` | `libretro/beetle-psx-libretro` | `d6383bff89a93e02aad10a586e804829861c3de1` | 2025-11-14 | `27260` @ 2025-11-19T17:19:05Z | Candidate |

The pipeline column names *a* build of that revision, not *the* build that produced the shipped
bytes. Establishing the latter is exactly what is missing.

All three commits exist in both the canonical `git.libretro.com` project and its `github.com/libretro`
mirror. None of the three repositories has any submodule (`.gitmodules` returns 404 at each candidate
revision), so corresponding source for these three is a single repository each — materially simpler
than Dolphin.

**Why these are strong candidates.** Two independent properties, neither of which is date guessing:

1. **They come from primary build records, not from a guess about what was current.** Each is the
   revision a *named, dated, successful CI pipeline* actually built.
2. **They are insensitive to timing uncertainty.** This is the decisive point. Even if the precise
   pipeline that fed the bundle cannot be identified, the answer does not change: each revision was
   the built revision across the *entire* release window. `bsnes-mercury`'s candidate had been
   unchanged since **2024-10-21** — over a year. `nestopia`'s since 2025-11-08, `beetle-psx`'s since
   2025-11-14. Every pipeline from 2025-11-14 to 2025-11-24 built these same three revisions.

**Why they are nonetheless not proven.** One link is missing and it is not a formality: nothing
public binds the *bytes inside the bundle* to a *specific CI job*. RetroFrontier can show that
libretro's CI built revision X around the right time and with a toolchain matching the binary's
fingerprint; it cannot show that the `.so` in `RetroArch_cores.7z` **is** that job's output. The
bundle could in principle have been assembled from a cached, re-run or differently-triggered build.
Absent that binding, asserting these revisions as corresponding source would be asserting a
conclusion the evidence does not carry — and GPL §3 is exactly where that is not acceptable.

### 3.3 Precise per-system status

Stated explicitly because "the core's revision is proven" and "the system may be distributed" are
different claims, and collapsing them is the most likely misreading of this milestone.

| Item | Status after M10.3 |
|---|---|
| **NES / Nestopia** | Exact revision **unknown**; candidate identified, awaiting libretro confirmation. |
| **SNES / bsnes-mercury Balanced** | Exact revision **unknown**; candidate identified, awaiting libretro confirmation. |
| **PlayStation / Beetle PSX** | Exact revision **unknown**; candidate identified, awaiting libretro confirmation. **Additionally** the `GPL-2.0-only` / GPLv3-host separate-work legal gate remains independently open — confirming the revision would not release PlayStation. |
| **GameCube / Dolphin core** | Top-level revision provenance **CLOSED / PROVEN**: `fd1aca3af7db75504ed7512406d8a4cf4187110a`. Complete corresponding-source materialisation/archive **OPEN**: the 33 gitlink pins are determined by that commit but are not recoverable from the binary, and the checkout is not materialised or archived (§3.1, §7.3). |
| **`dolphin-sys` support asset** | **Separate blocker, open.** `source_revision` remains `null`; upstream `https://buildbot.libretro.com/assets/system/Dolphin.zip` is **not version-addressed**; immutable mirroring is still outstanding. M10.3 proved nothing about this component. |
| **GameCube qualification** | **Partially qualified.** Content execution is still **not confirmed** (inherited from M7.5; M10.3 measured nothing). |

**Consequence for Strategy B.** Obtaining the three remaining core revisions from libretro would
**not**, by itself, unblock GameCube public distribution — GameCube's core revision was never the
item Strategy B was needed for, and its two remaining gates (`dolphin-sys`, content execution) are
untouched by any libretro reply. Nor would it release PlayStation, which carries an independent legal
gate. The honest statement is that a successful Strategy B would remove **one** of several gates for
NES and SNES, and would leave every system still short of a distribution decision.

## 4. Public provenance sources inspected

M10.2 examined four sources and found nothing. M10.3 re-checked all four and added five that M10.2
did not examine. The new material is where the recovery came from.

### 4.1 Re-checked from M10.2 — all confirmed negative

| Source | Result |
|---|---|
| Release 002 authenticated manifest | Confirmed: `"source_revision": null` for all four cores; only the aggregate bundle digest is pinned. |
| `buildbot.libretro.com/stable/1.22.2/linux/x86_64/` | Confirmed: exactly three archives (`RetroArch.7z`, `RetroArch_cores.7z`, `RetroArch_Qt.7z`), all dated 2025-11-20. No manifest, no log, no revision record. |
| `libretro/libretro-core-info` `.info` metadata | Confirmed: no revision field exists in the format. |
| Shipped binaries | **Partially overturned.** Negative for three cores; **positive for Dolphin** (§3.1). This is M10.2's one incorrect generalisation. |

### 4.2 New sources examined by M10.3

**`git.libretro.com` — libretro's canonical GitLab, with an unauthenticated REST API.** This is the
single most consequential source M10.2 missed. The `/builds/libretro/dolphin/...` paths embedded in
the Dolphin binary are GitLab CI runner working directories and are what identified it.

| Source | Public? | What it yields |
|---|---|---|
| `/api/v4/projects/libretro/<core>` | **Yes** | All four core projects resolve (ids 132, 67, 122, 31). |
| `/api/v4/projects/<id>/pipelines` | **Yes** | Full dated pipeline history **including the exact commit SHA of every build**. This is the source of §3.2. |
| `/api/v4/projects/<id>/pipelines/<pl>/jobs` | **Yes** | Per-target job records: `libretro-build-linux-x64`, `libretro-build-windows-x64`, `libretro-build-osx-x64`, `libretro-build-osx-arm64`, plus mobile/console targets. |
| Job **artifacts** | **No — do not exist** | Every job reports no artefact. The CI templates set `expire_in: 10 min`. Artefacts are unrecoverable, permanently. |
| Job **traces** (build logs) | **No** | `/api/v4/projects/<id>/jobs/<id>/trace` returns **HTTP 401**. Logs may be retained internally; they are not public. |
| `.gitlab-ci.yml` per core | **Yes** | Full build recipe. A recipe, **not** provenance. |
| `libretro-infrastructure/ci-templates` | **Yes** | The shared build templates: images, compilers, flags, deployment targets. |

**Build recipes recovered (primary source, `libretro-infrastructure/ci-templates`):**

| Target | Image / runner | Compiler | Notes |
|---|---|---|---|
| Linux x86_64 (Makefile cores) | `libretro-build-amd64-ubuntu:xenial-gcc9` | `gcc` 9 | **Matches the `Ubuntu 9.4.0-1ubuntu1~16.04` fingerprint in the three Makefile cores.** |
| Linux x86_64 (Dolphin, CMake) | `libretro-build-amd64-ubuntu:backports` | `/usr/bin/gcc-12` | **Matches Dolphin's `12.4.0-1ubuntu1~18.04` fingerprint.** |
| Windows x86_64 | `libretro-build-mxe-win-cross-cores:gcc11` (Dolphin: `:mingw12`) | `x86_64-w64-mingw32.static-gcc` | **Cross-compiled from Linux via MXE**; no Windows host used. |
| macOS x86_64 | runner tag `macosx` / `mac-apple-silicon` | `clang` | `MACOSX_DEPLOYMENT_TARGET` `10.9` (Makefile) / arch forced via `-DCMAKE_OSX_ARCHITECTURES=x86_64`. Verified with `lipo -info`. |
| macOS arm64 | runner tag `mac-apple-silicon` | `clang` | `LIBRETRO_APPLE_PLATFORM=arm64-apple-macos10.15`; CMake path sets `MACOSX_DEPLOYMENT_TARGET "10.15"`. |

The Linux toolchain match in the first two rows is a real corroboration: the compiler string baked
into the shipped binaries is exactly the compiler in the public CI image for their build path.

**`buildbot.libretro.com` machine-readable indexes.**
`nightly/linux/x86_64/latest/.index-extended` **exists** and lists `date`, a CRC32, and a filename per
core — **no revision**. The equivalent path under `stable/1.22.2/linux/x86_64/` returns **404**: the
stable tree publishes no index at all. This both confirms and sharpens M10.2's finding.

**`libretro/libretro-super` build recipes on GitHub.** `recipes/linux/cores-linux-x64-generic`
records, per core, the source repository and build flags libretro's buildbot uses. The historical
revision `9f56d6248fe83ba1d88df71a7230fde7e1cf2083` (2025-10-15) is the last change to that file
before the 2025-11-20 stable build, and is what establishes **repository identity** for all four
cores (§3.1, §3.2). It names branches, not revisions, so it is not used for revision provenance.

**A note on GitHub fork networks.** GitHub resolves a commit object through *any* repository in a
fork network, so `libretro/dolphin` and `dolphin-emu/dolphin` both return the same commit. Repository
identity therefore **cannot** be derived from GitHub commit resolution, and a commit's absence from
upstream Dolphin's current `master` proves nothing about which repository a build used. M10.3's first
pass drew that inference; it has been withdrawn, and repository identity now rests on the recipe
above.

## 5. Strategy B — public provenance recovery

### 5.1 Acceptance criteria

Strategy B is **closed only** when this entire chain holds for each core:

```
exact Release 002 binary (SHA-256 + GNU build-id, §2)
  → exact libretro build (a named CI job / pipeline id, or an equivalent build record)
    → exact source repository (canonical host and path)
      → exact full 40-character source revision
        → corresponding source obtainable at that revision, including every submodule
```

Strategy B is **not** closed by any of the following, and a maintainer reply containing only these
must be recorded as "still open":

- a build date, or a statement that a build "would have been from master at that time",
- a branch name, or the current head of that branch,
- a `display_version` or product version string,
- a build recipe or CI configuration file,
- a GNU build-id echoed back without a revision,
- a statement that the source "is on GitHub" — availability is not corresponding source,
- confirmation for some cores but not all four.

Additionally, for any core whose corresponding source spans more than one repository or submodule,
the answer must identify what is needed for the *complete* source. For the four cores here that is
tractable and already characterised: Dolphin needs its 33 submodule pins (which its commit
determines); the other three have no submodules.

**Dolphin's *revision* link in that chain is already established** by §3.1, independently of any
libretro reply: the last two steps — exact repository and exact 40-character revision — hold. The
final step, *corresponding source obtainable at that revision including every submodule*, is **not**
discharged: the 33 gitlink pins are determined by the commit but the checkout has not been
materialised or archived (§3.1, §7.3). And the binary→job binding, which Strategy B exists to supply,
is not needed for Dolphin because the revision comes from the bytes rather than from a job record.

So the outreach concerns three cores for revision recovery, and asks libretro only to *confirm*
Dolphin rather than supply it — which conveniently gives the maintainer a built-in correctness check
on their own lookup. It does **not** follow that Dolphin's corresponding-source obligation is
satisfied; that is layer 2 work (see the headline table), and it is open.

### 5.2 Is the request technically well-specified and realistic?

Yes, and materially more so than M10.2 could have judged:

- **Well-specified.** Every binary is identified by SHA-256 *and* GNU build-id, inside a
  digest-pinned, version-addressed bundle. There is no ambiguity about which artefacts are meant.
- **Narrow.** Because §3.2 already names a candidate revision per core, the question is a
  confirmation ("is this the revision?"), not an investigation.
- **Plausibly answerable.** libretro's CI *does* retain per-build commit SHAs — that data is already
  public. What is not public is the binding from a stable bundle member to a job. A maintainer with
  buildbot access is likely to be able to state how the stable bundle was assembled.
- **Honest about the risk.** libretro may simply not retain the bundle-assembly record. Job artefacts
  are definitively gone (10-minute expiry), so no re-verification against original bytes is possible
  from libretro's side either. This is the concrete reason Strategy B may fail, and it is a
  retention question RetroFrontier cannot resolve from outside.

### 5.3 Ready-to-post outreach request

Neutral, technical, and **not** an accusation. libretro is not alleged to have violated anything;
RetroFrontier is establishing corresponding source for binaries **it** may redistribute.

> **Do not send or publish this without explicit product owner instruction.**

---

**Subject: Source revisions for four cores in the stable 1.22.2 Linux x86_64 core bundle**

Hello,

We are packaging a managed RetroArch runtime and want to get our GPL corresponding-source obligations
right before we distribute anything publicly. We redistribute four cores taken from the official
stable bundle, and we would like to record the exact source revision each shipped binary was built
from. This is a question about build records on our side — we are not suggesting anything is wrong
with what libretro publishes.

Artefacts, so there is no ambiguity about which build we mean:

- Release: RetroArch/libretro **stable 1.22.2**, platform **Linux x86_64**
- Bundle: `https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch_cores.7z`
- Bundle SHA-256: `4b7ed8dc97d4bf035fce182c64b5658c7662e2e9e5d42129538afbd4b6096307` (274,237,400 bytes, indexed 2025-11-20 02:50)

The four cores, by SHA-256 and GNU build-id of the `.so` as extracted from that bundle:

| Core file | SHA-256 | GNU build-id |
|---|---|---|
| `nestopia_libretro.so` | `3f1b76f6d8e68c149a3269c314b406d15f806597333466b1f6a0af01afab52c7` | `8f18c1eed82244fe24d89783f7c3c6c7ba31f4ab` |
| `bsnes_mercury_balanced_libretro.so` | `06fe34874cf8fdec00801a2d22c497c477721a23a87a6e7b5cae82dc1770c5be` | `3843d7c2ecdd0ba55f3bda9819437801cc47aa73` |
| `mednafen_psx_libretro.so` | `ffc1c18a1fc41bf1f28cccaaa7e30e6677ec2aeda91c39b2d8f72d3bd4e2e641` | `4a982e5ed3f47f4a0e1635c0e87479f90fa16ec6` |
| `dolphin_libretro.so` | `c28dc9a2207ffed938810abf3e24df23dc39ef58c6a16c036fc2c58c2240ef10` | `0c693b7863fb713d45c41b54e7715111d77da1fb` |

**Main question: which source repository and which full commit SHA produced each of these four
binaries?**

To make this as cheap as possible to answer, here is what we have already worked out from public
information, so you may only need to confirm or correct it.

`dolphin_libretro.so` we believe we have already resolved without needing your records: the binary
embeds the full 40-character `SCM_REV_STR` that Dolphin's build emits from `git rev-parse HEAD`
(stored as 32 bytes in `.rodata` plus the final 8 as an inline immediate, with a `std::string` length
of 40), alongside `SCM_DESC_STR = fd1aca3a` and `SCM_BRANCH_STR = HEAD`. Together with your
`libretro-super` recipe naming `https://github.com/libretro/dolphin.git`, that gives
`libretro/dolphin` @ `fd1aca3af7db75504ed7512406d8a4cf4187110a`. If that looks wrong to you, we would
very much like to know.

For the other three, the binaries carry no revision (they are stripped by the CI template), so we
derived candidates from public pipeline records on `git.libretro.com`:

| Core | Candidate repository | Candidate revision |
|---|---|---|
| `nestopia_libretro.so` | `libretro/nestopia` | `5deada54077fae87e2873f5ad9ef77e3ab7af5e1` |
| `bsnes_mercury_balanced_libretro.so` | `libretro/bsnes-mercury` | `0f35d044bf2f2b879018a0500e676447e93a1db1` |
| `mednafen_psx_libretro.so` | `libretro/beetle-psx-libretro` | `d6383bff89a93e02aad10a586e804829861c3de1` |

We cannot treat those as established, because we can find nothing public that ties a member of the
stable bundle to a specific CI job — and job artefacts expire after 10 minutes, so the original bytes
cannot be re-checked. **A simple "yes, those are the revisions" (or a correction) would fully resolve
this for us.**

If it is easy to answer, it would also help to know whether your build infrastructure retains, for a
stable bundle:

1. per-core source revisions used to assemble the bundle,
2. build timestamps,
3. build logs,
4. CI job or pipeline identifiers,
5. toolchain/container identity for each target.

If that information is retained but not currently published, publishing per-core revisions alongside
future stable bundles would let downstream packagers meet corresponding-source obligations without
having to ask.

Thanks for your time, and for the buildbot.

---

**Suggested channel.** The libretro GitHub organisation's issue tracker for build infrastructure, or
the official libretro forums/Discord buildbot channels. Post in exactly one place first; the request
is a question, not a report, and duplicating it across channels is noise.

### 5.4 Recording the outcome

Outreach is **not** a resolved blocker. Until a usable answer arrives, the corresponding-source item
in `BACKLOG.md` stays open and the redistribution status of all four systems is unchanged. A reply
that does not satisfy §5.1 must be recorded as a failed attempt, not as partial closure.

## 6. Strategy C — self-build feasibility across all four V1 platforms

Assessed per platform rather than extrapolated from Linux. The dominant finding is that libretro's
**public** CI templates (§4.2) constitute a working, primary-source reference build for every one of
the four V1 targets — which removes most of the unknowns M10.2 assumed Strategy C carried.

### 6.1 Per-core build shape

| Core | Canonical repo to build | Build system | Submodules | Generated/vendored source affecting corresponding source |
|---|---|---|---|---|
| `nestopia` | `libretro/nestopia` | Makefile (`MAKEFILE_PATH: libretro`) | **None** | Vendored Nestopia UE core tree; upstream is the same repository. |
| `bsnes-mercury-balanced` | `libretro/bsnes-mercury` | Makefile, `PROFILE=balanced` | **None** | Three profiles from one tree; only `balanced` is shipped. |
| `beetle-psx` | `libretro/beetle-psx-libretro` | Makefile, `CORENAME=mednafen_psx` (software; **not** the `_hw` variant) | **None** | Vendored Mednafen subtree; `libretro_core_options_intl.h` is **generated and committed**, so it is captured by the revision. |
| `dolphin` | `libretro/dolphin` | CMake, `-DLIBRETRO=ON` | **33 recursive gitlinks** | Large `Externals/` tree; several are vendored *and* submoduled. |

Beetle PSX's shipped variant matters: Release 002 ships `mednafen_psx_libretro.so`, the software
renderer, not `mednafen_psx_hw_libretro.so`. A self-build must reproduce that choice exactly.

### 6.2 Four-platform feasibility matrix

| | **Linux x86_64** | **Windows x86_64** | **macOS arm64** | **macOS x86_64** |
|---|---|---|---|---|
| Credible build path | **Yes** — container | **Yes** — MXE cross-compile **from Linux** | **Yes** — native on Apple Silicon | **Yes** — from Apple Silicon, arch forced |
| Reference recipe exists publicly | Yes (`linux-x64.yml`, `linux-cmake.yml`) | Yes (`windows-x64-mingw.yml`, `windows-cmake-mingw.yml`) | Yes (`osx-arm64.yml`, `osx-cmake-arm64.yml`) | Yes (`osx-x64.yml`, `osx-cmake-x86.yml`) |
| Toolchain | GCC 9 (`xenial-gcc9`); GCC 12 (`backports`) for Dolphin | `x86_64-w64-mingw32.static-gcc` (MXE), GCC 11/12 | `clang` | `clang` |
| Host hardware required | Linux only | **Linux only** | **Apple Silicon Mac** | **Apple Silicon Mac** (same host) |
| Upstream/libretro CI exercises it? | Yes, all four cores | Yes, all four cores | Yes, all four cores | Yes, all four cores |
| Native output | `.so` | `.dll` | Native arm64 `.dylib` | Native x86_64 `.dylib` |
| Deployment target | `GLIBC_2.14`/`2.27` floor observed today | n/a | `10.15` | `10.9`–`10.15` depending on template |
| Output verification in reference recipe | `strip --strip-unneeded` | `strip` | `lipo -info`, `otool -l` | `lipo -info` arch assertion |

**The two conclusions that most change Strategy C's cost:**

1. **Three of the four targets appear to need only Linux containers.** Windows is cross-compiled via
   MXE, not built on Windows, so no Windows build host or CI runner is indicated.
2. **One Apple Silicon machine appears to cover both macOS targets.** libretro produces the x86_64
   dylib on `mac-apple-silicon` runners by forcing `-DCMAKE_OSX_ARCHITECTURES=x86_64` /
   `LIBRETRO_APPLE_PLATFORM`.

This is materially cheaper than the "four-platform build and signing pipeline" M10.2 estimated.

> **This section records feasibility, not qualification.** Every row above is evidence that a
> *credible build path exists*, read from libretro's public CI configuration. RetroFrontier has built
> nothing. Specifically:
>
> - **A Windows cross-compilation path identified is not a production Windows build proven.** No
>   `.dll` has been produced or run by RetroFrontier.
> - **One Apple Silicon host plausibly building both architectures is not two qualified production
>   dylibs.** Neither macOS artefact has been produced, loaded, or measured.
> - **macOS core build availability is not signing, notarization or quarantine readiness** (§6.4).
> - **A pinned-source build is not a reproducible build** (§6.3).
> - No managed launch has been measured on any platform other than Linux x86_64, and M10.3 measured
>   nothing at all. No qualification status changes.
>
> The prototype in §10 exists precisely to convert one cell of this matrix from *feasible* to
> *measured* before the remaining fifteen are relied on.

### 6.3 Reproducibility — **pinned, not reproducible**

Stated deliberately, because conflating these two is the most likely way this document could be
misread.

**RetroFrontier can build from a pinned revision. That is not a reproducible build.** A pinned build
guarantees *we know what source went in*. A reproducible build guarantees *anyone re-running it gets
byte-identical output*. Only the first is on offer here, and only the first should ever be claimed.

Evidence that byte-for-byte reproducibility is **not** currently established:

- No byte-for-byte rebuild has been attempted or demonstrated by this milestone. Under this
  document's own standard, that alone forbids the claim.
- The observed binaries carry GNU build-ids, which are hashes of linked output and are sensitive to
  toolchain, link order and paths.
- The Dolphin binary embeds absolute builder paths (`/builds/libretro/dolphin/...`), so its output is
  path-dependent unless `-ffile-prefix-map` is applied.
- libretro's own templates use `ccache` and `git-restore-mtime` — aids to build *stability*, not
  proof of reproducibility.

Reproducible builds are a worthwhile later goal and would strengthen the provenance story
considerably. They are **out of scope for M10.3 and must not be asserted** in notices, release
metadata or marketing. Where a Runtime Release records provenance, the correct phrasing is
"built from pinned revision <sha>", never "reproducible build".

### 6.4 macOS signing and notarization — explicitly out of scope

Strategy C makes RetroFrontier the **producer** of macOS core artefacts. It does **not** make them
signed or notarized. Producing a native `arm64` `.dylib` is a build question; Developer ID signing,
notarization, stapling and Gatekeeper quarantine handling are separate packaging and security work,
already tracked in `BACKLOG.md` under M2/M10. Being able to build a dylib must never be reported as
macOS release readiness.

The interaction that *is* real and worth recording now: RetroArch loads cores with `dlopen`, so on
macOS each core dylib is subject to code-signing policy in its own right. Whether cores are signed
individually, or covered by the app bundle's signature and hardened-runtime entitlements, is a
decision the packaging milestone must make. Strategy C does not settle it, but it does make it
tractable, because RetroFrontier would control the artefacts at signing time rather than embedding
third-party binaries it did not produce.

## 7. Source archive and build-record model for Strategy C

### 7.1 Constraint: trust semantics are unchanged

The archive model adds provenance **alongside** the existing trust chain. It does not modify it.

- TUF metadata, key policy, threshold and the Runtime inventory target (M10.1) are untouched.
- Every component remains digest-pinned and authenticated exactly as today; a self-built core is
  pinned by `artifact_sha256` in the same way a downloaded one is.
- `source_revision` becomes a real 40-character commit id instead of `null`. It is **provenance
  metadata, not a trust anchor**: the client must keep authenticating bytes by digest and signature,
  never by revision.
- No new client-side verification step, and no new failure mode in activation or rollback.

### 7.2 What a production build record must contain

One record per **(core, platform, architecture)**, published with the release and archived
immutably:

| Field | Content | Why |
|---|---|---|
| `component_id` | e.g. `dolphin` | Ties to the Runtime Release component. |
| `source_repository` | Canonical URL (`git.libretro.com` project, GitHub mirror recorded separately) | Corresponding source location. |
| `source_revision` | **Full 40-character** commit SHA | The obligation. Never abbreviated. |
| `submodule_revisions` | Path → 40-char SHA for every gitlink, recursively | Dolphin needs 33; the other three need none. Without these the source is incomplete. |
| `patches` | Ordered list with digests, or explicitly **none** | RetroFrontier applies none today; that must be *stated*, not implied. |
| `build_scripts` | Digest of the exact build script/recipe used | Distinguishes recipe from record. |
| `toolchain_identity` | Container image digest, or host OS + compiler version | Must be a **digest**, not a tag: `xenial-gcc9` is mutable. |
| `build_flags` | Full effective flags (e.g. `-DLIBRETRO=ON`, `PROFILE=balanced`, `HAVE_HW=0`) | Beetle PSX and bsnes-mercury ship one of several variants from one tree. |
| `source_archive` | Digest + immutable location of the archived source at that revision, **including submodules** | Satisfies the offer without depending on a third-party host staying up. |
| `licence_and_notices` | Licence text + copyright notices for the core and every vendored/submoduled dependency | Dolphin's `Externals/` makes this non-trivial. |
| `output_digest` | SHA-256 of the produced artefact | Binds the record to the shipped bytes. |
| `output_build_id` | GNU build-id (ELF) / equivalent | Secondary binary identity. |
| `platform` / `architecture` | e.g. `macos` / `arm64` | Records are per-target and must never be generalised. |
| `provenance_manifest` | Signed aggregate of the above | Published with the release. |
| `reproducibility_status` | `pinned` \| `reproducible` | Defaults to `pinned`. Only ever `reproducible` on demonstrated byte-for-byte evidence. |

### 7.3 Archiving obligation

Mirroring the *source* is what actually discharges the obligation, and it must not depend on GitHub
or `git.libretro.com` remaining available. For each build: archive a source tarball at the exact
revision with all submodules materialised, store its digest, and keep it for as long as the binary is
distributed plus the offer period. This is the same immutability principle that forced Release 001 →
002, applied to source instead of binaries.

This also subsumes M10.2 §4's separate `dolphin-sys` problem: the non-version-addressed
`https://buildbot.libretro.com/assets/system/Dolphin.zip` must be mirrored into RetroFrontier-controlled
immutable storage regardless of which strategy is chosen. That item is **not** closed by M10.3.

## 8. Interaction with the macOS blocker

M10.2 established that libretro publishes **no immutable stable macOS core bundle at any
architecture** — only rolling `nightly/apple/osx/{arm64,x86_64}/latest/` per-core archives, which
ADR-004 rejects.

**Strategy C solves this blocker; Strategy B cannot.**

- The blocker is about *acquisition*: there is no immutable macOS artefact to pin. Strategy B
  supplies *revisions*, which does nothing about acquisition. Even a complete, perfect libretro reply
  leaves macOS exactly as blocked as it is today.
- Under Strategy C, RetroFrontier *produces* the macOS artefacts. Immutability then follows from
  RetroFrontier's own release process: the dylib is built from a pinned revision, digest-pinned into
  a Runtime Release, and authenticated by the existing TUF chain. The rolling-nightly problem
  disappears because the rolling nightly is no longer an input.
- §6.2 shows this is achievable with **one Apple Silicon machine** covering both macOS
  architectures.

Two things this explicitly does **not** claim:

1. It does not make macOS *signed or notarized* (§6.4). Producing artefacts is not release readiness.
2. It does not qualify macOS. No managed launch has been measured on macOS, and
   [`docs/CORE_MATRIX.md`](CORE_MATRIX.md) qualification statuses are unchanged.

## 9. Other M10.2 blockers — unchanged

M10.3 solves none of these, and none is closed by assertion. All remain open exactly as recorded:

| Blocker | Status after M10.3 |
|---|---|
| Mega Drive has no approvable core (non-commercial licence terms) | **Open.** Untouched. |
| Nintendo 64 licence identity conflict | **Open.** Untouched. |
| Beetle PSX `GPL-2.0-only` vs GPLv3-host separate-work review | **Open.** Note this gates PlayStation *independently* of corresponding source; recovering Beetle PSX's revision would not release PlayStation. |
| PlayStation / Saturn `.iso` catalog mismatch | **Open.** Untouched. |
| Dreamcast BIOS layout (`dc/` subdirectory) | **Open.** Untouched. |
| Production TUF key ceremony under independent custody | **Open.** Untouched. |
| Public hosting / mirroring, incl. `dolphin-sys` immutable mirror | **Open.** §7.3 restates the requirement. |
| Application updater | **Open.** Untouched. |
| Windows and macOS packaging, signing, notarization | **Open.** Untouched (§6.4). |
| Final clean-machine qualification | **Open.** M10.3 measured nothing. |

## 10. Recommendation — **B-then-C**

**Send the precise provenance request now, and in parallel begin only the low-regret Strategy C
foundations.** Do not stand up the full self-build pipeline yet, and do not wait idle on libretro.

**Why B first, and why it is now genuinely cheap.** M10.2 rated Strategy B a plausible long shot.
M10.3 changes that: the ask is three yes/no confirmations against named candidate revisions, with
Dolphin already resolved and serving as a correctness check on the maintainer's own lookup. The
outreach costs one message.

**What a fully successful Strategy B would and would not achieve**, stated precisely (§3.3):

- It would establish the corresponding-source *revision* for the three remaining cores on Linux
  x86_64. That is one gate, removed, for three cores.
- It would **not** unblock **GameCube** public distribution. GameCube's core revision is already
  proven and was never what Strategy B was for; its open gates are the `dolphin-sys`
  provenance/immutability blocker and unconfirmed content execution, neither of which any libretro
  reply touches.
- It would **not** release **PlayStation**, which carries an independent `GPL-2.0-only` /
  GPLv3-host legal gate.
- It would **not** by itself clear **NES** or **SNES** for distribution either: source archiving,
  notices, a written offer, immutable mirroring, hosting and the production key ceremony all remain
  open regardless.

So the honest case for B is narrower than "it unblocks systems": it removes the single gate that is
currently *cheapest to remove*, on the one platform that is actually qualified today.

**Why C must start anyway, and not after.** Strategy B has a hard ceiling that no reply can raise:

- it **cannot** close macOS, which is an acquisition problem, not a provenance one (§8),
- it **cannot** close Mega Drive,
- it leaves RetroFrontier shipping binaries it cannot rebuild, permanently dependent on a third
  party's retention — and libretro's artefacts are already provably gone after 10 minutes,
- it may simply fail, and RetroFrontier cannot influence whether it does.

**The tradeoff, stated plainly.** B is cheap, fast and high-value but externally dependent and
ceiling-limited. C is self-sufficient and closes both the corresponding-source and macOS blockers by
construction, but costs a maintained build pipeline, one Apple Silicon machine, and the review burden
of distributing binaries RetroFrontier compiled. Choosing B-then-C buys the chance of a fast partial
win without betting the milestone on a third party's answer. The risk being accepted is a modest
amount of duplicated effort if libretro answers immediately and completely.

**C-now was seriously considered and rejected**, on the ground that B's cost has fallen far enough
(one message, candidates already identified) that skipping it would discard a cheap, real chance of
removing the corresponding-source gate for three cores on the one platform that is actually
qualified today.

### Low-regret Strategy C foundations to start now

Useful under *either* outcome, and none of them creates a Runtime Release or touches trust code:

1. Record the recovered Dolphin top-level revision and the 33 gitlink pins its tree declares
   (**done** by this milestone). Note that materialising and archiving that checkout — which is what
   actually closes Dolphin's corresponding source — is item 2, not item 1.
2. Archive source at the four identified revisions into RetroFrontier-controlled immutable storage —
   valuable whether the revision is later confirmed by libretro or replaced by a self-build pin.
3. Mirror the non-version-addressed `dolphin-sys` asset (already an open M10.2 item, §7.3).
4. Specify the build-record schema (§7.2) and how `source_revision` is populated, without changing
   the release format yet.
5. Prototype exactly **one** core on **one** platform — Nestopia on Linux x86_64, the simplest
   Makefile core with no submodules — to measure real cost before committing to four cores × four
   platforms.

Explicitly **not** started now: the macOS runner, the Windows MXE pipeline, signing infrastructure,
or any change to Runtime Release construction.

## 11. What is *not* claimed

- **No GPL compliance claim is made.** No core has complete corresponding source materialised,
  archived and published — **Dolphin included**. Public redistribution remains blocked.
- **The corresponding-source blocker is NOT closed for any core.** What is closed is one layer for
  one core: Dolphin's **top-level revision provenance**. The three other cores' revisions are open,
  and complete corresponding-source materialisation is open for all four. The blocker item stays
  open.
- **Proving a revision is not producing corresponding source.** Dolphin's 33 submodule pins are
  determined by the proven commit, but the checkout has not been materialised, archived or
  accompanied by notices, and none of that is claimed as done.
- **Candidate revisions are not corresponding source.** Nestopia, bsnes-mercury and Beetle PSX have
  named candidates, and a candidate must never be written into `source_revision` or a notice file.
- **No public distribution has occurred**, and none is authorised by this document.
- **Outreach is not resolution.** The request in §5.3 is unsent, and an unanswered request closes
  nothing.
- **"Pinned" is not "reproducible"** (§6.3). No byte-for-byte reproducibility has been demonstrated.
- **Building for macOS is not macOS readiness** (§6.4, §8). Signing, notarization and qualification
  are separate and open.
- **A GNU build-id is not a source revision.** It is recorded only to let libretro identify a build.
- **A build recipe is not build provenance.** The `.gitlab-ci.yml` files describe how builds run, not
  that a particular build produced a particular artefact.
- **Nothing here measured anything.** No managed launch was run; no qualification status changed.
- **No legal conclusion is asserted.** Statuses are engineering decisions under current policy.

## 12. Evidence

Primary sources, retrieved 2026-09-05. No community lists, wikis or forum inference were used.

**Local, authenticated:**

- `release/linux-x86_64/runtime-release.json` at `main` `f280905`.
- The authenticated active installation `rf-runtime-linux-x86_64-002`
  (`i-18d14638042bd789-1-51189`), per `runtime/active.json`.
- Direct ELF inspection of the four installed core binaries: `sha256sum`, `readelf -n` (build-id),
  `readelf -p .comment` (toolchain), `readelf -d`/`-V` (ABI floor), and byte-exact NUL-delimited
  `.rodata`/`.data.rel.ro` string extraction.

**libretro GitLab (`git.libretro.com`), unauthenticated REST API v4:**

- Projects `libretro/dolphin` (132), `libretro/nestopia` (67), `libretro/bsnes-mercury` (122),
  `libretro/beetle-psx-libretro` (31).
- Pipeline histories 2025-11-10 → 2025-11-25 for all four projects, and job listings for pipelines
  27186, 27189, 27232, 27260.
- Job trace endpoint: HTTP 401 (not public).
- `.gitlab-ci.yml` for all four cores at their candidate/recovered revisions.
- `libretro-infrastructure/ci-templates`: `linux-x64.yml`, `linux-cmake.yml`, `windows-x64-mingw.yml`,
  `osx-x64.yml`, `osx-arm64.yml`, `osx-cmake-arm64.yml`.

**GitHub:**

- `libretro/dolphin` commit resolution for `fd1aca3af7db75504ed7512406d8a4cf4187110a`.
- `libretro/libretro-super` `recipes/linux/cores-linux-x64-generic`, both at current `master` and at
  the historical revision `9f56d6248fe83ba1d88df71a7230fde7e1cf2083` (2025-10-15), with the four
  core lines confirmed identical between them.
- `objdump -d` of the `GetScmRevGitStr()` construction site in the shipped Dolphin binary, and the
  `.rodata` bytes at `0x10f17c0`, to reassemble the full 40-character `SCM_REV_STR`.
- `.gitmodules` and recursive tree (submodule gitlinks) at
  `fd1aca3af7db75504ed7512406d8a4cf4187110a`.
- Mirror existence checks for the three candidate revisions.

**libretro buildbot:**

- `stable/1.22.2/`, `stable/1.22.2/linux/x86_64/` indexes.
- `nightly/linux/x86_64/latest/.index-extended` (exists; date + CRC32 + filename, no revision).
- `stable/1.22.2/linux/x86_64/.index-extended` (HTTP 404).

**Re-verification.** `docs/research/m10-3/verify-core-provenance.sh` re-derives, from a local
installation and without network access:

- the §2 binary identity table (SHA-256 and GNU build-id for all four cores);
- the §3.1 Dolphin revision, at instruction level — it enumerates the `operator new(41)` sites in
  `.text`, keeps the single one that assembles 40 lowercase hex characters from its own two
  `.rodata` loads and its own `movabs` immediate through one base register, and checks that value,
  the NUL at offset 40, the stored length `0x28`, and the surrounding scm_rev literal context. It
  never searches the file for the expected SHA, and it fails rather than degrading to a byte search
  if the instruction-level association cannot be established;
- the §3.2 narrow negative for the other three cores: each documented candidate revision is absent
  from its binary as an embedded revision identifier.

Its generic 40-character hex scan is printed as **diagnostic output only** and establishes no
provenance conclusion. The script asserts nothing about submodule closure, corresponding-source
materialisation, or redistribution.
