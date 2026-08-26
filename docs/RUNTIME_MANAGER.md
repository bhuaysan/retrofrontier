# Linux RuntimeManager

This document describes the M2 implementation boundary. It is implementation documentation, not
a replacement for the runtime research spikes or the runtime ADRs.

## Scope

M2 implements the Linux x86_64 RuntimeManager foundation. RetroArch is not bundled, and the
application does not use a `retroarch` executable from `PATH`. The application currently ships
with no production release URL or signing root configuration; an approved source is injected by
the application composition root when that infrastructure is available. Tests use synthetic
local targets.

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

The Linux process abstraction uses managed-process record schema version 2. It records PID,
`/proc` start-time ticks, the Linux boot ID, authenticated AppRun path, and the observed executable
path for script-based AppRun support. A dead, PID-reused, or pre-reboot process is treated as stale.
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
only `get_runtime_status` over IPC. The shell shows a small design-system-compatible runtime
status value and does not implement an updater/settings screen.

## Review markers and deferred work

The code contains focused Sol Max review markers for TUF trusted-root lifecycle, extraction,
activation durability, process identity, and cleanup ownership. Production key ceremony/release
hosting, real RetroArch/AppImage integration, executable smoke execution, Windows/macOS adapters,
core policy expansion, game launch, and runtime UI remain outside M2.
