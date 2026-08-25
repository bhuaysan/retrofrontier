# Managed RetroArch Runtime Spike Report

- **Date:** 2026-08-25
- **Status:** Research and disposable proof complete; production RuntimeManager not implemented
- **Model:** GPT Luna Max; GPT Sol Max review required for open security and macOS decisions

## 1. Executive conclusion

**Conclusion: viable with platform-specific adaptations.**

RetroFrontier can manage an isolated RetroArch runtime without bundling RetroArch in its installer and without using a system RetroArch installation. Linux x86_64 was experimentally proven with an official versioned archive, an extracted AppImage payload, explicit config/core/content paths, synthetic content, and a hostile existing RetroArch configuration.

This is not a uniform four-platform production approval. Windows has a suitable documented portable archive but needs a real Windows test. macOS has suitable DMG/app artifacts but native core loading, quarantine, notarization, and hardened-runtime/library-validation behavior remain unresolved. Linux has no catch-all distribution and needs an AppImage extraction/FUSE policy. HTTPS plus a hash is insufficient if the manifest and hash come from the same compromised source.

The evidence supports the existing architecture and refines it to immutable version directories, an app-owned pointer file, a transaction journal, explicit control of all RetroArch paths, full-candidate V1 repair, platform adapters, and a RetroFrontier-signed manifest. No updater, extractor, RuntimeManager, UI, or runtime binary was added to the repository.

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
| Windows x86_64 | Versioned RetroArch.7z; stable index also exposes setup.exe and RetroArch_cores.7z. Extract the archive into managed storage. Official docs describe archive and installer distributions as portable/self-contained. | Strong candidate with absolute executable and pointer activation. | File locks, antivirus/SmartScreen reputation, child process, and controllers untested. Documented, not locally verified. |
| macOS arm64 | Stable RetroArch_Metal.dmg is universal; nightly arm64 runtime/core indexes exist. Copy the app bundle into a version directory. | Feasible in principle with explicit Contents/MacOS/RetroArch and config. | Gatekeeper, quarantine, notarization, hardened runtime, library validation, and .dylib core loading unresolved. Real Apple Silicon test required. |
| macOS x86_64 | Stable universal Metal DMG and separate x86_64 DMG; x86_64 nightly cores exist. | Same pointer/config model is feasible in principle. | Same signing/core risks; Rosetta matters on Apple Silicon. Real Intel Mac test required. |
| Linux x86_64 | Versioned RetroArch.7z with AppImage; official options also include AppImage Qt, Snap, Flatpak. Extract AppImage or run where FUSE is available. | Extracted AppImage payload, core, synthetic content, and controlled paths verified locally. | Host graphics/audio/kernel/desktop dependencies and FUSE/sandbox behavior. |

