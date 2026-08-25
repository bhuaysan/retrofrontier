# Linux Runtime Qualification

**Date:** 2026-08-25
**Scope:** Linux x86_64 only
**Verdict:** **READY WITH DOCUMENTED LIMITATIONS**

This spike qualifies the Linux managed RetroArch model sufficiently to begin a
Linux-only production `RuntimeManager` implementation. It does not qualify a
public Linux release, and it does not change the Windows or macOS architecture.
The remaining release gates are called out explicitly below.

## Executive decision

RetroFrontier should use an extracted RetroArch AppImage/AppDir as the managed
Linux x86_64 runtime. The production launch path is the AppDir-defined
`AppRun`, never a path inferred from the AppImage internals. The tested
RetroArch 1.22.2 artifact has an `AppRun` symlink to `usr/bin/retroarch`, but
that is an observation about this artifact, not a launch contract for every
future artifact.

The Linux filesystem model reviewed by Sol Max is supported by the evidence:

- immutable complete version trees and same-directory `active.json` replacement
- no authoritative transaction journal
- full-tree reconstruction for repair
- one active and at most one previous runtime by default
- an application lock and a separate runtime mutation lock
- a durable process identity record checked beyond PID alone
- an embedded trusted TUF root with persistent local trust metadata

The implementation may now begin behind Rust boundaries. Cross-distribution
testing, power-loss testing, controller mapping, native X11 desktop testing,
and final packaging remain release qualification work.

## 1. Tested Linux environment

The experiments ran on one physical workstation:

| Item | Observed value |
| --- | --- |
| Distribution | Fedora Linux 44, KDE Plasma Desktop Edition |
| Kernel | 7.1.9-200.fc44.x86_64 |
| Architecture | x86_64 |
| glibc | 2.43 |
| Session | KDE Wayland (`XDG_SESSION_TYPE=wayland`) with `DISPLAY=:0` XWayland |
| GPU | AMD Radeon RX 7800 XT |
| OpenGL | Mesa 26.1.7, direct rendering, OpenGL 4.6 |
| Vulkan | loader 1.4.341, Mesa RADV 26.1.7 |
| Audio | PipeWire 1.6.8 exposing a PulseAudio protocol server |
| Controller | Sony DualSense, visible through `/dev/input/event21` and `js1` |
| Tools | 7-Zip, `file`, `readelf`, `ldd`, `objdump`, `strace`, `vulkaninfo`, `glxinfo`, `pactl`, `pw-cli`, `udevadm`, `flock`, Rust/Cargo |

The host is deliberately recorded as one test point, not as a Linux support
claim. The extraction, pointer, lock, rollback, repair, and TUF tests used
synthetic or disposable data under `/tmp`. No downloaded runtime, core, ROM,
BIOS, signing key, or generated runtime payload is part of this repository.

## 2. RetroArch artifact and AppRun model

### Artifact

The test used the official RetroArch 1.22.2 Linux x86_64 ranged release and
one approved representative Nestopia core. The AppImage payload hash in the
disposable test tree was:

```text
RetroArch-Linux-x86_64.AppImage
794b0f65d4efa918e2ad05cac34b444a4f3207ed6c74834b7c14eb5fb15e1cc4
```

The release archive was unpacked into a disposable managed version tree. The
AppImage was then extracted into an AppDir containing:

```text
<version>/runtime/RetroArch-Linux-x86_64/squashfs-root/
├── AppRun -> usr/bin/retroarch
├── usr/bin/retroarch
└── usr/lib/...
```

The AppImage documentation defines `AppRun` as the AppDir entry point and
allows it to be an executable, script, or symlink. It also documents extraction
as a valid fallback when FUSE is unavailable. Those properties match the
managed-runtime requirement: extract once into a verified immutable tree and
launch the extracted AppDir entry point thereafter.

