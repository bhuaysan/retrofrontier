# Managed RetroArch Runtime Spike Report

- **Date:** 2026-08-25
- **Status:** Research/disposable Linux proof and GPT Sol Max architecture/security review complete; production RuntimeManager not implemented
- **Model:** GPT Luna Max spike; GPT Sol Max architecture/security review

## 1. Executive conclusion

**Spike conclusion:** viable with platform-specific adaptations.

**Senior-review verdict:** acceptable as a production architecture basis only with the changes in this report and ADR-011/ADR-012. It is not a four-platform production approval.

RetroFrontier can manage an isolated RetroArch runtime without bundling RetroArch in its installer and without using a system RetroArch installation. Linux x86_64 was experimentally proven with an official versioned archive, an extracted AppImage payload, explicit config/core/content paths, synthetic content, and a hostile existing RetroArch configuration.

Windows has a plausible portable archive strategy but needs a real Windows test. macOS has suitable upstream DMG/app artifacts, but downloaded-runtime and native-core signing, quarantine, notarization, and hardened-runtime/library-validation behavior remain an explicit production security blocker. Linux has no catch-all distribution; the one-host proof does not establish a robust extracted-AppImage entry point or distribution matrix. HTTPS plus a hash is insufficient if the expected hash can be replaced by the same attacker.

The review accepts immutable version directories, private staging, a minimal app-owned pointer, explicit control of all RetroArch paths, full reconstruction repair, active-plus-one rollback retention, approved managed cores, and platform adapters. It rejects an authoritative transaction journal, moves the smoke test before activation, requires filesystem-derived recovery and cross-process locking, and selects a TUF 1.0-compatible trust model rather than a bespoke single signed manifest. No updater, extractor, RuntimeManager, UI, or runtime binary was added to the repository.

## 2. Verification status and local proof

Only Linux x86_64 was available for execution. Windows and macOS findings are official-document/distribution research.

- **Experimentally verified:** executed and observed here.
- **Documented, not locally verified:** supported by official documentation or distribution indexes.
- **Unknown:** requires a later real-device or security review.

Host:

~~~~text
Linux workstation 7.1.9-200.fc44.x86_64
Architecture: x86_64
~~~~

All generated material was kept under /tmp/retrofront-runtime-spike, outside the repository. The host had /usr/bin/retroarch; it was deliberately not used.

Procedure and results:

