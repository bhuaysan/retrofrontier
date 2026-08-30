# M7.5 — Real Managed RetroArch Runtime and Linux Qualification

## Purpose

M2 built the managed runtime lifecycle. M7 built the managed launch path. Neither produced a real
Runtime Release, so on any ordinary development installation the product answered `RUNTIME NOT
READY` and no game could ever start.

M7.5 connects the two halves with **real RetroArch and real libretro cores on Linux x86_64**,
without weakening the architecture. It adds no shortcut: no system RetroArch, no `PATH` lookup, no
user-chosen executable, no arbitrary core path, no unverified runtime marked ready, no manifest
bypass, and no substitution of SHA-256-only trust for release authentication.

Starting `main`: `73eb1af024df393c4539be35ab22816b39a0242f` (`feat: complete M7 RetroArch launch (#16)`).

## Root cause of `RUNTIME NOT READY`

Four things were missing, in this order of severity.

1. **No configured trusted release source.** `RuntimeManager::for_app` always installed
   `UnavailableTrustedReleaseSource`, whose only behaviour is to fail with
   *"no approved managed runtime source is configured"*. `ToughTrustedReleaseSource` existed and
   worked, but nothing ever constructed one. Installation was therefore impossible by construction.
2. **No Runtime Release existed.** There was no release definition, no authenticated manifest, no
   inventory, and no published TUF repository for any platform.
3. **No installation surface.** `RuntimeManager::install` / `repair` were fully implemented but
   reachable from neither an application service nor an IPC command, and Settings had no runtime
   section at all. `getRuntimeStatus` existed in `src/platform/ipc.ts` and was never called.
4. **A real-artefact extraction defect.** `find_squashfs_offset` accepted the first `hsqs` byte
   sequence in an AppImage. The official RetroArch AppImage runtime embeds a literal
   `hsqs`/`sqsh`/`shsq`/`qshs` signature table at offset 194183, while the real SquashFS superblock
   begins at 944632. Every real AppImage would have failed extraction. Fixed by validating the
   SquashFS 4.0 superblock (version, block size against block log, compressor id, and declared
   `bytes_used` against the remaining artefact length) at each candidate offset.

## The Runtime Release

Release id `rf-runtime-1.22.2-linux-x86_64-001`, sequence 1, channel `stable`, platform
`linux`/`x86_64`. Defined declaratively in
[`release/linux-x86_64/runtime-release.json`](../release/linux-x86_64/runtime-release.json).

### RetroArch

| | |
| --- | --- |
| Upstream | Official libretro stable buildbot |
| Source | `https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch.7z` |
| Version | 1.22.2, Git `69a4f0e`, built 2025-11-20 |
| Upstream SHA-256 | `7d62da9a21397d6e1b9490785cedbeafd262781b50115076736fbe8a77ef30e9` (179 361 448 bytes) |
| Component artefact | `RetroArch-Linux-x86_64/RetroArch-Linux-x86_64.AppImage`, lifted out of the 7z |
| Artefact SHA-256 | `794b0f65d4efa918e2ad05cac34b444a4f3207ed6c74834b7c14eb5fb15e1cc4` (10 390 008 bytes) |
| Licence | GPL-3.0-only |
| Install path | `runtime/retroarch` |
| AppRun | `runtime/retroarch/AppRun`, a symlink to `usr/bin/retroarch` |

The 7z also contains a 380 MB `…AppImage.home` portable-home asset tree. RetroFrontier controls
every RetroArch path explicitly and never uses a portable home, so that tree is deliberately not
part of the release. The AppImage is extracted from the container rather than redistributed inside
it, which keeps the target small and keeps extraction to one reviewed code path.

The extracted AppDir matches the M0.1 assumptions: `usr/bin/retroarch` has
`RUNPATH=$ORIGIN/../lib`, so dropping `LD_LIBRARY_PATH` from the child environment remains correct.
`AppRun` is a symlink, not a `#!` script, so `/proc/<pid>/exe` resolves to the managed
`usr/bin/retroarch` inside the version tree — M7's separate `expected_executable_path` records
exactly that.