References: [AppDir specification](https://docs.appimage.org/reference/appdir.html),
[AppImage extraction and FUSE fallback](https://docs.appimage.org/user-guide/troubleshooting/fuse.html).

### AppRun experiments

The extracted `AppRun` was invoked with an absolute path from both the
repository directory and `/tmp`. Both runs used a clean child environment,
absolute paths, an explicit config, the absolute Nestopia core, a synthetic
legal iNES file, and a bounded frame count. Both exited successfully and wrote
screenshots and logs only below the RetroFrontier-owned disposable data root.

The verbose run confirmed the following exact resolutions:

- core: the managed absolute `nestopia_libretro.so`
- content: the managed absolute synthetic ROM
- system/BIOS: the explicit managed system directory
- saves and save states: the explicit managed directories
- screenshots and runtime log: the explicit managed directories
- core-info, core options, assets, cache, history, playlists, thumbnails,
  remaps, and autoconfig: explicit managed paths or disabled

An untrusted `XDG_CONFIG_HOME` containing a hostile RetroArch configuration did
not affect the run. Changing the working directory did not affect the run.

The tested AppRun is a symlink, so it did not establish shell environment
variables or perform wrapper-script setup. The executable's ELF `RUNPATH` is
`$ORIGIN/../lib`, and the loader selected bundled libraries where present.
That is evidence about this artifact only. A future AppRun may be a script or
another executable and may establish environment variables. Therefore:

1. authenticate and inventory `AppRun` and all allowed in-tree link targets;
2. launch `AppRun` as the only Linux entry point;
3. preserve the artifact's AppDir-relative assumptions;
4. pass all RetroFrontier paths explicitly; and
5. do not infer or replace AppRun with `usr/bin/retroarch` in production code.

The direct AppImage invocation also encountered the host's lack of a usable
FUSE helper. This reinforces the extracted-AppDir choice: the managed runtime
does not require FUSE, a temporary mount, or a privileged mount operation at
game launch.

### AppRun-dependent behavior

| Behavior | Depends on AppRun? | Qualification result |
| --- | --- | --- |
| Selecting the AppDir entry point | Yes | Verified; `AppRun` resolves to the payload in this artifact |
| AppDir-relative executable/library layout | Yes, through the launched payload and its location | Verified; `$ORIGIN/../lib` selected bundled libraries |
| Shell environment setup | Artifact-dependent | None observed in this symlink AppRun; do not assume none for future artifacts |
| RetroArch config/core/save/system paths | No implicit AppRun setup is required | Verified through explicit CLI/config paths |
| Working-directory independence | No special cwd was required by the test | Verified from two working directories |

## 3. Host compatibility boundary

`readelf`, `ldd`, `objdump`, and `LD_DEBUG=libs` were used on the executable
and bundled ELF files. The main executable advertises at most `GLIBC_2.22`
and `GLIBCXX_3.4.22` in the inspected symbol versions. That is not a complete
distribution floor because the runtime still uses host graphics, audio,
device, and service infrastructure.

| Area | Classification | Evidence and implication |
| --- | --- | --- |
| RetroArch/AppDir payload libraries | Bundled | The AppDir contains RetroArch's `liblzma`, SDL2, PulseAudio client, several X11/Wayland support libraries, udev, V4L2, and their bundled transitive libraries. The executable's runpath is `$ORIGIN/../lib`. |
| ELF loader and glibc | Host-required | `/lib64/ld-linux-x86-64.so.2`, libc, libm, libmvec, librt, libpthread, libdl, and resolver/NSS behavior come from the host. |
| C++ runtime | Host-required | `libstdc++.so.6` and `libgcc_s.so.1` were resolved from the host. |
| Fonts and text stack | Host-required | FreeType/fontconfig and host transitive dependencies were required. |
| ALSA and JACK | Host-required/optional | `libasound.so.2` and `libjack.so.0` were host libraries. JACK resolved through the Fedora/PipeWire compatibility stack on this host. They are needed only when the selected audio path uses them. |
| PulseAudio client | Bundled client, host service | The AppDir supplied `libpulse`, but the PulseAudio protocol socket/server was host-provided. PipeWire's `pipewire-pulse` compatibility layer worked. |
| Wayland | Mixed | Wayland EGL/cursor pieces were bundled, while `libwayland-client.so.0` and the compositor/session were host-provided. |
| X11/XWayland | Host-required | X11, XCB, GLX, Xinerama, XRandR, Xi, Xss, and the X server/XWayland session were host-provided. |
| OpenGL/EGL/GBM/DRM | Host-required | `libGL`, `libEGL`, GBM/DRM, Mesa or a vendor implementation, and the GPU device were host-provided. Direct AMD OpenGL worked. |
| Vulkan | Host-required and optional | The executable has no direct `libvulkan.so.1` NEEDED entry and loads Vulkan dynamically. The Vulkan loader, ICD, driver, and GPU access remain host requirements when Vulkan is selected. RADV worked. |
| udev/input | Bundled client plus host devices/policy | The AppDir supplied a udev client library and SDL2, but `/dev/input`, udev metadata, ACLs, and device permissions were host-controlled. |
| Video4Linux2 | Bundled client, optional host devices | V4L2 support was present; no camera qualification was attempted. |
| Audio/video services | Host-required | Session sockets, compositor, PulseAudio/PipeWire service, and device nodes cannot be made self-contained by extracting the AppDir. |
| NVIDIA or unusual vendor stacks | Unknown | No NVIDIA proprietary stack, non-RADV Vulkan ICD, or hybrid-GPU configuration was tested. |

The tested artifact therefore has a useful old glibc symbol baseline, but its
practical boundary is “modern 64-bit glibc host with the expected desktop,
graphics, audio, and udev services.” Runtime validation must report missing
host dependencies separately from a corrupt managed installation.

## 4. Wayland and X11

### Wayland

Under the real KDE Wayland session, RetroArch created a Wayland EGL/GL window
using the AMD GPU, rendered at 2560x1440, detected the udev controller, and
exited cleanly after a bounded synthetic run. A separate fullscreen run entered
fullscreen at the display's native resolution and exited cleanly with a
screenshot.

### X11/XWayland

With Wayland disabled for the child, `SDL_VIDEODRIVER=x11`, `DISPLAY=:0`, the
host `XAUTHORITY`, and `video_context_driver="glx"`, RetroArch created an X11
GLX context through the current session's XWayland server. It detected the
controller and exited cleanly. This is an X11/XWayland result, not a native
standalone X11 desktop result.

The environment/configuration inputs that selected the backend were
`WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, `DISPLAY`, `XAUTHORITY`, SDL's video
driver variable, and RetroArch's context-driver setting. No platform-specific
RetroFrontier hack is justified. The launch context should preserve the
desktop's required display variables while controlling only the paths and
security-sensitive variables RetroFrontier owns.

Not verified here:

- focus returning to an actual RetroFrontier window after game exit;
- native X11 desktop rather than XWayland;
- multiple monitors and compositor-specific fullscreen behavior;
- controller button mapping during a game.

These are later integration/manual test cases, not reasons to change the
runtime model.

## 5. Audio

The host exposes PipeWire 1.6.8 through a PulseAudio protocol server. With
the bundled PulseAudio client and an explicit PulseAudio socket, RetroArch
initialized audio, reported a negotiated buffer, ran for bounded frames, and
exited successfully. This validates the common modern Linux path of a
PulseAudio-compatible client connecting to PipeWire.

A separate standalone PulseAudio daemon was not tested. ALSA and JACK remain
host dependencies if those backends are selected. RetroFrontier should use a
single internal default policy, not expose Linux audio backend selection as a
normal user setting.

If the audio server/socket is absent or audio initialization fails, the
runtime should not be marked corrupt. The launch result should carry a clear
“audio unavailable” diagnostic and follow the product policy for degraded
launch (prefer a visible warning and continued video when RetroArch can run,
with a retry path). The earlier restricted-sandbox run demonstrated that
RetroArch can continue without audio while logging initialization failure;
that is a diagnostic state, not successful audio qualification.

## 6. Controller input

A real DualSense was visible through RetroArch's `udev` joypad driver:

```text
[udev] Pad #0 (/dev/input/event21) supports force feedback
[Input] Found joypad driver: "udev"
```

The host supplied user ACLs (`seat:uaccess`) on the event and joystick nodes.
The user was not relying on membership in the `input` group. This establishes
the expected permission boundary: the managed runtime can bundle SDL/udev
clients, but it cannot grant access to `/dev/input` or repair host udev policy.

The isolated runtime had no host controller profile leakage. RetroArch reported
the DualSense as detected but not configured. Buttons and axes were not
manually exercised, so input mapping is **partially verified, not fully
verified**. A production runtime must ship or select only approved controller
profiles and must surface an actionable unmapped-controller state. It must not
silently depend on a user's unrelated system RetroArch configuration.

## 7. Safe extraction proof

A disposable archive proof under `/tmp` created synthetic tar archives and
validated archive listings before extraction. It tested the following policy:

| Archive case | Result |
| --- | --- |
| Normal file and directory tree | Accepted |
| `../` traversal | Rejected |
| Absolute member path | Rejected |
| Absolute symlink target | Rejected |
| Relative symlink escaping the root | Rejected |
| Duplicate path | Rejected |
| File/directory path conflict | Rejected after normalized path conflict |
| Excessive declared expansion (1 GiB sparse declaration against a 1 MiB limit) | Rejected |
| Corrupt archive | Rejected by archive test/read failure |

The proof was a disposable preflight harness, not production extraction code.
The production primitive must additionally:

- authenticate the release and expected file inventory before trusting it;
- reject hard links, device nodes, FIFOs, sockets, and other special files;
- bound entry count, path length, expanded bytes, compression ratio, and nesting;
- create files relative to a trusted destination using descriptor/handle-safe
  operations where the selected Rust archive library supports them;
- validate link targets both lexically and after resolution, allowing only the
  exact format-approved in-tree links such as this AppDir's `AppRun`;
- avoid extracting over an existing directory; and
- validate the completed tree and executable identity before finalization.

No production `RuntimeManager` or reusable extractor was added in this spike.

## 8. `active.json` Linux protocol

The synthetic protocol used complete A and B installation trees, a completion
marker, an authenticated release-manifest digest, and the exact three-field
active pointer schema already defined by the architecture. The writer:

1. created a temporary file in the same directory as `active.json`;
2. wrote the complete JSON and flushed/closed it;
3. reopened and validated the temporary pointer and its target installation;
4. renamed it over `active.json` within the same directory; and
5. opened and fsynced the parent runtime directory.

Observed results:

| Scenario | Result |
| --- | --- |
| Initial pointer to A | Passed |
| Activate validated B | Passed; readers saw B |
| Restart/reopen after activation | Passed |
| Corrupt `active.json` | Rejected |
| Pointer to missing target | Rejected |
| Incomplete/stale staging directory | Ignored by activation; cleanup-only |
| Crash before rename | Old A remained authoritative; orphan temp remained disposable |
| Crash after rename | Complete new pointer was visible; startup validation passed |
| 160 simultaneous renames/readers | Passed; no torn JSON or invalid target observed |
| Journal files | None; only `active.json` was authoritative |

This supports the Sol-reviewed conclusion that filesystem state plus
`active.json` is sufficient for V1 on Linux. The protocol is crash-consistent
for the tested process failures and concurrent readers. It is not a proof
against sudden power loss, storage-device write caching, network filesystems,
or unusual filesystems. V1 must keep the same-filesystem requirement, use
file flush plus directory fsync where available, validate at startup, and keep
power-loss testing as a release gate. No transaction journal should be added.

**`active.json` verdict: accepted for Linux V1 with the documented durability
limit.**

## 9. Mutation lock and managed-game identity

The disposable Linux proof used advisory `flock(2)` locks on stable files:

- one application lock for one RetroFrontier instance per user;
- one runtime mutation lock for install, activation, rollback, repair, and
  cleanup.

Contention rejected the second holder. Killing a holder released the lock in
the kernel, and a new holder acquired it without stale-lock deletion. Stable
lock files remain as lock targets; their presence does not mean they are
held.

The synthetic game-process proof recorded a launch identity containing an
installation id, absolute executable path, PID, and Linux process start time.
It checked:

- `/proc/<pid>` exists;
- `/proc/<pid>/exe` resolves to the expected executable;
- `/proc/<pid>/stat` start time matches the recorded start time; and
- the installation path still matches the managed version being protected.

PID reuse with a different start time and a process-path mismatch were both
rejected. This is sufficient for local single-user desktop software without
distributed locking. A conservative `launching` record should block mutation
after an application crash until the process identity is proven absent.

The second-instance UX (forwarding focus/open requests to the first instance)
is application integration work. The locking decision itself is accepted.

## 10. Rollback and retention

Synthetic A/B/C runtime trees exercised:

1. A current;
2. B downloaded, staged, and validated;
3. B activated;
4. restart/reopen on B;
5. rollback to A; and
6. rejection of a tampered candidate and a candidate with a missing component.

Activation failures left A current. A full reconstruction produced A-prime,
validated it, activated it, and left the broken A untouched until deferred
cleanup.

### Recommended V1 defaults

- retain at most **two complete runtime installations**: active plus one
  previous known-good installation;
- allow one operation-specific staging/candidate tree, deleted after success
  or failure;
- enforce an internal **2 GiB logical runtime storage ceiling** for complete
  versions plus active staging, configurable without changing the data model;
- refuse an update when the candidate and required working-space reservation
  cannot fit; and
- never delete the active tree to make space.

The ceiling is intentionally conservative for V1 and should be adjusted only
from measured signed runtime sizes. Save states remain outside runtime trees
and carry runtime/core identity; they do not justify unlimited runtime
retention.

## 11. Full reconstruction repair

The synthetic repair flow was:

```text
broken A
  -> construct a new verified A-prime from the approved release
  -> validate the complete tree and manifest
  -> activate A-prime
  -> remove broken A later when no process uses it
```

It passed with the broken A left unchanged. This is simpler and safer for V1
than component-level repair: it avoids mixing versions, makes validation
identical to installation validation, and leaves a clear recovery point until
cleanup. Component-level repair is not part of the production scope.

## 12. TUF Rust-client feasibility

### Recommendation

Use the maintained Rust `tough` client, currently documented as version 0.24.0
at the time of this qualification, rather than designing a custom signing or
metadata protocol. `tough` states that it implements TUF 1.0.0, supports local
filesystem transport, persistent metadata storage, target length/hash
verification, threshold signatures, and an embedded out-of-band trusted root.
The project is published under MIT OR Apache-2.0 and its upstream test suite
includes sequential root-rotation coverage; the disposable fixture here used
one root and did not itself exercise rotation.
Its documented limitation is that delegated roles/TAP 3 and TAP 4 are not
implemented. RetroFrontier can use top-level `root`, `targets`, `snapshot`, and
`timestamp` roles for its initial runtime/core release profile, so that
limitation is not an architecture blocker.

Relevant references:

- [`tough` 0.24.0 crate documentation](https://docs.rs/tough/latest/tough/)
- [`RepositoryLoader` trusted root and persistent datastore](https://docs.rs/tough/latest/tough/struct.RepositoryLoader.html)
- [`Repository` target verification and safe target saving](https://docs.rs/tough/latest/tough/struct.Repository.html)
- [AWS Labs `tough` repository and license](https://github.com/awslabs/tough)
- [`tough` root-rotation integration test](https://raw.githubusercontent.com/awslabs/tough/develop/tough/tests/rotated_root.rs)
- [TUF specification roles and threshold model](https://github.com/theupdateframework/specification/blob/master/tuf-spec.md)

The official [`rust-tuf`](https://github.com/theupdateframework/rust-tuf) project
is worth tracking, but its README still describes the implementation as beta
with an unstable API. It is not the lower-risk first production dependency.

### Disposable fixture

A synthetic local repository was created with three Ed25519 keys:

- root threshold: 2 of 3;
- targets threshold: 2 of 3;
- snapshot and timestamp thresholds: 1;
- trusted root supplied out-of-band;
- metadata and target served through local filesystem URLs.

The `tuftool` fixture successfully verified metadata and downloaded the signed
synthetic target. After changing the target bytes without changing signed
metadata, the download failed with a TUF hash mismatch. The fixture therefore
proved trusted root -> signed metadata -> threshold target verification and a
negative tamper case. It was run with a disposable `tuftool` 0.16 installation
using `tough` 0.23 dependencies; neither was added to the repository.

Before production integration, pin the then-current patched `tough` release,
review its security advisories and transitive crypto/TLS dependencies, and add
tests for root rotation, threshold failure, expiry, rollback floors, target
path safety, offline metadata, and corrupted local datastore state. Use the
client's persistent datastore for timestamp/snapshot/targets anti-rollback
state and keep the trusted root embedded or otherwise protected by an
out-of-band application update process.

## 13. Core runtime test

Only the approved Nestopia representative core was used. RetroArch was passed
the absolute managed core path and the isolated config's core directory was
also managed. The verbose launch log identified that exact core path and the
synthetic content path. No system `retroarch` executable or unrelated system
core was selected, even when the child environment contained only a minimal
`PATH`.

The production policy remains:

- allow only cores listed by the authenticated Runtime Release;
- pass an absolute approved core path at launch;
- keep managed core files inside the immutable version tree or its authenticated
  release location; and
- do not discover arbitrary system cores as a fallback.

This is a path-selection qualification, not the final V1 core matrix.

## 14. Proposed Linux distribution matrix

RetroFrontier should publish a deliberately small matrix rather than claim
every distribution. The minimum pre-release matrix should be:

| Environment | Why it is included | Required coverage |
| --- | --- | --- |
| Ubuntu 22.04 LTS amd64 | Older-glibc Ubuntu-family anchor while still supported through May 2027 according to the Ubuntu release cycle | Clean install, Wayland/XWayland, native X11, PipeWire/Pulse, extraction, launch, update, rollback |
| Debian 13 “trixie” amd64 | Current Debian stable and a different packaging/service baseline; its `libc6` is glibc 2.41 | Same runtime matrix plus filesystem/udev/audio checks |
| Fedora current stable amd64 | Fedora/PipeWire/SELinux and the development host family; Fedora 44 uses glibc 2.43 | KDE/GNOME Wayland, XWayland, native X11, GPU/audio/controller checks |
| Arch Linux pinned snapshot amd64 | Rolling-family behavior and newer glibc/graphics stack; the observed package page lists glibc 2.44 | Pinned image, not “latest at random”; repeat launch/update and device checks |

The current official references identify Ubuntu 22.04 and 24.04/26.04 LTS
support windows, Debian 13 as stable, Fedora 44 as released, Debian trixie
`libc6` 2.41, Fedora 44 glibc 2.43, and Arch's current x86_64 glibc package.
See [Ubuntu's release cycle](https://ubuntu.com/about/release-cycle),
[Debian releases](https://www.debian.org/releases/),
[Debian trixie libc6](https://packages.debian.org/en/trixie/libc6),
[Fedora 44 release](https://fedoramagazine.org/announcing-fedora-linux-44/),
[Fedora glibc packages](https://packages.fedoraproject.org/pkgs/glibc/glibc/),
and [Arch glibc](https://archlinux.org/packages/core/x86_64/glibc/).

Debian stable materially improves confidence in distribution diversity and is
worth keeping in the matrix, but it is not an old-glibc floor by itself.
Ubuntu 22.04 is the more useful older-glibc anchor for this release decision.
The inspected RetroArch binary's `GLIBC_2.22` symbol ceiling does not remove
the need to test host graphics/audio/udev stacks. Do not advertise support for
systems older than the selected matrix without a separate product decision and
test point.

Later CI/VM/manual work is required for all four environments, both display
session types, at least one Intel or NVIDIA graphics path where practical,
PipeWire and a real PulseAudio service, controller profiles, upgrade/restart,
power-loss simulation, and a real frontend-to-game focus cycle. Containers are
useful for archive/TUF/filesystem tests but cannot substitute for desktop
device qualification.

## 15. Linux packaging implications

No final RetroFrontier packaging format is selected by this spike.

- A native `.deb`/`.rpm` package provides the simplest host integration for
  filesystem access, child process execution, udev, graphics, and audio. It
  still needs a cross-distribution application-data layout and must not use a
  system RetroArch.
- An AppImage for RetroFrontier is compatible with the extracted managed
  runtime model, provided the managed RetroArch AppImage is extracted and
  verified rather than requiring nested FUSE at launch. Its own AppImage/FUSE
  behavior remains packaging QA.
- Flatpak is materially different. By default it has no host file access,
  device nodes, X11, PulseAudio, or host services. A useful package would need
  explicit filesystem grants for external ROM roots and runtime data,
  `--device=dri` for graphics, `--device=input` for controllers, PulseAudio
  and Wayland/X11 sockets, network access for downloads, and a policy for
  executing and updating native managed runtime content inside the sandbox.
  The official permission reference documents these defaults and grants.

See [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html).
Flatpak should be deferred from the initial Linux release unless a separate
sandbox architecture is deliberately designed and tested. It should not
contaminate the normal native Linux runtime design.

## 16. Remaining blockers and limitations

These do not block beginning the Linux `RuntimeManager`, but they block a
public Linux release claim until closed:

1. The artifact was exercised on one Fedora 44 host. The proposed Ubuntu,
   Debian, Fedora, and Arch matrix still needs VM/CI/manual runs.
2. Native standalone X11, frontend focus return, multi-monitor behavior, and
   compositor-specific fullscreen behavior remain unverified.
3. Controller discovery and permissions passed, but button/axis mapping and
   profile behavior were not validated with gameplay.
4. PipeWire's PulseAudio compatibility path passed; a separate PulseAudio
   daemon and other GPU vendor stacks were not tested.
5. The active-pointer proof covered process crashes and concurrent readers,
   not physical power loss or hostile/network filesystems.
6. Production safe extraction and authenticated file-inventory code still need
   implementation and focused security review.
7. The `tough` dependency and key-custody/root-rotation ceremony still need a
   production version pin, audit, and conformance/security regression suite.
8. Final Linux packaging and the signed, approved RetroArch/core artifact
   publication process remain open.

There is no evidence requiring a transaction journal, component-level repair,
system RetroArch fallback, platform-specific display hack, or Windows/macOS
architecture change.

## 17. Exact next production task

Begin a focused Linux-only implementation task:

> Implement the Rust Linux `RuntimeManager` foundation around an extracted,
> authenticated RetroArch AppDir. Add the safe extraction primitive and
> installed-file inventory, `tough`-backed trust adapter, immutable version
> lifecycle, `active.json` atomic replacement, Linux `flock` locks, process
> identity checks, bounded retention, full reconstruction repair, and an
> explicit AppRun launch context with one approved synthetic/legal core fixture.

Keep Tauri commands thin, do not implement Windows/macOS adapters, do not add
the final UI, and do not ship production runtime binaries or cores in the
repository. The implementation should land with synthetic tests for the
failure cases recorded in this report and should not declare public Linux
support until the distribution/device matrix and power-loss gate are complete.

## Qualification conclusion

**READY WITH DOCUMENTED LIMITATIONS.** The extracted AppImage/AppDir plus
AppRun model, explicit-path launch contract, Linux active-pointer protocol,
advisory locking strategy, rollback/retention policy, full reconstruction
repair, and TUF implementation direction are sufficiently qualified to start
the Linux production `RuntimeManager`. The evidence is not sufficient to claim
that every Linux distribution, GPU, audio service, controller, or packaging
format is supported.