Stable indexes reviewed: [Windows](https://buildbot.libretro.com/stable/1.22.2/windows/x86_64/), [Linux](https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/), [macOS universal](https://buildbot.libretro.com/stable/1.22.2/apple/osx/universal/), and [macOS x86_64](https://buildbot.libretro.com/stable/1.22.2/apple/osx/x86_64/).

Base runtime archives do not imply a complete core library. Official indexes expose separate core archives and individual packages: .dll.zip on Windows, .dylib.zip on macOS, and .so.zip on Linux. The [core-info repository](https://github.com/libretro/libretro-core-info/) supplies display version, system, license, and firmware metadata but not necessarily a stable binary build identity. RetroArch is GPLv3; core licenses vary. Core directories such as nightly/<platform>/latest/ are mutable evidence sources, never production release IDs.

## 4. Isolation and recommended launch strategy

RetroArch’s [CLI documentation](https://docs.libretro.com/guides/cli-intro/) supports an explicit config path and explicit -L core path. Its implicit defaults include Linux/macOS XDG/home paths and Windows locations near the executable and in APPDATA. Managed launches must never depend on them.

Rust should read runtime/active.json, validate and canonicalize a release below runtime/versions, and resolve an absolute executable:

~~~~text
Windows: versions/<release-id>/runtime/retroarch.exe
macOS:   versions/<release-id>/runtime/RetroArch.app/Contents/MacOS/RetroArch
Linux:   versions/<release-id>/runtime/squashfs-root/usr/bin/retroarch
~~~~

Launch with an argument vector containing an explicit managed config, managed log, managed core, and absolute content path. Use a controlled working directory, never the ROM directory. Construct the child environment rather than inheriting it blindly; remove unrelated RetroArch variables and review LD_PRELOAD and DYLD_*. LIBRETRO_* variables are defense in depth, not the authority.

Explicitly control, as applicable: libretro_directory, libretro_info_path, system_directory, savefile_directory, savestate_directory, screenshot_directory, assets_directory, core_assets_directory, video_shader_directory, playlist_directory, content_database_path, thumbnails_directory, input_remapping_directory, overlay_directory, joypad_autoconfig_dir, cache_directory, log_dir, runtime_log_directory, history paths, and core_options_path. Disable or redirect the core-info cache. This is path isolation, not a sandbox; the [RetroArch security policy](https://github.com/libretro/RetroArch/security) says cores can read/write/delete files, spawn processes, and use the network.

## 5. Recommended runtime layout and active reference

~~~~text
<app-data>/RetroFrontier/
├── runtime/
│   ├── versions/
│   │   ├── <release-a>/
│   │   │   ├── runtime/                 # immutable executable/support files
│   │   │   ├── cores/                   # pinned release-compatible cores
│   │   │   ├── core-info/
│   │   │   ├── licenses/
│   │   │   └── installed-manifest.json
│   │   └── <release-b>/
│   ├── staging/<operation-id>/
│   ├── transactions/<operation-id>.json
│   ├── active.json
│   ├── previous.json
│   └── locks/
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

| Reference mechanism | Finding |
|---|---|
| Symlink | Easy on Unix, but Windows privilege/dev-mode policy, reparse-point resolution, and security handling complicate a common authority. |
| Junction | Windows-specific directory/reparse semantics; does not carry a manifest hash or transaction generation. |
| Renamed active directory | Harder crash recovery and encourages in-place coupling. |
| Database-only field | Useful indexed state, but insufficient as filesystem activation authority. |
| Small pointer file | Same model on every platform; can carry generation and manifest identity and be replaced in one directory. |

Use runtime/active.json as the authority; a database active-version field is derived. Do not require symlinks or junctions. Validate that the release ID is safe, resolves below versions, and matches the installed manifest:

~~~~json
{
  "schema_version": 1,
  "generation": 4,
  "release_id": "rf-1.22.2-linux-x86_64-20260825",
  "manifest_sha256": "…",
  "activated_at": "2026-08-25T00:00:00Z"
}
~~~~

## 6. Draft runtime manifest

This is a draft schema, not a final service or signature protocol:

~~~~json
{
  "schema_version": 1,
  "manifest_id": "rf-runtime-2026-08-25-linux-x86_64",
  "channel": "stable",
  "issued_at": "2026-08-25T00:00:00Z",
  "expires_at": "2026-11-25T00:00:00Z",
  "min_retrofrontier_version": "0.1.0",
  "release": {
    "release_id": "rf-1.22.2-linux-x86_64-001",
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
  },
  "signature": {
    "algorithm": "placeholder",
    "key_id": "retrofrontier-release-key-1",
    "value": "placeholder-over-canonical-manifest"
  }
}
~~~~

The moving core URL and null source revision intentionally show what production must fix: use an immutable approved artifact or RetroFrontier snapshot and record a build/revision identity where available. The final schema must also support platform/architecture, exact sizes, SHA-256 or stronger hashes, mirrors, expected roots, executable paths, file policies, component versions, support assets, licenses, compatibility, expiry/revocation, and signed canonical serialization.

## 7. Integrity, authenticity, and security

| Property | Provides | Does not provide |
|---|---|---|
| HTTPS | Encrypted transport and endpoint authentication | Proof that a compromised approved endpoint serves approved content |
| SHA-256 | Difference/corruption detection against a trusted expected hash | Publisher authenticity when the hash came from the same compromised source |
| Signed manifest | Authentication of approved hashes/metadata when the key is independently trusted | Safety from stolen keys, bad approval, or malicious native cores |
| Upstream signature | Upstream identity, if available and verified | RetroFrontier approval or compatibility |
| RetroFrontier signature | RetroFrontier approval of the artifact set | That upstream code is benign |

The [RetroArch security page](https://github.com/libretro/RetroArch/security) says RetroArch/core binaries are not signed and that cores have broad capabilities. Reviewed stable indexes exposed artifacts but no established upstream signature sidecars.

Confirmed requirements: never use upstream latest as a release identity; verify HTTPS transfer, size, digest, archive policy, and manifest before activation; treat cores as native code; keep keys/secrets/binaries out of the repository; preserve licenses.

Recommended: embed a RetroFrontier public-key trust root in a signed application release; verify a signed canonical manifest; design rotation, expiry, revocation, emergency recovery, and offline behavior; prefer immutable approved snapshots; keep candidates immutable and switch only the pointer.

Open decisions requiring GPT Sol Max review:

1. Trust-root distribution, rotation/revocation, recovery, and rollback semantics.
2. Whether RetroFrontier mirrors/re-signs upstream artifacts or redistributes them unchanged.
3. macOS app/core signing, notarization, quarantine, and Hardened Runtime library validation.
4. Threat model and acceptable permissions for third-party cores.
5. Hosting behavior for expired/revoked manifests.
6. Redistribution license/notice obligations.

If manifest and hashes come from the same compromised endpoint, an attacker can change both and pass SHA-256. HTTPS does not close that gap.

## 8. Safe extraction requirements

Future extraction must use a new operation directory below runtime/staging and reject absolute, drive-letter, UNC, and traversal paths after normalization; canonicalize and contain destinations; reject or quarantine symlinks, hard links, junctions, reparse points, device files, sockets, and FIFOs; reject unexpected duplicates; enforce compressed-size, expanded-size, entry-count, and compression-ratio limits; allow executable destinations only from a manifest allowlist; validate a fresh tree before installation; reject truncated/corrupt archives; use resumable bounded downloads; and clean incomplete staging without touching an active version or user data.

Use synthetic archives to test traversal, absolute paths, links, duplicates, oversize declarations, compression bombs, malformed headers, and interrupted writes. No production extractor was written.

## 9. Atomic activation, state machine, and recovery

~~~~text
resolve approved manifest
  -> resumable download
  -> verify length/hash/signature
  -> extract fresh staging tree
  -> validate files/allowlist/platform
  -> write installed manifest and health record
  -> ensure no game/RetroArch process is running
  -> atomically replace active.json
  -> bounded smoke test
  -> mark healthy
  -> retain previous release
~~~~

Use immutable version directories. Replace only the pointer in the same directory: POSIX write/flush/rename plus parent fsync where supported; Windows ReplaceFileW or MoveFileExW with replacement/write-through and retries; macOS signed bundles validated and never modified in place. This is near-atomic, not a universal power-loss guarantee; journal, generation, backup pointer, and startup reconciliation remain required.

~~~~text
NotInstalled -> Downloading -> Staging -> Verifying -> Validating -> Ready
Ready -> UpdateAvailable -> Updating -> SmokeTesting -> Ready
Ready -> Broken -> Repairing -> Staging
SmokeTesting -> RollbackAvailable -> RollingBack -> Ready
Interrupted transaction -> RecoveryPending -> Ready or Broken
~~~~

States: NotInstalled, Downloading, Staging, Verifying, Validating, Ready, UpdateAvailable, Updating, SmokeTesting, RollbackAvailable, RollingBack, Broken, Repairing, and RecoveryPending.

Startup reads the journal and pointer, discards incomplete staging, ignores candidates without a completed health record, verifies the active installed manifest, restores the previous valid pointer if activation failed, and marks Broken only when no valid release remains. It never deletes ROMs, BIOS, saves, states, metadata, SQLite, or user config.

## 10. Rollback and repair

V1 should keep the active and at least one previous known-good release, retain up to two when disk permits, and delete only versions unreferenced by processes, journals, recovery records, or sessions. Automatic rollback is for activation/smoke-test health failure; user-triggered rollback covers other cases. Do not auto-rollback merely because one game crashes. Save-state metadata should record runtime/core identity, but rollback never deletes or migrates user data.

For V1 repair, use a full reinstall of the exact approved release into fresh staging. Verify the installed manifest, download missing or complete approved components, extract/validate a complete candidate, activate normally, and retain the old release until smoke testing succeeds. Verified component reuse may reduce bandwidth internally, but activation must not create a mixed-version tree.

## 11. Platform risks

### Windows x86_64

The official portable .7z model fits app-data management better than a traditional installer. Use absolute paths, same-volume version directories, no in-place DLL/executable updates, and wait for the complete RetroArch process tree. Microsoft documents SmartScreen publisher/file-hash reputation and recommends signing releases; RetroArch reports its binaries are unsigned. Test antivirus quarantine, locks, child processes, XInput, and controllers on real Windows.

**Most important Windows finding:** portable distribution is viable on paper, but file locking and reputation controls make pointer activation essential; none was locally verified.

### macOS arm64/x86_64

The official model is DMG copy of RetroArch.app; stable Metal is universal and architecture-specific nightly core indexes exist. Apple’s [Developer ID](https://developer.apple.com/support/developer-id/), [notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution?changes=_9), [Hardened Runtime](https://developer.apple.com/documentation/Security/hardened-runtime), and [universal binary](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary) guidance make separately downloaded native cores a first-class issue. Quarantined Internet plugins on modern macOS require notarization, and library validation can reject arbitrary plugins.

Keep the app bundle immutable after signing. Test first launch, quarantine, core loading, universal/Intel/Rosetta behavior, update, rollback, and core signature/notarization on both real architectures.

**Most important macOS finding:** independently downloaded .dylib cores are a release blocker until the signing/notarization/library-validation strategy is proven.

### Linux x86_64

Official guidance says there is no catch-all build. Extracted-AppImage execution passed locally; direct AppImage execution failed for lack of FUSE. [AppImage documentation](https://docs.appimage.org/user-guide/run-appimages.html) supports extraction. The payload still depends on host kernel, graphics, audio, display, and devices. If RetroFrontier later ships as Flatpak, [Flatpak permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html) require explicit filesystem, network, display, audio, DRI, and input decisions.

Test Fedora, Ubuntu, a Debian-family system, X11/Wayland, graphics/audio, and controllers before claiming V1 breadth.

**Most important Linux finding:** extracted AppImage execution works here, but V1 must choose extraction versus FUSE and must not promise distro-independent graphics/audio behavior.

## 12. Core-management implications

This does not finalize CORE_MATRIX.md; only Nestopia was used. Cores are independent platform-specific native packages; exact archive hash and build/revision identity are required; .info metadata is useful but not sufficient to identify a binary; support files belong in explicit system/BIOS mappings; cores have broad permissions. After Sol review, test one representative synthetic-content core per platform packaging family. Do not commit core collections.

## 13. Recommended architecture changes

Evidence supports these refinements:

1. Replace generic active-runtime-reference with runtime/active.json, a transaction journal, and previous-pointer recovery. Symlinks/junctions are not required.
2. Make versions immutable and move runtime-user config/options/cache/logs outside them.
3. Add platform adapters for Windows portable archives, macOS app/signing validation, and Linux AppImage extraction/FUSE.
4. Add explicit core-info-cache, core-options, libretro-info, history, cache, and runtime-log controls to the RetroArch launch contract.
5. Treat manifest signatures, archive policy validation, candidate health, startup reconciliation, and rollback as RuntimeManager responsibilities.
6. Use full-candidate reinstall for V1 repair.
7. Keep macOS signing/core loading and manifest authenticity open pending Sol review.

## 14. Recommended next implementation task

After GPT Sol Max review, implement a fixture-backed Runtime Release transaction harness, not a user-facing updater: parse/validate the draft manifest; verify synthetic archive size/hash; test safe extraction; install and recover active.json; simulate interrupted download/extraction/activation, failed smoke test, rollback, and full repair; and run on Windows/macOS runners when available.

## 15. Sources

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
- [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
- [Apple Developer ID](https://developer.apple.com/support/developer-id/)
- [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution?changes=_9)
- [Apple Hardened Runtime](https://developer.apple.com/documentation/Security/hardened-runtime)
- [Apple universal binaries](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary)
- [Microsoft SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Microsoft ReplaceFile](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilea)
- [Microsoft MoveFileEx](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexa)
- [Microsoft moving/replacing files](https://learn.microsoft.com/en-us/windows/win32/fileio/moving-and-replacing-files)
- [Microsoft DLL updates](https://learn.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-updates)
