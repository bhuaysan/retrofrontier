# Linux RuntimeManager

This document describes the M2 implementation boundary. It is implementation documentation, not
a replacement for the runtime research spikes or the runtime ADRs.

## Scope

M2 implements the Linux x86_64 RuntimeManager foundation. RetroArch is not bundled, and the
application does not use a `retroarch` executable from `PATH`.

**M7.5 update.** The composition root now configures a trusted release source, so installation is
reachable in practice. `RuntimeManager::for_app` takes an optional `TrustedReleaseSource`; when
none is configured it keeps the deliberately failing `UnavailableTrustedReleaseSource`, so "no
approved source" stays a trust refusal rather than an absent capability. There is still no
*production* release URL or shipped signing root — that is M10 — but a real, TUF-authenticated
Linux x86_64 release now exists and installs through this exact code path. See
[`M7_5_RUNTIME_QUALIFICATION.md`](M7_5_RUNTIME_QUALIFICATION.md).

## Ownership and paths

`RuntimePaths` is constructed from the Tauri `app_data_dir()`. Its owned tree is:

```text
<app-data>/RetroFrontier/
├── runtime/
│   ├── versions/<opaque-installation-id>/
│   ├── staging/<operation-id>/
│   ├── locks/runtime-mutation.lock
│   ├── locks/application.lock
│   ├── active.json
│   └── game-process.json
└── runtime-trust/
    ├── tuf/
    └── trust-state.json
```

The database, metadata, logs, saves, states, screenshots, ROM roots, and BIOS roots are not
RuntimeManager-owned paths. Cleanup accepts only a direct child of `runtime/versions` with a
valid `SafeIdentifier`, and refuses symlink/non-directory targets before deletion. The `versions`
and `staging` roots are revalidated as real directories while holding the runtime mutation lock
immediately before enumeration; a relocated or symlinked root produces a repair-required state
and is never recursively deleted.

Runtime-owned directories are created with user-only `0700` permissions. Pointer, trust, process,
lock, manifest, and completion state files are created with user-only `0600` permissions.

## State

The public status is derived on every read from the pointer, trusted state, completion markers,
and verified installed trees:

| State | Source |
| --- | --- |
| `NotInstalled` | No active pointer and no complete installation |
| `Ready` | Pointer resolves to a trusted, complete, inventory-verified installation |
| `RollbackAvailable` | The active installation is valid and another trusted, verified installation has a strictly lower release sequence |
| `Broken` | Pointer, target, trust state, completion marker, or installed inventory is invalid |

`Installing`, `Updating`, and `Repairing` are domain vocabulary for future operation reporting;
they are transient and are not persisted as UI state. No runtime transaction journal is used.

## Release and trust boundary

`RuntimeManifest` is a strict, versioned RetroFrontier release type. It validates Linux/x86_64
compatibility, safe identifiers and paths, component target/installation uniqueness, exact
archive sizes and SHA-256 values, approved core mappings, AppRun location, licenses, extraction
limits, and the complete installed inventory. JSON duplicate keys and unknown fields are rejected.

`TrustedReleaseSource` separates RuntimeManager from network I/O. `ToughTrustedReleaseSource`
keeps TUF verification behind that boundary using the maintained `tough` client, authenticated
targets, safe metadata expiration, a persistent datastore, and a required authenticated,
versioned runtime-policy target. Construction validates the supplied trusted root before a source
can be used. `LocalTrustedReleaseSource` is limited to explicit synthetic/development fixtures
but preserves the same manifest/target size/hash relationships.

The local `RuntimeTrustState` retains authenticated release digests, metadata versions, the
highest observed release floor, and revocations. It is separate from TUF metadata and survives
runtime cleanup. This is a persistence aid for anti-rollback decisions, not a custom signing
protocol.

## Dependencies

The implementation uses maintained ecosystem crates for security-sensitive primitives. `tough
0.24.0` (MIT OR Apache-2.0) provides TUF 1.0 verification and persistent metadata rollback
protection; `fs4 1.1.0` (MIT OR Apache-2.0) provides the Linux kernel advisory lock; `sha2 0.11`
(MIT OR Apache-2.0) provides SHA-256; `tar 0.4.46` (MIT OR Apache-2.0), `zip 8.6.0` (MIT),
`sevenz-rust2 0.22.0` (Apache-2.0), and `backhand 0.25.1` (MIT OR Apache-2.0) provide format
parsers/readers. `async-trait 0.1`, `futures 0.3`, and `url 2` are small MIT OR Apache-2.0
ecosystem dependencies used for the source boundary and TUF streams. The archive, lock, and TUF
versions are pinned in `src-tauri/Cargo.toml`; no custom cryptographic, archive, or locking
primitive was added.

## Installation flow

1. Resolve and validate an approved release through `TrustedReleaseSource`.
2. Download each target into an operation-specific staging directory using a bounded stream,
   exact target identity, size, and SHA-256 verification.
