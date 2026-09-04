# Core Build Provenance and Build Strategy

Authoritative M10.3 record: public provenance recovery for the four Release 002 core binaries, the
libretro outreach payload, Strategy B acceptance criteria, Strategy C four-platform feasibility, the
self-build archive model, and the resulting recommendation.

Companion documents:

- [`docs/SOURCE_PROVENANCE.md`](SOURCE_PROVENANCE.md) — M10.2 licence, redistribution and
  corresponding-source closure. M10.3 **narrows** its headline finding; it does not overturn it.
- [`docs/CORE_MATRIX.md`](CORE_MATRIX.md) — policy, availability, implementation, qualification.
- [`docs/RUNTIME_MANAGER.md`](RUNTIME_MANAGER.md), [`docs/adr/ADR-012-runtime-trust-model.md`](adr/ADR-012-runtime-trust-model.md)
  — the trust semantics this milestone must not weaken.

**Scope.** Research and design only. M10.3 created no Runtime Release, approved no core, changed no
trust semantics, and left Release 002 byte-identical. It is not legal advice.

## Headline finding

> M10.2 reported that the source revision of all four Release 002 cores was "not recoverable from
> public sources". That is now **false for one core and materially overstated for the other three.**
>
> - **Dolphin's exact source revision is recovered and proven** from the distributed binary itself:
>   `libretro/dolphin` @ `fd1aca3af7db75504ed7512406d8a4cf4187110a`. The binary carries its own
>   build-system-generated SCM constants. No inference from dates or version strings is involved.
> - **Nestopia, bsnes-mercury Balanced and Beetle PSX remain unproven**, but are no longer unknown:
>   each has a single, named, high-confidence **candidate revision** derived from libretro's
>   *public* GitLab CI pipeline records, which M10.2 did not examine.
> - The missing link for those three is narrow and specific: nothing public binds the **bundle member
>   bytes** to a **specific CI job**. libretro's CI destroys build artefacts after **10 minutes**, and
>   job logs require authentication.

The operative consequence is unchanged: **RetroFrontier still cannot satisfy GPL corresponding-source
obligations for three of the four cores, so public redistribution of the managed runtime remains
blocked.** What changed is the *shape* of the remaining gap, and therefore the cost of closing it.
The outreach to libretro is no longer "please excavate unknown information" but "please confirm four
specific, already-identified revisions and their job binding" — a far cheaper and far more answerable
request.

## 1. Proof standard used in this document

M10.3 applies the standard the milestone set, deliberately and strictly:

> A revision is **recovered** only when there is an authenticated or otherwise unambiguous chain from
> the exact distributed binary to that exact source revision.

Three grades are used, and they are never conflated:

| Grade | Meaning | Admitted evidence |
|---|---|---|
| **Proven** | The chain runs from the distributed bytes themselves to one revision, with no unproven step. | Revision constants emitted by the source's own build system into the shipped binary; unique resolution of that identifier in the correct repository. |
| **Candidate (high confidence)** | A single named revision is strongly indicated by primary build-infrastructure records, but one link in the chain is not publicly verifiable. | Public CI pipeline records; toolchain fingerprints matching a specific public CI image; stability of the revision across the whole release window. |
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

This is a genuine, checkable link from the shipped bytes to a **specific public libretro CI image**
(§4.2). It authenticates *who built the binary and with what*. It does **not** identify a source
revision, and is not used as one.

## 3. Per-core recovery result

### 3.1 Dolphin — **PROVEN**

**Exact source revision: `libretro/dolphin` @ `fd1aca3af7db75504ed7512406d8a4cf4187110a`.**

The Dolphin binary carries revision constants that Dolphin's own CMake build emits into
`scm_rev.h`. These are stored as `(pointer, length)` pairs in `.data.rel.ro`, so their boundaries are
exact rather than inferred from `strings` heuristics:

| Dolphin constant | Value in the shipped binary | Length field |
|---|---|---|
| `SCM_DESC_STR` (`git describe`) | `fd1aca3a` | 8 |
| `SCM_BRANCH_STR` | `HEAD` (detached checkout) | 4 |
| `SCM_DISTRIBUTOR_STR` | `None` | 4 |
| netplay version string | `fd1aca3a Lin` | 12 |

A `.rodata` literal pool additionally contains the string `Dolphin [HEAD] ` immediately followed by
a 33-character hex run whose final 32 characters, `fd1aca3af7db75504ed7512406d8a4cf`, are a
**32-character prefix** of the full revision.

Resolution and verification, all against primary sources:

1. `fd1aca3a` and the 32-character prefix both resolve, in `libretro/dolphin`, to exactly
   `fd1aca3af7db75504ed7512406d8a4cf4187110a`. A 32-hex prefix is a 128-bit constraint; ambiguity is
   not a live concern.
2. That commit is an **ancestor of `libretro/dolphin`'s `master`** (`compare master...<sha>` reports
   `status: behind`, `ahead_by: 0`), so it genuinely belongs to the libretro fork's history.
3. It is **not** an ancestor of `dolphin-emu/dolphin`'s `master` (`status: diverged`, `ahead_by: 213`),
   confirming the fork — not upstream Dolphin — is the correct corresponding-source repository.
4. The commit is titled **"libretro: Add SCM Git revision to log"** and touches
   `Source/Core/DolphinLibretro/Boot.cpp`. The change that causes a Dolphin libretro core to carry
   its SCM revision is the very commit the binary reports. The binary is self-consistent with its
   own provenance mechanism.