### Cores

All four M7 reference cores remain available from the official libretro Linux x86_64 core buildbot
and their upstream projects are unchanged. No substitution was made.

| Component | libretro core | Upstream | Artefact | SHA-256 | Bytes | Licence |
| --- | --- | --- | --- | --- | --- | --- |
| `nestopia` | `nestopia_libretro` | `github.com/libretro/nestopia` | `nestopia_libretro.so.zip` | `9ec90cae869191d6a1985b04c2e0c1f10a969c5ccd695028d1285ef313273620` | 723 373 | GPL-2.0-or-later |
| `bsnes-mercury-balanced` | `bsnes_mercury_balanced_libretro` | `github.com/libretro/bsnes-mercury` | `bsnes_mercury_balanced_libretro.so.zip` | `d49103384d09b411eafb175999add943bdaa99794390cc36dff25202a03e05d1` | 589 198 | GPL-3.0-only |
| `beetle-psx` | `mednafen_psx_libretro` | `github.com/libretro/beetle-psx-libretro` | `mednafen_psx_libretro.so.zip` | `07bcd1f680d9732640a6a648bc59a1792209b60f992ff2d60ad217c98c2eb4e5` | 1 746 959 | GPL-2.0-only |
| `dolphin` | `dolphin_libretro` | `github.com/libretro/dolphin` | `dolphin_libretro.so.zip` | `289ce0390dcc89f30913573b7b7afcc324f964754d1741e20bdf8e0a0181f14c` | 6 501 960 | GPL-2.0-or-later |

Each core zip is redistributed byte-for-byte as the TUF target; the zip contains exactly one file,
which extraction places at `cores/<component>/<core>.so`.

### Dolphin support component