3. Extract only into a private empty staging tree. Linux AppImages are read as SquashFS without
   executing the AppImage runtime; a 7z outer container may unwrap only its declared payload.
4. Reject traversal, absolute paths, special nodes, hard links, unsafe links, duplicates,
   conflicting parents, excessive entries/sizes/ratios, and unexpected inventory types.
5. Apply the authenticated executable modes, write the release manifest, verify the complete tree,
   and perform structural AppRun smoke validation.
6. Move the verified tree to a new opaque `versions/<installation-id>` directory and write
   `complete.json` last. The directory is immutable after completion.
7. Under the mutation lock, recheck process/pointer conditions and atomically replace `active.json`.

Repair follows the same full reconstruction path and creates a new installation; it never patches
the damaged directory in place.

## Pointer and coordination

`active.json` contains exactly `schema_version`, `installation_id`, and `manifest_sha256`. The
Linux writer uses a unique same-directory temporary file, flush/sync, close/reopen/parse/compare,
atomic rename, parent-directory fsync, and final reopen/validation. Corrupt or missing pointers
are never resolved by choosing the newest directory.

The runtime mutation lock is a kernel advisory `flock` on a stable file and releases with process
exit. Application startup also takes a separate application lock, while the runtime lock remains
mandatory for defense in depth.

The Linux process abstraction uses managed-process record schema version 3. M7 added the launch
identifier and the play-session identifier, and made the process identity optional so a conservative
`launching` record can be written before the child is spawned; ADR-011 requires that pre-spawn
record because the window between `exec` and durably persisting a PID is where a crash could
otherwise leave a live managed RetroArch that no safety check knows about.

Validation is phase-specific. A `running` record must carry PID, `/proc` start-time ticks, and the
observed executable path (which supports a script-based AppRun, where `/proc/<pid>/exe` is the
interpreter); a `launching` record must carry none of them, so a fabricated identity cannot be
mistaken for a real one. The authenticated AppRun path and the Linux boot ID are required in both
phases.

Liveness fails closed in both phases. A record from a previous boot cannot describe a live process
and is cleared. A `running` record is decided by boot ID, start-time ticks, and canonical
`/proc/<pid>/exe` equality, so a dead or PID-reused process is stale while an identity mismatch stays
uncertain and blocking. A `launching` record has no PID by construction, so it is decided by a
bounded `/proc` scan that matches an executable resolving inside `runtime/versions/` or *any*
command-line element naming the authenticated AppRun. Matching the whole command line rather than
`argv[0]` is what makes a `#!` script AppRun detectable: Linux runs the interpreter instead, so the
executable is outside the managed tree and the AppRun appears as an interpreter argument. The scan
deliberately over-detects, because a false positive only keeps mutation blocked while a false
negative would let an update run underneath a live emulator.
PID alone is never identity.

An old, newer, or otherwise incompatible schema is not deleted: it is treated as uncertain,
blocks runtime mutation, and makes startup report `Broken`/repair-required until an explicit
recovery path handles it. A regular record with no recognizable supported schema that is truncated
or otherwise unparseable may be quarantined and cleared; a live or identity-mismatched process
remains blocking. Startup reports `Broken`/repair-required for reconciliation failures so the UI
remains usable while mutation paths continue to enforce the safety check.

Startup reconciliation removes owned pointer/process temporary and leftover process-quarantine
files in `runtime/`, trust-state temporary files in `runtime-trust/`, staging operations, and
incomplete non-active version directories while holding the mutation lock. It then derives status
from the authoritative pointer and verification results.

## Retention and rollback

Defaults retain two verified installations and cap logical runtime storage at 2 GiB. Before a
download starts, the manager reserves the declared archive bytes, expanded inventory bytes, and
metadata working space in addition to current runtime staging/version usage. It rechecks measured
usage under the mutation lock before activation. With an authoritative current pointer, retention
is normalized before pointer replacement so only the current installation and verified candidate
remain; if cleanup cannot complete, activation aborts before the pointer changes. Cleanup after a
successful pointer replacement is housekeeping and is logged/deferred rather than reported as an
activation failure: once `active.json` is durably replaced, the candidate is active and activation
has succeeded. The active installation and one verified fallback are preserved. Cleanup never
deletes the active runtime, the selected fallback, an unsafe path, or anything outside the versions
root. If pointer state is not authoritative, complete installations are preserved for explicit
repair rather than guessed or deleted.

Rollback is monotonic. `rollback()` selects the highest-sequence trusted and fully verified
installation whose release sequence is strictly lower than the active release. It never rolls
forward to a newer retained installation. Status uses this exact same eligibility predicate, so
`canRollback` is true if and only if `rollback()` can select a candidate. After rolling back, a
newer installation may remain as the retained fallback, but it is not rollback-eligible; status
therefore reports `Ready` unless another eligible lower-sequence installation exists.

## Application boundary