Independent corroboration (**corroboration only — not the basis of the claim**): libretro CI pipeline
`27084` first built this revision at 2025-11-19T15:04:35Z and pipeline `27186` rebuilt it at
2025-11-19T17:17:36Z; the stable bundle was published 2025-11-20 02:50.

**Why this meets the standard.** The chain is: distributed bytes → constants that Dolphin's build
system wrote into those bytes from its own source tree → unique commit in the correct repository. No
step depends on a date, a version string, or a branch head.

**Corresponding source for Dolphin therefore also requires its submodules.** `.gitlab-ci.yml` sets
`GIT_SUBMODULE_STRATEGY: recursive`, and the tree at that commit contains **33 submodule gitlinks**.
Crucially, those revisions are *determined by* the recovered commit, so the corresponding source is
fully specified by it. The pinned set is recorded in [§7.2](#72-what-a-production-build-record-must-contain)
and reproduced in full in `docs/research/m10-3/dolphin-submodules-fd1aca3a.txt`.

### 3.2 Nestopia, bsnes-mercury Balanced, Beetle PSX — **CANDIDATE, NOT PROVEN**

None of these three binaries embeds any revision. This was checked positively, not assumed:

- no 40-character hex run anywhere in any of the three (the only 40-char runs found binary-wide are
  decimal digit tables),
- no `git describe`-shaped `g<hex>` identifier,
- no `scm_rev` / `git_commit` / `git_version` symbol or string,
- all three are `stripped`, which the public CI template explains: `STRIP_CORE_LIB: 1` runs
  `strip --strip-unneeded` on every core before upload.

Two near-misses were investigated and **rejected**, precisely because they are the kind of thing that
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
- `mednafen_psx_libretro.so` contains four 40-character runs that fall in the ASCII hex range, such
  as `aaaaaaaaabbbbbbbbccccccccddddddddeeeeeee`. Each lies inside a ~4 KB **byte-lookup table of
  ascending values** (`0x01…`, `0x02…`, … `0x61 'a'`, `0x62 'b'`, …), not inside any string. They are
  data, not identifiers. The verification script excludes such runs on a stated mechanical criterion
  — fewer than 20 adjacent character transitions — rather than by hand, so the exclusion is
  reproducible and cannot quietly hide a real revision.

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

**Fork-network verification on GitHub** was used to establish repository membership for the Dolphin
commit (§3.1), because GitHub's API resolves any commit in a fork network and a naive lookup would
have wrongly suggested upstream `dolphin-emu/dolphin` as the corresponding-source repository.

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

**Dolphin is already closed under this standard** by §3.1, independently of any libretro reply. The
outreach therefore concerns three cores, and asks libretro to *confirm* Dolphin rather than supply it
— which conveniently gives the maintainer a built-in correctness check on their own lookup.

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
embeds Dolphin's own `scm_rev` constants (`SCM_DESC_STR = fd1aca3a`, `SCM_BRANCH_STR = HEAD`), and a
32-character prefix of the revision appears in `.rodata`. That resolves to
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

1. **Three of the four targets need only Linux containers.** Windows is cross-compiled via MXE, not
   built on Windows. No Windows build host, and no Windows CI runner, is required to produce cores.
2. **One Apple Silicon machine covers both macOS targets.** libretro produces the x86_64 dylib on
   `mac-apple-silicon` runners by forcing `-DCMAKE_OSX_ARCHITECTURES=x86_64` /
   `LIBRETRO_APPLE_PLATFORM`. A single Mac is the entire macOS hardware requirement.

This is materially cheaper than the "four-platform build and signing pipeline" M10.2 estimated. The
remaining genuine costs are ongoing maintenance, one Mac, and RetroFrontier becoming the distributor
of binaries it compiled.

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
outreach costs one message. If it succeeds, the corresponding-source blocker closes for Linux
x86_64 — which unblocks NES, SNES and GameCube redistribution far sooner than any build pipeline
could, and PlayStation still waits on its separate legal gate.

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
unblocking three systems on the platform that is actually qualified today.

### Low-regret Strategy C foundations to start now

Useful under *either* outcome, and none of them creates a Runtime Release or touches trust code:

1. Record the recovered Dolphin revision and its 33 submodule pins as durable provenance (**done** by
   this milestone).
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

- **No GPL compliance claim is made.** Three of four cores still lack established corresponding
  source, so public redistribution remains blocked.
- **The corresponding-source blocker is NOT closed.** It is closed for Dolphin's revision
  specifically; it is open for the other three, and the blocker item stays open.
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

- `libretro/dolphin` commit resolution and fork-network comparison against `dolphin-emu/dolphin`.
- `.gitmodules` and recursive tree (submodule gitlinks) at
  `fd1aca3af7db75504ed7512406d8a4cf4187110a`.
- Mirror existence checks for the three candidate revisions.

**libretro buildbot:**

- `stable/1.22.2/`, `stable/1.22.2/linux/x86_64/` indexes.
- `nightly/linux/x86_64/latest/.index-extended` (exists; date + CRC32 + filename, no revision).
- `stable/1.22.2/linux/x86_64/.index-extended` (HTTP 404).

**Re-verification.** `docs/research/m10-3/verify-core-provenance.sh` re-derives the §2 binary
identity table and the Dolphin embedded-revision evidence from a local installation, so the central
claim of this document can be independently re-checked without network access.