1. Downloaded the official stable archive [RetroArch 1.22.2 Linux x86_64](https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch.7z).
2. A normal transfer ended with an OpenSSL EOF. Validated HTTP range reassembly completed successfully. Final size was 179,361,448 bytes; SHA-256 was 7d62da9a21397d6e1b9490785cedbeafd262781b50115076736fbe8a77ef30e9.
3. Extracted into a versioned managed directory; 7z reported Everything is Ok.
4. Direct AppImage execution failed because this host has no suitable FUSE helper. The documented --appimage-extract operation succeeded.
5. Launched the extracted AppImage payload usr/bin/retroarch by absolute path; --version reported RetroArch 1.22.2, Git 69a4f0e.
6. Downloaded the official [Nestopia Linux core](https://buildbot.libretro.com/nightly/linux/x86_64/latest/nestopia_libretro.so.zip), size 716,942 bytes, SHA-256 f577e98bba42dde4c13dd3cf9760116600535105420bfe76e23536d4de4e3a29.
7. Created a synthetic iNES-format NES fixture; no commercial ROM or BIOS was used.
8. Launched with explicit managed config, log, core, content, and bounded frame count. The process exited successfully and produced a screenshot.
9. Logs showed controlled save, state, screenshot, system/BIOS, playlist, and runtime-log paths.
10. Repeated under a hostile XDG_CONFIG_HOME; the explicit managed config won and the hostile directory received no new files.

The first incomplete config pass attempted to write:

~~~~text
/home/ben/.config/retroarch/cores/core_info.cache
/home/ben/.config/retroarch/config/Nestopia/Nestopia.opt
~~~~

Adding core_info_cache_enable = "false", explicit core_options_path, libretro_info_path, rgui_config_directory, history paths, and runtime-log paths removed those observed host writes. This must be a regression test.

Not tested: Windows, macOS, real graphics/audio devices, controllers, direct AppImage FUSE execution on a desktop, power loss, update, rollback, and repair.

## 3. Distribution and platform matrix

Primary sources: [official platform downloads](https://retroarch.com/?page=platforms), [Windows guide](https://docs.libretro.com/guides/install-windows/), [macOS guide](https://docs.libretro.com/guides/install-macos/), [Linux instructions](https://retroarch.com/index.php?page=linux-instructions), [buildbot](https://buildbot.libretro.com/), and the [RetroArch repository](https://github.com/libretro/RetroArch).

| Target | Candidate artifact and installation | Feasibility | Risks and status |
|---|---|---|---|
| Windows x86_64 | Versioned `RetroArch.7z`; stable index also exposes installer/core archives. Extract a prepared, approved archive into managed storage. | **Requires experiment.** Pointer/version model is plausible. | Authenticode/Smart App Control/SmartScreen, antivirus, locks, process tree, long paths, graphics/audio/controllers untested. |
| macOS arm64 | Stable `RetroArch_Metal.dmg` is universal; arm64 core indexes exist. Install an independently signed/notarized approved payload into a version directory. | **Requires further experiment; current blocker.** | Gatekeeper, quarantine, notarization, Team ID, Hardened Runtime, library validation, `.dylib` loading, update/rollback unresolved. |
| macOS x86_64 | Stable universal and x86_64 DMGs/core indexes exist. Install an independently signed/notarized approved payload. | **Requires further experiment; current blocker.** | Same signing/core risks plus real Intel testing; a universal host does not make incompatible cores loadable. |
| Linux x86_64 | Versioned archive containing an AppImage. Verify, safely extract, and launch the proven AppDir entry point without FUSE. | **Requires experiment.** One-host inner-binary proof succeeded. | `AppRun` behavior, glibc/distro portability, graphics/audio/controller dependencies, and RetroFrontier packaging interaction. |

Stable indexes reviewed: [Windows](https://buildbot.libretro.com/stable/1.22.2/windows/x86_64/), [Linux](https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/), [macOS universal](https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/), and [macOS x86_64](https://buildbot.libretro.com/stable/1.22.2/apple/osx/x86_64/).

Base runtime archives do not imply a complete core library. Official indexes expose separate core archives and individual packages: .dll.zip on Windows, .dylib.zip on macOS, and .so.zip on Linux. The [core-info repository](https://github.com/libretro/libretro-core-info/) supplies display version, system, license, and firmware metadata but not necessarily a stable binary build identity. RetroArch is GPLv3; core licenses vary. Core directories such as nightly/<platform>/latest/ are mutable evidence sources, never production release IDs.

## 4. Isolation and recommended launch strategy

RetroArch’s [CLI documentation](https://docs.libretro.com/guides/cli-intro/) supports an explicit config path and explicit -L core path. Its implicit defaults include Linux/macOS XDG/home paths and Windows locations near the executable and in APPDATA. Managed launches must never depend on them.

Rust should read `runtime/active.json`, validate an exact installation below `runtime/versions`, and resolve an absolute executable:

~~~~text
Windows: versions/<installation-id>/runtime/retroarch.exe
macOS:   versions/<installation-id>/runtime/RetroArch.app/Contents/MacOS/RetroArch
Linux:   versions/<installation-id>/runtime/squashfs-root/AppRun (candidate; prove before approval)
~~~~

Launch with an argument vector containing an explicit managed config, managed log, managed core, and absolute content path. Use a controlled working directory, never the ROM directory. Construct the child environment rather than inheriting it blindly; remove unrelated RetroArch variables and review LD_PRELOAD and DYLD_*. LIBRETRO_* variables are defense in depth, not the authority.

Explicitly control, as applicable: libretro_directory, libretro_info_path, system_directory, savefile_directory, savestate_directory, screenshot_directory, assets_directory, core_assets_directory, video_shader_directory, playlist_directory, content_database_path, thumbnails_directory, input_remapping_directory, overlay_directory, joypad_autoconfig_dir, cache_directory, log_dir, runtime_log_directory, history paths, and core_options_path. Disable or redirect the core-info cache. This is path isolation, not a sandbox; the [RetroArch security policy](https://github.com/libretro/RetroArch/security) says cores can read/write/delete files, spawn processes, and use the network.

## 5. Recommended runtime layout and active reference

~~~~text
<app-data>/RetroFrontier/
├── runtime/
│   ├── versions/
│   │   ├── <installation-a>/
│   │   │   ├── runtime/                 # immutable executable/support files
│   │   │   ├── cores/                   # pinned release-compatible cores
│   │   │   ├── core-info/
│   │   │   ├── licenses/
│   │   │   ├── release-manifest.json    # canonical authenticated manifest
│   │   │   └── complete.json            # written last; immutable afterward
│   │   └── <installation-b>/
│   ├── staging/<operation-id>/
│   ├── active.json
│   ├── game-process.json                  # durable launch/liveness record
│   └── locks/
├── runtime-trust/                            # trusted TUF state; not runtime cache
├── runtime-user/
│   ├── config/ assets/ shaders/ autoconfig/
│   └── playlists/ cache/ core-options/
├── database/
├── metadata/
├── saves/
├── states/
├── screenshots/
└── logs/
~~~~

User-visible content remains:

~~~~text
Documents/RetroFrontier/
├── ROMs/
└── BIOS/
~~~~

ROMs, BIOS, normal saves, save states, metadata, SQLite, and user configuration must not be inside replaceable runtime versions.

The runtime, activation metadata, and `runtime-trust` require a local application-data filesystem with supported locking and same-directory replacement semantics. V1 does not place them on a network share or cloud-synchronized root. This restriction does not apply to separately configured external ROM roots.

| Reference mechanism | Finding |
|---|---|
| Symlink | Easy on Unix, but Windows privilege/dev-mode policy, reparse-point resolution, and security handling complicate a common authority. |
| Junction | Windows-specific directory/reparse semantics and security handling complicate a common authority. |
| Renamed active directory | Harder crash recovery and encourages in-place coupling. |
| Database-only field | Useful indexed state, but insufficient as filesystem activation authority. |
| Small pointer file | Same model on every platform; can identify one exact installation and be replaced in one directory. |

Use runtime/active.json as the sole activation authority; a database active-version field is derived. Do not require symlinks or junctions. Validate that the opaque installation ID is a safe basename. Resolve the trusted runtime root, `versions`, and installation-directory boundary by handle without following links or reparse points; separately validate any format-approved internal bundle links against the authenticated inventory. Require a valid completion marker and exact canonical release-manifest digest:

~~~~json
{
  "schema_version": 1,
  "installation_id": "01J6RUNTIME7Q4M5N8P2X3Y9Z0AB",
  "manifest_sha256": "…"
}
~~~~

Do not include the semantic runtime version, generation, activation timestamp, previous release, or health state. Those values are either authenticated manifest data, derived application state, or mutable history. A unique installation ID permits full reconstruction of the same approved release without overwriting a damaged or locked tree.

## 6. Draft runtime manifest

This remains proof evidence rather than a final schema. The production release manifest is a strict RFC 8785 JCS document published as an immutable TUF target; it does not contain an embedded signature object:

~~~~json
{
  "schema_version": 1,
  "manifest_id": "rf-runtime-2026-08-25-linux-x86_64",
  "channel": "stable",
  "min_retrofrontier_version": "0.1.0",
  "release": {
    "release_id": "rf-1.22.2-linux-x86_64-001",
    "release_sequence": 1,
    "retrofrontier_runtime_version": "2026.08.25.1",
    "retroarch_version": "1.22.2",
    "platform": "linux",
    "architecture": "x86_64",
    "components": [
      {
        "id": "retroarch",
        "kind": "runtime",
        "source_url": "https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch.7z",
        "archive_format": "7z",
        "archive_size_bytes": 179361448,
        "sha256": "7d62da9a21397d6e1b9490785cedbeafd262781b50115076736fbe8a77ef30e9",
        "expected_root": "RetroArch-Linux-x86_64",
        "executable_relative_path": "runtime/squashfs-root/usr/bin/retroarch",
        "license": "GPL-3.0"
      },
      {
        "id": "nestopia",
        "kind": "core",
        "source_url": "https://buildbot.libretro.com/nightly/linux/x86_64/latest/nestopia_libretro.so.zip",
        "archive_format": "zip",
        "archive_size_bytes": 716942,
        "sha256": "f577e98bba42dde4c13dd3cf9760116600535105420bfe76e23536d4de4e3a29",
        "payload_filename": "nestopia_libretro.so",
        "display_version": "1.53.1",
        "source_revision": null,
        "source_pinning": "proof-only-moving-nightly-url",
        "license": "GPL-2.0",
        "firmware": [{"path": "disksys.rom", "required": false}]
      }
    ]
  },
  "compatibility": {
    "retroarch_core_api": "record-required",
    "save_state_policy": "associate-runtime-and-core-version"
  }
}
~~~~

The moving core URL, null source revision, and inner Linux executable path intentionally show what production must fix: use immutable TUF target paths, record a build/revision identity, and prove the extracted AppDir's `AppRun` entry point. Every runtime archive, core archive, and external inventory is itself a TUF target. The final release manifest repeats each component's trusted target name, exact size, and SHA-256 and must match TUF targets metadata. It must also authenticate every download/extraction/launch input: platform/architecture, approved mirror and redirect policy, expected roots, extraction limits, executable paths, component versions, support assets, licenses, core allowlist, OS code-signing requirements, compatibility, format-specific link policy, and an exact installed-file inventory or the digest of a separate inventory target. Expiration and metadata-version freshness belong to TUF metadata; an authenticated versioned runtime-policy target carries release revocations and minimum-safe release sequences.

## 7. Integrity, authenticity, and security

| Property | Provides | Does not provide |
|---|---|---|
| HTTPS | Encrypted transport and endpoint authentication | Proof that a compromised approved endpoint serves approved content |
| SHA-256 | Difference/corruption detection against a trusted expected hash | Publisher authenticity when the hash came from the same compromised source |
| TUF-signed metadata | Authentication of approved target lengths, hashes, release policy, and metadata consistency under threshold keys | Safety after threshold compromise, bad approval, or malicious native cores |
| Upstream signature | Upstream identity, if available and verified | RetroFrontier approval or compatibility |
| Authenticated release manifest | RetroFrontier approval and compatibility of one exact artifact set | That approved upstream code is benign |

The [RetroArch security page](https://github.com/libretro/RetroArch/security) says RetroArch/core binaries are not signed and that cores have broad capabilities. Reviewed stable indexes exposed artifacts but no established upstream signature sidecars.

Confirmed requirements: never use upstream latest as a release identity; verify HTTPS transfer, size, digest, archive policy, and manifest before activation; treat cores as native code; keep keys/secrets/binaries out of the repository; preserve licenses.

Review decision: use a conforming TUF 1.0 client and repository profile. Embed initial trusted root metadata in the signed application; use Ed25519 with 2-of-3 offline thresholds for root and targets, separately scoped snapshot/timestamp keys, consistent snapshots, persisted metadata versions, expiration for update operations, an authenticated release-policy target, and a monotonic minimum-safe release sequence. The canonical release manifest and every downloadable component are immutable TUF targets whose exact lengths and SHA-256 values are authenticated by targets metadata. Initial maximum lifetimes are 7 days for timestamp, 31 days for snapshot, 90 days for targets, and 366 days for root. ADR-012 is authoritative.

Cryptographic target verification is offline-capable once trusted metadata and bytes are present. Installed runtimes remain usable offline after update metadata expires. Expiration blocks discovering/downloading under stale metadata; it does not brick an already authenticated runtime. Full reconstruction may reuse a previously verified cache at or above the locally known security floor. A client cannot know about a revocation it has never received, and no metadata design can prevent a compromised host from denying service. Once an authenticated revocation is received, RetroFrontier may block the vulnerable active runtime even if the host withholds a replacement; this intentional security-over-availability result is a repair-required state.

Still open for implementation/release readiness: TUF client-library selection, independent key custody and recovery drills, immutable artifact hosting/redistribution, and platform code signing. Compromise of enough targets keys can authorize malicious native code until revocation reaches clients and can poison accepted version/floor counters; clients that cannot be advanced past those counters require an independently authenticated application update. Compromise of the root threshold always requires independently authenticated application or out-of-band recovery.

If manifest and hashes come from the same compromised endpoint, an attacker can change both and pass SHA-256. HTTPS does not close that gap.

**Is HTTPS + a signed manifest + SHA-256 sufficient?** Not as a bare design. It authenticates transport and bytes only while its unspecified signing key and manifest freshness remain trustworthy. The intended threat model additionally requires threshold key custody and rotation, emergency recovery, metadata version/expiry and release-floor anti-rollback checks, exact installed-tree validation, safe extraction, cross-process coordination, and independent Windows/macOS code-policy validation. With the TUF profile and those controls, the combination is sufficient against remote substitution and replay below the configured key threshold; it still cannot make approved native code safe, defeat same-user/administrator compromise, or guarantee availability.

## 8. Safe extraction requirements

Future extraction must use a new operation directory below runtime/staging and reject absolute, drive-letter, UNC, and traversal paths after normalization; canonicalize and contain destinations; reject hard links, junctions, reparse points, device files, sockets, FIFOs, and unexpected duplicates; enforce compressed-size, expanded-size, entry-count, and compression-ratio limits; allow executable destinations only from a manifest allowlist; validate a fresh tree before installation; reject truncated/corrupt archives; use resumable bounded downloads; and clean incomplete staging without touching an active version or user data.

Generic ZIP/7z extraction rejects symbolic links. A reviewed platform-format adapter may preserve a symbolic link required by an authenticated AppImage AppDir or macOS bundle only when the exact inventory declares the link type and normalized relative target, the target remains within the new immutable tree, and extraction creates the link without following it. Validate the final link graph for escape and cycles. Never permit a link to runtime-user or user data; Windows reparse points remain forbidden. This narrow exception is necessary because rejecting every internal link can invalidate otherwise legitimate platform bundles.

Use synthetic archives to test traversal, absolute paths, links, duplicates, oversize declarations, compression bombs, malformed headers, and interrupted writes. No production extractor was written.

Extraction operates through descriptor/handle-relative paths where practical, never follows a pre-existing link, and never overwrites an existing version tree. The exact installed tree, including permitted link targets, is checked against the authenticated inventory before `complete.json` is committed. The selected executable, selected core, and managed transitive native-code files are rechecked before launch or under an equivalent platform code-signing policy. This detects many accidental/manual changes but cannot fully defend against malware already executing as the same OS user or eliminate every verify-to-execute race.

An AppImage's `--appimage-extract` option executes the AppImage runtime. Production must verify the complete TUF target first and then either use a reviewed non-executing SquashFS extractor or explicitly treat the verified AppImage runtime as approved executable code. After extraction, test the AppDir's `AppRun` entry point; directly invoking `usr/bin/retroarch` bypasses AppImage entry-point behavior and is not yet a production-approved strategy.

## 9. Atomic activation, state machine, and recovery

~~~~text
refresh trusted metadata and resolve approved target
  -> resumable download
  -> verify trusted length/hash
  -> extract fresh staging tree
  -> validate exact inventory/allowlist/platform/OS signature
  -> finalize under a unique incomplete version path
  -> bounded smoke test from final path and revalidate tree
  -> write/flush completion marker last; version becomes immutable
  -> acquire mutation lock
  -> ensure no game/RetroArch process is running
  -> retain only current selection and candidate among complete installs
  -> atomically replace active.json
  -> retain previous release
~~~~

Use immutable version directories and replace only the pointer. ADR-011 defines the exact high-level protocol: create a uniquely named temporary pointer in the same directory, enforce a 4 KiB strict schema, write and flush it, close/reopen/validate it, then replace. Linux uses atomic rename plus parent-directory `fsync`; macOS uses rename plus the strongest supported flush and directory synchronization; Windows uses `ReplaceFileW` with a same-volume recovery backup for an existing pointer without relying on its unsupported write-through flag, or `MoveFileExW` with `MOVEFILE_WRITE_THROUGH` for first install, followed by explicit inspection because documented failure states differ. Windows readers request delete sharing and close promptly. The backup is not an authority. No platform is promised universal power-loss immunity; startup always validates the result.

### Durable, inferred, and transient state

Durable correctness state is limited to trusted TUF metadata and anti-rollback floors under `runtime-trust/`, canonical release manifests, immutable version trees with completion markers, `active.json`, and the game-process identity record. Per-staging resume metadata is disposable. Runtime uninstall and cache cleanup preserve `runtime-trust/`.

`NotInstalled`, `Ready`, `Broken`, `UpdateAvailable`, and `RollbackAvailable` are inferred from those artifacts. `Downloading`, `Extracting`, `Verifying`, `Finalizing`, `SmokeTesting`, `Activating`, `Repairing`, and `RollingBack` are transient operation/UI phases and are not persisted as an authoritative state machine. `Staging`, `Updating`, and `Installing` are only umbrella UI labels over those phases.

### Transaction journal decision

V1 has no authoritative transaction journal and no durable `previous.json`. Active-pointer-only correctness cannot remember a post-activation smoke-test result or an intended predecessor. Those needs disappear when the candidate is smoke-tested before activation and, under the mutation lock, older inactive installations are removed before replacement while the current selection and candidate are preserved. After success the former current installation is the sole inactive rollback candidate; after failure the current selection remains authoritative. A journal would improve audit/progress resumption, but it would also create a second mutable record whose partial write and ordering against `active.json` require recovery. The filesystem already distinguishes every correctness-relevant phase.

| Alternative | Material result |
|---|---|
| A. Immutable versions + staging + atomic pointer | One mutable authority; correctness recovery is inferred from staging, completion markers, the active selection, and the sole inactive fallback. Interrupted progress may be retried or discarded. |
| B. Alternative A + authoritative journal | Can preserve detailed progress/audit intent, but does not improve activation atomicity and adds partial-write plus journal/pointer ordering states. |

Choose **A** for V1. Disposable download-resume metadata inside one staging operation is allowed because losing or discarding it cannot change the active runtime. Best-effort diagnostic logs may record operations, but recovery never trusts them.

### Crash recovery

- **Download:** keep `.part` data only inside staging. Resume only with compatible server validators, then hash the entire completed target; otherwise discard.
- **Extraction:** a crash leaves a staging tree, never an installed or active tree. Delete or restart it.
- **Verification:** no completion marker exists, so reverify from authenticated inputs or discard.
- **Finalization:** a version without a valid completion marker is incomplete. A complete but inactive version is harmless and may be reused only after revalidation.
- **Pointer update:** a reader sees old or new on POSIX. On Windows, inspect active/temp/backup outcomes and restore only a fully verified pointer. Never guess the highest version.
- **First launch:** one game/core crash does not trigger automatic rollback. Record the failure and offer an approved manual rollback; deterministic app-controlled preflight failures should have prevented activation.
- **Rollback:** it is another atomic pointer replacement, so a crash leaves one of two complete approved selections.
- **OS restart:** startup runs the same validation and removes only owned incomplete staging after locks and process liveness checks.

A missing, malformed, oversized, stale-below-policy, or target-mismatched `active.json` produces a broken/repair-required state unless a verified platform replacement backup can be restored. It never causes deletion of ROMs, BIOS, saves, states, metadata, SQLite, or user configuration.

## 10. Rollback and repair

V1 retains two installed runtimes total under normal conditions: the active installation and at most one previous known-good approved installation. A staged/finalizing candidate may temporarily make three trees. Apply both a count cap and a byte/minimum-free-space policy. Delete an obsolete inactive version before downloading if needed for space. Immediately before pointer replacement, under the mutation lock, preserve the current selection and candidate and remove every other complete inactive installation; if safe cleanup cannot complete, abort activation. A successful switch therefore leaves the former current installation as the sole rollback candidate. Refuse an update if the active tree plus candidate cannot fit, and never delete the active tree to create space. Cleanup never follows links or escapes `runtime/versions`.

Automatic rollback is deliberately narrow. A candidate that fails deterministic validation or the app-controlled smoke test is never activated. If pointer replacement reports a failure, restore the previously validated pointer within that operation. V1 performs no post-activation automatic rollback: one game/core crash, a malformed active selection, or detected runtime tampering produces an explicit error/repair path rather than silently selecting older native code. Manual rollback may be offered after a launch failure or by explicit user action, but may select only a complete, compatible approved installation at or above the freshest authenticated minimum-safe release sequence. A known-vulnerable/revoked runtime is blocked even if retained locally.

Normal game saves (SRAM, memory cards, and equivalent persistent game data) are runtime-independent user data and require no runtime retention. Emulator save states are implementation snapshots whose compatibility may depend on exact core/runtime versions. Record that identity and warn on mismatch, but do not keep unlimited runtimes or bypass a security floor merely to preserve hypothetical save-state compatibility.

For V1 repair, use full reconstruction of the exact approved release into fresh staging and a new installation ID. Verify trusted metadata, build and validate a complete tree, finalize/smoke-test it, and activate normally. Never patch the active tree or create a mixed-version installation. Verified download-cache reuse may reduce bandwidth, but it is only an input optimization and does not weaken whole-tree validation.

## 11. Platform risks

### Windows x86_64

The official portable `.7z` model fits app-data management better than a traditional installer. Use absolute Unicode paths, a shallow bounded runtime path, same-volume version directories, no in-place DLL/executable updates, a safe working directory, and a sanitized child environment/PATH. Authenticate every managed DLL and test the runtime's DLL search behavior against working-directory and PATH hijacking. Running files remain in their old immutable tree; cleanup waits for the complete RetroArch process tree and tolerates delayed antivirus release.

Microsoft documents publisher and file-hash reputation, and current Smart App Control can evaluate all executable files rather than only browser downloads. Signing RetroFrontier does not sign downloaded `retroarch.exe`, DLLs, or core DLLs. The intended Windows release pipeline should Authenticode-sign every distributed PE with one stable publisher identity after the final binary build and before immutable archive packaging and TUF hashing, subject to licensing/release-policy review. Signing improves identity/reputation but does not guarantee no first-download warning. Do not strip Mark-of-the-Web/Zone.Identifier merely to suppress warnings; test how the intended downloader and extractor propagate reputation and security-zone information.

The active-pointer Windows adapter must test `ReplaceFileW` failure outcomes, same-volume backup recovery, `MoveFileExW` first install, flushing, readers opened with delete sharing, Defender/third-party antivirus interference, and OS restart. Use wide APIs and test non-ASCII paths, reserved names, reparse points, long ROM paths, and whether both RetroFrontier and RetroArch are long-path aware.

**Windows verdict:** viable with requirements, but the portable-runtime and pointer semantics remain **REQUIRES EXPERIMENT** before production.

### macOS arm64/x86_64

The official model is a DMG copy of `RetroArch.app`; the stable Metal app is universal and architecture-specific core indexes exist. Artifact availability does not establish deployability. Apple requires Developer ID signing, Hardened Runtime, and notarization for normal direct distribution, and explicitly says a network installer must separately notarize both the installer and the items it downloads. A notarized RetroFrontier DMG/app therefore does not confer trust on a later-downloaded RetroArch app or `.dylib` core.

The RetroArch process—not RetroFrontier—is the host that loads cores. Under Hardened Runtime library validation, loaded libraries must be Apple-signed or signed with the host's Team ID unless RetroArch carries `com.apple.security.cs.disable-library-validation`. Disabling library validation is a material reduction in protection and is not assumed acceptable. The preferred candidate is a RetroFrontier-controlled, Developer ID-signed and notarized runtime/core set with all native code signed consistently, immutable after signing, and verified through both Apple policy and TUF metadata. Sign nested code from the inside out, notarize the final distributed containers, staple where the format supports it, and only then publish immutable TUF targets and hashes. If relying on upstream signatures instead, their exact identity, notarization, entitlement, and update properties must be proven.

All code loaded in the RetroArch process must match its running architecture. A universal app does not make an x86_64-only core load into an arm64 process; Rosetta is a separate x86_64 execution mode. The release manifest must select an exact architecture-compatible set, and both real Apple Silicon and real Intel targets require tests.

Do not remove quarantine as a product strategy. Use the intended downloader, inspect/preserve realistic quarantine state, and validate with `codesign --verify --deep --strict`, `spctl --assess`, notarization/stapling evidence, first launch, core load, synthetic content, update, rollback, offline relaunch, and restart. RetroFrontier itself may be distributed through a notarized DMG outside the store. The current managed-code download model conflicts with Mac App Store self-containment/code-download rules and must not claim App Store compatibility.

**macOS verdict:** **REQUIRES FURTHER EXPERIMENT** and is currently a production security blocker. It becomes viable with requirements only after the signed/notarized runtime and approved-core path passes on arm64 and x86_64 without an unaccepted library-validation bypass.

### Linux x86_64

Official guidance says there is no catch-all build. Extracted-AppImage execution passed on one Fedora host; direct AppImage execution failed for lack of FUSE. AppImage supports extraction, but its AppDir contract names `AppRun` as the entry point. The spike launched the inner `usr/bin/retroarch`, which can bypass bundled-library/environment setup and is not yet a robust production choice. Test the verified extracted AppDir through `AppRun`, then prove all explicit RetroArch paths still win and sanitize inherited AppImage/loader variables.

AppImage portability still depends on the build's glibc baseline and host kernel, GPU driver/OpenGL/Vulkan stack, audio stack, display server, udev/input permissions, and controller devices. Define a minimum supported distribution baseline and test at least Ubuntu LTS, Debian stable, and current Fedora across Wayland/X11, PipeWire/PulseAudio as applicable, Intel/AMD/NVIDIA graphics where available, controllers, saves, and return-to-frontend behavior.

If RetroFrontier ships as Flatpak, the managed child runs in a materially different sandbox. Flatpak defaults exclude host files, network, graphics/audio sockets, and input devices; host execution requires broad `org.freedesktop.Flatpak` access. Do not assume the native/AppImage runtime adapter works from a Flatpak. Treat Flatpak as a separate packaging spike or exclude it from the V1 Linux package set.

**Linux verdict:** viable with requirements on native distribution, but the extracted-AppImage production strategy is **REQUIRES EXPERIMENT** across the declared distribution/device matrix.

## 12. Core-management implications

This does not finalize `CORE_MATRIX.md`; only Nestopia was used. Cores are independent platform-specific native packages with the user's process permissions. Each approved core archive is a separate TUF target and release-manifest component. Exact archive and installed-payload hashes, build/revision identity, license, platform/architecture, supported-system mapping, and OS code-signing requirements are authenticated separately for every core. `.info` metadata is useful but does not identify or authenticate the binary.

V1 enforces managed approved cores only. RetroFrontier does not load an arbitrary core path, a core from a system RetroArch installation, a user-downloaded core, or a general online-core-store result. A per-game override may select another installed approved core only. A future advanced/custom-core mode requires a separate product/security decision, separate storage, and explicit risk communication; it must not weaken the managed default. Manifest approval establishes provenance and project approval, not sandboxing or benign behavior.

## 13. Recommended architecture changes

Evidence supports these refinements:

1. Replace generic active-runtime-reference with the minimal `runtime/active.json` and filesystem-derived recovery. Reject an authoritative transaction journal and durable previous pointer. Symlinks/junctions are not required.
2. Make versions immutable and move runtime-user config/options/cache/logs outside them.
3. Add platform adapters for Windows portable archives, macOS app/signing validation, and Linux AppImage extraction/FUSE.
4. Add explicit core-info-cache, core-options, libretro-info, history, cache, and runtime-log controls to the RetroArch launch contract.
5. Use TUF 1.0-compatible trusted metadata with Ed25519 threshold keys, SHA-256 targets/components, consistent snapshots, expiry for update operations, and signed rollback floors.
6. Use full-candidate reinstall for V1 repair.
7. Enforce managed approved cores only in V1.
8. Enforce single-instance behavior plus independent runtime mutation and live-game coordination.
9. Keep macOS signed/notarized runtime/core loading as a production blocker pending real-hardware proof.

## 14. Recommended next implementation task

The exact next Luna Max runtime-risk task is the smallest macOS proof-of-concept, not RuntimeManager implementation: on a real Apple Silicon Mac, use a notarized Developer ID RetroFrontier test launcher distributed from a quarantined DMG to download one intended signed/notarized RetroArch candidate and one approved architecture-compatible core through the intended packaging path; preserve realistic quarantine; verify TUF fixtures plus `codesign`/`spctl`; install under Application Support; load synthetic content; exercise pointer activation, restart, update, rollback, and offline relaunch; and document whether library validation works without an unacceptable entitlement. Repeat the proven recipe on a real Intel Mac before macOS x86_64 approval. Keep all binaries and generated material outside the repository. This is a macOS release gate; the platform-adapter boundary means it does not block separately scheduled Linux-first foundation work.

## 15. Explicit threat model

### Assets and trust boundaries

The protected assets are the application trust root, trusted metadata history and rollback floor, active runtime selection, runtime executable/core bytes, runtime-user configuration, ROMs, BIOS, normal saves, save states, metadata, and SQLite data. Network origins, mirrors, CDNs, upstream build systems, archives, native cores, local app-data directories, and every archive parser are trust boundaries.

The model assumes the installed RetroFrontier application and fewer than the configured root/targets key threshold remain trustworthy. It assumes ordinary OS filesystem and process isolation between different users. Administrator/root compromise, compromise of the RetroFrontier application-update trust root, and malware already executing as the same OS user are outside the boundary that this updater can fully defend.

| Threat/failure | Required defense | Residual limit |
|---|---|---|
| Compromised download server or CDN | TUF target length/hash, SHA-256 streamed before use, immutable target naming, bounded response/download | Can withhold or corrupt indefinitely, causing detected denial of service |
| Compromised manifest host | TUF threshold signatures, metadata versions, snapshot consistency, timestamp expiry, persisted highest versions | Can withhold updates; cannot forge below threshold; expired metadata blocks new updates |
| DNS or TLS compromise | HTTPS is still required, but TUF—not the TLS endpoint—is artifact authority; redirects are signed/allowlisted and remain HTTPS | Network attacker can observe some metadata and deny service; certificate pinning is not required for artifact authenticity |
| Replaced RetroArch archive/core | Exact authenticated archive and installed-file hashes, platform/architecture and OS-signature checks | Does not detect malicious code already approved and signed by RetroFrontier's release threshold |
| Malicious upstream release/build | Human/release approval, source/build provenance, reproducible-build evidence where practical, threshold targets signing | Manifest signing records approval; it does not prove benign code or a clean upstream build system |
| One signing key compromised | 2-of-3 root and targets thresholds with independent custody; rotate/revoke the key | Compromise at threshold can authorize malware; root-threshold compromise requires out-of-band application recovery |
| Replay/rollback to vulnerable runtime | TUF metadata-version/expiry checks, persisted highest versions, signed monotonic release sequence, revocations and minimum-safe sequence | Offline client cannot know a revocation it has never received; wall-clock manipulation can affect expiry diagnostics |
| Malicious archive paths/types | Private staging; bounded parser; reject absolute, drive, UNC, traversal, hard links/reparse points, special files, duplicates, bombs, and unexpected executable paths; allow only exact authenticated internal symbolic links required by a reviewed format | Parser vulnerabilities remain possible; use a maintained reviewed extractor and synthetic hostile fixtures |
| Symlink/hard-link/junction race | User-private roots, descriptor/handle-relative operations, no-follow/reparse checks, unique directories, never overwrite, authenticated relative link targets contained inside the new tree | Same-user malware can still race or alter the process; OS sandboxing would be needed for a stronger guarantee |
| Different local unprivileged user modifies runtime | OS user-only permissions/ACLs plus signed inventory validation | Depends on correct app-data permissions and filesystem semantics |
| Same-user attacker or administrator modifies runtime | Reverify trusted manifests and native-code inventory immediately before use where practical | Cannot reliably defeat same-user process injection, trust-state deletion, TOCTOU, administrator/root, or kernel compromise |
| Interrupted download | `.part` data confined to staging; resume only with compatible validators; full target re-hash | Progress may be lost, but active runtime is unchanged |
| Interrupted extraction/verification | No completion marker; staging or incomplete version is never selectable | Requires bounded cleanup/retry, not journal replay |
| Crash/restart during finalization | Unique same-filesystem directory; completion marker written and flushed last | Power-loss durability varies by filesystem; startup revalidates |
| Crash/restart during activation or rollback | Flushed temp pointer, platform replacement primitive, directory durability, Windows backup inspection, startup validation | Windows APIs have documented partial failure layouts; real power/restart tests remain required |
| Partially written journal | No authoritative journal exists | Disposable per-staging resume metadata can be discarded without correctness impact |
| Stale/corrupt/missing `active.json` | Strict bounded parser, exact manifest/complete-marker validation, security floor, verified replacement backup only | Do not guess a version; availability becomes Broken/Repair Required while user data remains intact |
| Runtime manually modified by user | Signed full inventory and native-code recheck; repair reconstructs a new tree | Intentional same-user bypass is not preventable without stronger OS enforcement |
| Malicious or vulnerable native core | V1 managed allowlist, threshold approval, exact hashes, OS signing, no secrets in child environment, revocation | Core runs unsandboxed with the RetroArch process's user permissions and may read/write/delete files, spawn, or use network |

RetroFrontier can defend the authenticity of approved bytes below the signing threshold, detect many local static modifications, prevent unsafe archive writes, keep activation all-old/all-new for readers, recover conservatively from crashes, and ensure runtime maintenance never targets user-data paths. It cannot guarantee availability, prove native code is safe, learn revocations while offline, survive compromise of enough signing/application-update keys, or provide a security boundary against same-user malware, administrator/root, kernel compromise, or unrestricted native-core behavior.

## 16. Managed runtime versus user data

| Path/category | Ownership and update rule |
|---|---|
| `runtime/versions`, `runtime/staging`, `runtime/active.json`, runtime locks/process record | RuntimeManager-owned. Update/repair/rollback may mutate only these exact owned paths under lock. |
| `runtime-trust` | Application security state containing trusted TUF roots/history, highest versions, revocations, and security floors. Preserve across runtime uninstall, repair, rollback, and cache cleanup. |
| `runtime-user/config` and `runtime-user/core-options` | Durable RetroFrontier/emulator settings. Preserve across runtime operations; reset only through a separate explicit user action. |
| Approved default assets, shaders, autoconfig, databases, and core info | If release-controlled, place them in the immutable version or identify them as reconstructable generated projections. Do not ambiguously mix them with user overrides. |
| User overrides | Store in a clearly separate runtime-user subtree and never replace recursively during update or repair. |
| Cache and logs | Explicitly classified as regenerable; cleanup must target only their exact owned subtrees. |
| `saves`, `states`, `screenshots`, `metadata`, `database` | User/application data outside `runtime`; runtime update, repair, rollback, cleanup, and runtime uninstall never delete them. |
| `Documents/RetroFrontier/BIOS` | Canonical user-supplied BIOS source; read/validate only. A generated core-facing projection must be separately named and reconstructable, never the sole copy. |
| Managed/external ROM roots | User content. V1 runtime and library maintenance never rename, move, convert, or delete ROMs. |

The spike's `runtime-user/assets`, `shaders`, and `autoconfig` names are ambiguous because some files are approved release components while others may be user overrides. Implementation must split these ownership classes before cleanup exists. The BIOS `system_directory` is also ambiguous: pointing a native core directly at the canonical BIOS root exposes it to writes, while copying BIOS into a runtime-owned tree risks accidental deletion. Use a clearly user-data-owned or reconstructable projection and never let runtime deletion logic own the source BIOS files.

A managed-runtime uninstall removes only installed/staged runtime payloads and activation/process metadata after proving no managed game is running. It preserves `runtime-trust` as well as ROMs, BIOS, normal saves, save states, screenshots, metadata, SQLite, and durable user configuration. Removing trust state requires a separately named whole-application-data reset with explicit warning because it also removes rollback history.

## 17. Multiple instances and process coordination

The simplest robust V1 policy is one RetroFrontier instance per OS user. A second invocation forwards focus/open intent to the first and exits. This also avoids unnecessary SQLite/UI coordination, but single-instance UX is not the only correctness guard.

Use a separate OS-backed runtime mutation lock acquired by install, finalization, activation, rollback, repair, and cleanup. OS locks release on process death and do not need stale lock-file deletion. Launch briefly takes the same coordination lock, durably records a unique launch ID, exact installation, executable path, and a conservative `launching` phase before spawning, then atomically adds PID and process-start identity. Before mutation, validate that record against OS liveness. After a crash between spawn and PID persistence, remain conservatively busy until a platform process/path check proves no matching child exists. This blocks updates while an orphaned managed RetroArch child remains alive and avoids PID-reuse mistakes.

A second reader that opens `active.json` during replacement observes the old or new complete document. On Windows it opens with delete sharing and closes promptly. On every platform it resolves and retains that installation ID for its operation; it never repeatedly follows a mutable pointer mid-launch. SQLite may mirror runtime status for queries, but neither SQLite nor a UI state is activation authority.

## 18. Keep the security mechanisms separate

| Mechanism | Addresses | Does not address |
|---|---|---|
| HTTPS | Transport confidentiality/integrity and server authentication in transit | A compromised authenticated origin/CDN, replay of valid old metadata, signing-key compromise, local tampering |
| SHA-256 file hash | Exact-byte integrity against a trusted expected digest | Who approved the digest, freshness, malicious-but-approved code, OS launch policy |
| TUF/RetroFrontier metadata signing | RetroFrontier approval, key delegation/thresholds, metadata consistency, replay/freeze detection, artifact hashes | OS publisher identity/notarization, code safety, native-code containment, availability |
| Windows Authenticode/Smart App Control | Windows-recognized publisher identity, PE integrity/reputation/policy | RetroFrontier's runtime/core compatibility approval, channel freshness, rollback policy |
| macOS Developer ID/Gatekeeper/notarization/Hardened Runtime | Apple-recognized publisher, tamper checks, malware scanning/tickets, runtime loading policy | RetroFrontier release selection, component compatibility, CDN authenticity, TUF rollback policy |

Signing/notarizing RetroFrontier itself covers its shipped bundle only. It does not transitively sign, notarize, or approve executable files downloaded after installation.

## 19. Architecture verdict matrix

| Decision | Verdict | Required change or gate |
|---|---|---|
| Downloaded managed runtime | **ACCEPT WITH CHANGES** | TUF trust, platform code policy, managed-only paths, real platform proofs |
| Immutable version directories | **ACCEPT** | Unique installation IDs; final-path smoke test and revalidation; completion marker last; no mutation after completion |
| Private same-filesystem staging | **ACCEPT** | Safe extractor, format-scoped authenticated internal links only, no hard links/reparse points/special files, explicit resource limits |
| `active.json` activation authority | **ACCEPT WITH CHANGES** | Minimal three fields and ADR-011 write/read protocol |
| Authoritative transaction journal | **REJECT** | Filesystem-derived recovery; disposable per-staging resume metadata only |
| RetroFrontier-signed manifest | **ACCEPT WITH CHANGES** | Immutable canonical manifest as TUF target; no bespoke embedded signature |
| SHA-256 component verification | **ACCEPT WITH CHANGES** | Digest/size authenticated by TUF plus installed inventory/native-code checks |
| Runtime rollback | **ACCEPT WITH CHANGES** | Narrow automation, explicit manual rollback, signed revocation/security floor |
| Full reinstall repair | **ACCEPT** | Reconstruct exact release under a new installation ID; no in-place component repair |
| Approved/pinned cores | **ACCEPT WITH CHANGES** | Enforce managed approved cores only in V1 |
| Two-runtime retention | **ACCEPT WITH CHANGES** | Active plus one previous, with count and disk/free-space limits; temporary candidate allowed |
| Single-instance and runtime/game locks | **ACCEPT** | OS-backed mutation lock plus live child identity validation |
| Linux extracted-AppImage strategy | **REQUIRES EXPERIMENT** | Verify first, test `AppRun`, distro/graphics/audio/controller matrix, packaging interaction |
| Windows portable runtime strategy | **REQUIRES EXPERIMENT** | PE signing/reputation, antivirus/locks, long paths, pointer crash/restart behavior |
| macOS downloaded runtime strategy | **REQUIRES EXPERIMENT** | Developer ID/notarization/quarantine/library-validation proof on arm64 and x86_64; current blocker |

## 20. Remaining experiments

1. **macOS arm64 first:** execute the exact proof in section 14 on a clean real machine with realistic quarantine and production-style signing/notarization. This is the smallest blocker-clearing experiment.
2. **macOS x86_64:** repeat on real Intel hardware; verify exact core slices and no accidental Rosetta dependence.
3. **Windows x86_64:** standard non-admin account; intended installer and download API; Authenticode every PE; Defender/Smart App Control/SmartScreen behavior; process-tree locks; non-ASCII and long paths; active-pointer crash/restart injection; graphics/audio/controller/save smoke test.
4. **Linux x86_64 matrix:** oldest supported Ubuntu LTS, Debian stable, current Fedora; extracted `AppRun`; glibc baseline; Wayland/X11; PipeWire/PulseAudio; Intel/AMD/NVIDIA where practical; controller permissions; native RetroFrontier package. Flatpak requires a separate decision/spike.
5. **Trust/recovery fixture harness:** after the macOS architecture clears, evaluate a conforming Rust TUF client and key ceremony with synthetic artifacts only; hostile metadata/archives; interrupted phase and pointer replacement injection; signed rollback floors; full reconstruction repair; no production RuntimeManager or UI.
6. **Power/restart and multi-process tests:** all supported local filesystems, mutation lock contention, stale child identity/PID reuse, active pointer replacement, startup reconciliation, and cleanup confinement.

## 21. Sources

### RetroArch/libretro

- [Platform downloads](https://retroarch.com/?page=platforms)
- [Windows installation](https://docs.libretro.com/guides/install-windows/)
- [macOS installation](https://docs.libretro.com/guides/install-macos/)
- [Linux installation](https://retroarch.com/index.php?page=linux-instructions)
- [CLI documentation](https://docs.libretro.com/guides/cli-intro/)
- [Directory configuration](https://docs.libretro.com/guides/change-directories/)
- [Core downloads](https://docs.libretro.com/guides/download-cores/)
- [RetroArch repository/license](https://github.com/libretro/RetroArch)
- [RetroArch security](https://github.com/libretro/RetroArch/security)
- [Buildbot](https://buildbot.libretro.com/)
- [Core-info](https://github.com/libretro/libretro-core-info/)
- [Build recipes](https://github.com/libretro/libretro-super)

### Platform/security

- [AppImage running/extraction](https://docs.appimage.org/user-guide/run-appimages.html)
- [AppImage AppDir/AppRun specification](https://docs.appimage.org/reference/appdir.html)
- [AppImage cross-distribution testing guidance](https://docs.appimage.org/packaging-guide/testing.html)
- [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
- [The Update Framework specification](https://theupdateframework.github.io/specification/latest/)
- [RFC 8785 JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785.html)
- [RFC 8032 Ed25519](https://www.rfc-editor.org/info/rfc8032/)
- [Apple Developer ID](https://developer.apple.com/support/developer-id/)
- [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Apple custom/network-installer notarization](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
- [Apple Hardened Runtime](https://developer.apple.com/documentation/Security/hardened-runtime)
- [Apple disable-library-validation entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.cs.disable-library-validation)
- [Apple universal binaries](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary)
- [Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)
- [Microsoft SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Microsoft Attachment Execution Services save validation](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-iattachmentexecute-save)
- [Microsoft ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)
- [Microsoft MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
- [Microsoft FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
- [Microsoft moving/replacing files](https://learn.microsoft.com/en-us/windows/win32/fileio/moving-and-replacing-files)
- [Microsoft DLL updates](https://learn.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-updates)
- [Microsoft maximum path length](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation)