| | |
| --- | --- |
| Upstream | Official libretro system-assets buildbot, `https://buildbot.libretro.com/assets/system/Dolphin.zip` |
| Upstream SHA-256 | `a406e5207481806f358b726ccc674f169d6e1a0c0528ae135b76b9e9259ee313` (3 195 803 bytes) |
| Licence | GPL-2.0-or-later (the archive ships Dolphin's own GPL-2.0 licence text) |
| Artefact | `dolphin-sys.tar`, a deterministic repackaging of the archive's `dolphin-emu/Sys` subtree |
| Artefact SHA-256 | `591b8df55ad99064824244c33ae9640714dc1701251aa2d2ba65810876fbda90` (7 959 552 bytes) |
| Install path | `runtime/support/dolphin-sys` |

It is never copied from a user's Dolphin installation. Repackaging is required because extraction
deliberately never rewrites archive paths, and the core needs the `Sys` directory itself at
`<system_directory>/dolphin-emu/Sys`. The repackager fixes mode, ownership, modification time, and
entry order, so the produced bytes are reproducible and pinned by digest.

## Release construction

`rf-runtime-release` (`src-tauri/src/bin/rf_runtime_release.rs`, behind the non-default
`release-tools` cargo feature so none of it ships in the application binary):

```text
committed release definition
  -> pinned upstream inputs, each verified against its declared length and SHA-256
  -> derived component artefacts, each verified against its own pin
  -> canonical (RFC 8785 JCS) release manifest + runtime policy
  -> proof extraction through the real client extractor, then verify_tree + validate_app_run
  -> signed TUF 1.0 repository
```

```bash
rf-runtime-release build   --definition release/linux-x86_64/runtime-release.json --output <dir>
rf-runtime-release publish --definition release/linux-x86_64/runtime-release.json --output <dir> --keys <keys-dir>
```

`--offline` refuses to download and uses only the verified input cache. No `curl | sh`; every input
is a pinned HTTPS URL whose bytes must match before use.

The **installed inventory is derived, not observed**: the tool reads each artefact with the same
archive readers the client extractor uses and emits every path, type, size, SHA-256, executable
bit, and symlink target. It then *proves* the result by extracting every component through
`LinuxRuntimeArchiveExtractor` against that inventory and running the client's own `verify_tree`
and `validate_app_run`. A definition that would produce a tree the client refuses fails on the
maintainer's machine.

The current release has **2932 inventory entries** and a **626 988 byte** manifest, against a
`MAX_MANIFEST_BYTES` limit of 1 MiB. See *Unresolved gates*.

## Trust

The qualification release is authenticated by a real TUF 1.0 repository built to the ADR-012
profile, and consumed by the unmodified production `ToughTrustedReleaseSource`:

- Ed25519 only, SHA-256 target digests, consistent snapshots, `spec_version` 1.0.0.
- `root` 2-of-3, `targets` 2-of-3, `snapshot` 1-of-1, `timestamp` 1-of-1, no key reused across roles.
- Metadata lifetimes as ADR-012 specifies: 366 / 90 / 31 / 7 days for root / targets / snapshot /
  timestamp.
- The release manifest repeats each component's exact target name, length, and SHA-256, and
  `TrustedRelease::validate` requires those to equal trusted TUF targets metadata before any
  download.
- An authenticated `runtime-policy.json` target carries the minimum safe release sequence and the
  revocation list.

**No private key material is in this repository.** Keys are generated into a directory the
maintainer names on the command line, outside the repository, with `0700`/`0600` permissions;
`*.pk8` and `qualification-keys/` are additionally gitignored. The generated root is verified to be
self-authenticating before it is written.

### What is deliberately *not* production trust

Qualification keys are generated on one machine and held by one person. ADR-012 requires offline
root and targets keys under independent custody, with a documented rotation and recovery ceremony.
That ceremony, and the public repository hosting it authenticates, are **M10** work. Until then
this build ships **no** production trusted root, and `production_release_source()` honestly returns
`None`.

## Installation source configuration

`src-tauri/src/adapters/runtime_release_source.rs` decides *where* trusted metadata comes from and
never *whether* it is trusted.

- **Production** — a trusted root and repository URLs compiled into a signed build. Does not exist
  yet (M10). The application then reports that installation is unavailable rather than pretending.
- **Qualification** — explicit environment opt-in, exact value `qualification`:

```bash
RETROFRONTIER_RUNTIME_SOURCE=qualification
RETROFRONTIER_RUNTIME_TUF_ROOT=<repo>/metadata/root.json
RETROFRONTIER_RUNTIME_METADATA_URL=file://<repo>/metadata/
RETROFRONTIER_RUNTIME_TARGETS_URL=file://<repo>/repository-targets/
RETROFRONTIER_RUNTIME_MANIFEST_TARGET=rf-runtime-linux-x86_64-001.manifest.json
# RETROFRONTIER_RUNTIME_POLICY_TARGET defaults to runtime-policy.json
```

A partially configured environment is a startup error, not a silent fallback. `https` and `file`
are the only accepted schemes, enforced by `ToughTrustedReleaseSource` itself.

This is not a trust hole: the root is self-authenticating TUF material, and anyone who can set this
process's environment can already replace its binary. The configured origin travels all the way to
the UI so a qualification build is never displayed as a public release channel.

`LocalTrustedReleaseSource` remains what it was — a unit-test and fixture source. It is **not** the
qualification proof, because it does not exercise TUF authentication.

## Settings UX

`RETROARCH RUNTIME` panel in Settings (`src/features/settings/RuntimePanel.tsx`):

```text
Not Installed  ── INSTALL RUNTIME ──▶  Installing (progress, action disabled)  ──▶  Ready
                                                     │
                                                     └─▶ typed failure + retry only when retrying can help
```

- The panel shows the verified state badge, the installed release and installation id, and the
  release-source origin.
- `INSTALL RUNTIME` is disabled with a stated reason whenever it cannot succeed — no configured
  source, or an operation already running.
- Failures are normalized codes, never IPC errors and never OS text:
  `sourceNotConfigured`, `installationInProgress`, `gameRunning`, `releaseNotTrusted`,
  `downloadFailed`, `verificationFailed`, `extractionFailed`, `storageLimit`,
  `unsupportedPlatform`, `installationFailed`.
- `gameRunning`, `sourceNotConfigured`, `unsupportedPlatform`, and `installationInProgress` offer no
  retry, because retrying cannot fix them.
- The authoritative state is always re-read from the backend after an attempt; the response alone
  never drives the badge.
- `RUNTIME NOT READY` on Game Detail is untouched. PLAY is still gated by RuntimeManager.

## Real qualification matrix

Performed on Fedora 44, KDE Plasma 6 Wayland, x86_64, through the real application services
(`RuntimeApplicationService::install_runtime`, then `LaunchApplicationService::launch_game`).
RetroArch was never invoked from a shell and no `runtime/versions` tree was hand-created.

| System | Runtime | Core loaded | BIOS / support | Content executed | Exit / reconcile |
| --- | --- | --- | --- | --- | --- |
| NES (Nestopia UE) | PASS | PASS | n/a | PASS | PASS |
| SNES (bsnes-mercury Balanced) | PASS | PASS | n/a | PASS — *Super Mario World* rendering confirmed | PASS |
| PlayStation (Beetle PSX) | PASS | PASS (installed, approved, resolvable) | BLOCKED — no approved BIOS dump available | BLOCKED — no legal test content available | not exercised |
| GameCube (Dolphin) | PASS | PASS | PASS — managed `dolphin-emu/Sys` link verified | NOT CONFIRMED — no rendered frame observed | PASS |

Detail:

- **NES** — `Over Horizon (Europe)`, content the operator owns. Real RetroArch started, `nestopia`
  resolved from the verified runtime, `game-process.json` reached `running` with full identity,
  clean exit, session `completed`, record cleared.
- **SNES** — `Super Mario World (USA)`, content the operator owns. A screenshot during the run shows
  the title screen rendering in the RetroArch window, so emulation is confirmed, not inferred.
- **PlayStation** — the core installs and resolves, and readiness correctly reports
  `MissingRequiredBios`. The only PlayStation dump on the machine is `scph1001.bin`, which M7
  deliberately removed from the candidate list because Beetle PSX does not look that filename up.
  RetroFrontier reports it as missing rather than usable — the intended behaviour. No PlayStation
  content was available and none was obtained.
- **GameCube** — `dolphin` resolved, and `runtime-user/system/dolphin-emu/Sys` is a symlink into the
  verified immutable version tree. No user Dolphin installation exists on the machine and none was
  consulted. RetroArch ran for 45 s with the operator's own disc image, but no rendered frame was
  captured, so content execution is reported as **not confirmed** rather than passing.

### Process and security regression

Verified against the real runtime:

- Second launch while a game runs → `gameAlreadyRunning`.
- `RuntimeManager::cleanup()` while a game runs → `Err(GameActive)`; mutation stays blocked.
- Process identity: `/proc/<pid>/exe` resolves to the managed `usr/bin/retroarch`, recorded
  separately from the `AppRun` symlink path, exactly as schema 3 intends.
- **Real crash recovery.** The harness process was `SIGKILL`ed while RetroArch was alive. The
  emulator survived as an orphan. A fresh service composition then found `game-process.json`,
  proved the process alive by boot id, `/proc/<pid>/stat` start ticks, and canonical
  `/proc/<pid>/exe`, kept the session `running`, refused runtime mutation with `GameActive`, and
  refused a new launch with `gameAlreadyRunning`. After the orphan was terminated, the record was
  cleared only once death was proven, the runtime returned to `Ready`, and mutation was allowed
  again.
- The installed tree contains only authenticated files: 2759 files, 4 symlinks, 173 directories,
  and `verify_tree` accepts it exactly.

## Observed desktop and focus behaviour (M8 input only)

No M8 behaviour is implemented in this branch. Observed on KDE Plasma 6 **Wayland**
(`XDG_SESSION_TYPE=wayland`):

- RetroArch opens a **decorated, windowed** surface — the generated `retroarch.cfg` sets no
  `video_fullscreen`, so RetroArch's own default applies. Window title is
  `RetroArch <version> | <git hash>`.
- The window is roughly the core's native resolution and is placed by KWin; it appeared **stacked
  above** the previously foreground window when it mapped.
- Whether it takes *keyboard* focus on map was not instrumented: KWin's activation policy governs
  it, and no scripting or input-automation tooling was available in this session to measure it
  reliably. This is the first thing M8 should measure.
- RetroFrontier's own window was neither raised nor lowered by the launch, and **no automatic
  re-raise of RetroFrontier on emulator exit was observed**. M8 should assume the user must return
  to RetroFrontier manually until proven otherwise.
- The child receives `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, and `DISPLAY` from the M7 allowlist. The
  AppImage bundles both X11 and Wayland client libraries, so the backend RetroArch actually selects
  was not pinned down; X11-versus-Wayland differences remain unmeasured.

## Other findings

- **`logs/retroarch/` is never populated.** The generated configuration sets `log_dir` and
  `log_to_file = true` but no `log_verbosity`, so RetroArch writes no log file. `docs/RETROARCH_LAUNCH.md`
  claims RetroArch's log goes there. Left unchanged in M7.5 because changing log verbosity is a
  product decision with a performance cost, but the documentation claim is currently untrue.
- **`startup_reconcile` reports `Broken` while a managed game is alive.** Its `ensure_no_active_game`
  failure is caught by the generic "startup must remain usable" handler and downgraded to
  `RuntimeStatus::broken()`, which reads as *repair required* for a perfectly healthy but busy
  runtime. User-visible impact is limited to one startup log line, because `verified_snapshot` —
  which every read boundary including the new Settings panel uses — performs no process check.
  Worth tidying, out of scope here.
- **The manifest is 61 % of its size limit** at four cores. See below.

## Unresolved gates

Completed by M7.5:

- one real Linux x86_64 managed RetroArch release, reproducibly constructed from pinned inputs;
- the four M7 reference cores and the managed Dolphin `Sys` component;
- an authenticated manifest and exact installed inventory;
- a real TUF 1.0 repository consumed by the production verification code;
- a usable `Not Installed → Ready` route in Settings;
- real Linux installation and real launch qualification for NES and SNES, with GameCube partially
  qualified.

Still deferred to M10:

- **Production key ceremony.** Offline root and targets keys under independent custody, rotation,
  and emergency-revocation drills. Until then no production trusted root ships.
- **Public runtime hosting.** An HTTPS repository, mirror and redirect policy, and metadata
  refresh automation that keeps timestamp and snapshot inside their lifetimes.
- **Immutable upstream mirroring.** The core artefacts come from libretro's *nightly* `latest/`
  path and the Dolphin system assets from a rolling `assets/` path; both rotate. A pinned release
  therefore cannot be reconstructed from upstream indefinitely, only from a verified input cache.
  Public distribution needs RetroFrontier-owned immutable copies of every approved input.
- **GPL source-offer obligations.** Every redistributed component is GPL-2.0 or GPL-3.0. Public
  distribution of these binaries requires corresponding source or a written offer, plus licence
  notices in the installer.
- **Manifest size headroom.** 626 988 bytes of 1 048 576 at four cores. Adding the remaining seven
  V1 systems will exceed `MAX_MANIFEST_BYTES`. ADR-012 already permits a separate immutable
  inventory target referenced by digest; M10 should adopt that before the core matrix grows.
- **Bundled frontend assets.** The AppDir carries no RetroArch menu assets (they live in the
  portable-home tree the release excludes), so the RetroArch menu is unstyled if a user opens it.
  Launching straight into content is unaffected.
- Windows and macOS packaging, signing, notarization, the clean-machine matrix, the application
  updater, and the V1 release checklist.

## Explicit non-claims

- This is **not** a public production release, and the trust material behind it is not a production
  key ceremony.
- Cross-distribution qualification was **not** performed. One host: Fedora 44, KDE Plasma 6
  Wayland, x86_64.
- Controller input, audio quality, save behaviour, and save states were **not** qualified here.
- GameCube content execution is **not** claimed.
- PlayStation content execution and BIOS validation against an approved dump are **not** claimed.
- No ROM, BIOS, RetroArch binary, core, AppImage, extracted runtime tree, database, or signing key
  was added to this repository.