The Tauri command calls `RuntimeApplicationService`, which calls `RuntimeManager`; filesystem,
TUF, download, extraction, process, and SQLite details do not appear in the command. M2 exposes
`get_runtime_status` over IPC. M3/M4 expose the read-only `verified_snapshot()` boundary through
`RuntimeApplicationService`: it returns core component IDs only from the active installation after
the same authenticated manifest, completion-marker, and installed-inventory verification used by
runtime status. It does not decide which systems approve a core.

`SystemsApplicationService` consumes one snapshot alongside the application-owned system catalog.
The catalog's policy and the runtime's installed availability remain separate, so a listed core is
not considered usable until RuntimeManager verifies its managed component. BIOS discovery is a
separate Rust service over the user-owned Documents/RetroFrontier/BIOS root and is never included
in runtime cleanup or update ownership.

## Verified runtime snapshot

`RuntimeManager::verified_snapshot()` performs one trust-consistent active-installation verification
and returns both the effective `RuntimeStatus` and the verified core component IDs from that same
installation. `RuntimeApplicationService` exposes this boundary, and `SystemsApplicationService`
consumes one snapshot for both system runtime status and core availability. The compatibility method
`current_verified_core_ids()` delegates to the snapshot and does not create a second verification
algorithm. Trust decisions are still recomputed on each snapshot request; no indefinite trust cache
was introduced.

## Launch boundary

M7 added two entry points and no new responsibility. `verified_launch_runtime()` performs the same
single trust-consistent active-installation verification as `verified_snapshot()` — pointer, trust
state, manifest, completion marker, installed inventory — plus AppRun validation, and returns the
absolute authenticated AppRun, the absolute path and release-declared systems of every authenticated
core component, and the absolute paths of support-asset components such as Dolphin's `Sys`. Runtime
status, installed core availability, and launch paths therefore never come from separate
verifications. A runtime that is not `Ready`/`RollbackAvailable` has no launch target at all, and a
core component with no authenticated executable is omitted rather than guessed at.

`lock_for_launch()` hands `LaunchApplicationService` the existing OS-backed runtime mutation lock, so
ADR-011's serialization of game launch and runtime mutation is enforced by that one lock rather than
a parallel mechanism. The launch service holds it from before verification until the durable
`running` record is committed, which closes the verification-to-spawn window against a concurrent
activation. `ensure_no_active_game()` exposes the same process-record check mutations already use.

RuntimeManager still owns installation, update, repair, rollback, trust, activation, installed-tree
verification, and managed-runtime mutation safety. Launch context construction, core resolution,
content resolution, configuration, environment, spawning, and monitoring live in `RetroArchService`
and `LaunchApplicationService`; see [`docs/RETROARCH_LAUNCH.md`](RETROARCH_LAUNCH.md).

## Review markers and deferred work

The code contains focused Sol Max review markers for TUF trusted-root lifecycle, extraction,
activation durability, process identity, and cleanup ownership. Production key ceremony/release
hosting, real RetroArch/AppImage integration, executable smoke execution, Windows/macOS adapters,
and runtime UI remain outside M2. Core policy for the four M7 reference systems and the Linux game
launch boundary have since landed; core policy for the remaining seven V1 systems is still open.


## Installation surface (M7.5)

`RuntimeApplicationService` owns the application-facing installation boundary:

| Operation | IPC command | Notes |
| --- | --- | --- |
| read state | `get_runtime_install_state` | status plus whether a source is configured, its origin, and the approved release target |
| install | `install_runtime` | installs the single approved manifest target this build is configured for |
| repair | `repair_runtime` | full reconstruction into a fresh immutable installation |

Installation is single-flight inside the process, on top of the cross-process
`RuntimeMutationLock`, so a second request reports `installationInProgress` instead of blocking an
IPC worker on a kernel lock. Anticipated problems are normalized codes rather than IPC errors, and
none of their messages carries a path, `errno`, or OS text. The runtime's real status accompanies
every response, so a failed install can never make the UI believe an already-installed runtime
disappeared.

React never supplies a manifest target, URL, executable, or core path. The approved release target
comes from the configured source alone.

## Release construction

Runtime Releases are produced by the maintainer-only `rf-runtime-release` tool behind the
non-default `release-tools` cargo feature, so signing and publication code never ships in the
application binary. It derives the authenticated installed inventory from each artefact and then
proves it by extracting every component through the production extractor and running this
module's own `verify_tree` and `validate_app_run`. See
[`M7_5_RUNTIME_QUALIFICATION.md`](M7_5_RUNTIME_QUALIFICATION.md).

### AppImage extraction

`find_squashfs_offset` validates a SquashFS 4.0 superblock at every candidate `hsqs` offset rather
than accepting the first magic match. The official RetroArch AppImage runtime embeds a literal
`hsqs`/`sqsh`/`shsq`/`qshs` signature table well before the real filesystem, so the naive scan
found the wrong offset and every real AppImage failed to extract.
