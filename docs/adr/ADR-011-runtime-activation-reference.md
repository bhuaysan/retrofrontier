# ADR-011: Runtime activation pointer and filesystem recovery
- Status: Accepted

## Context
RetroFrontier must switch between replaceable RetroArch runtime installations without overwriting running executable code, relying on platform-specific link privileges, or exposing a partially written selection to concurrent readers. Crashes and restarts can happen during every update phase.

Immutable version trees and a single small selection file make most recovery state observable. A separate authoritative transaction journal would duplicate that state, introduce ordering questions between two mutable authorities, and require its own corruption protocol.

## Decision
Use `runtime/active.json` as the sole activation authority. It contains exactly:

```json
{
  "schema_version": 1,
  "installation_id": "01J6RUNTIME7Q4M5N8P2X3Y9Z0AB",
  "manifest_sha256": "<64 lowercase hexadecimal characters>"
}
```

`installation_id` is a safe opaque directory basename, not a path and not a semantic version. It allows a damaged release to be reconstructed under a new identifier without overwriting the old tree. The manifest digest identifies the exact canonical release manifest. Release version, platform, architecture, activation time, previous release, health, and update generation are obtained from authenticated installed metadata or application state and do not belong in the pointer.

Candidates are built in a private operation-specific directory on the same local filesystem. A candidate is finalized under a unique `runtime/versions/<installation_id>` path while still incomplete, smoke-tested from that exact path with all writable outputs redirected, and revalidated against its authenticated inventory. Close and flush materialized files/directories as the platform permits, then commit a small completion marker with the same temporary-file, flush, replace, and parent-directory durability pattern used for the pointer. The marker is the last mutation before the version becomes immutable. A missing/partial marker or failed startup inventory check leaves the installation incomplete even if a prior write appeared successful.

Activation then follows this protocol:

1. Acquire the OS-backed exclusive runtime mutation lock.
2. Revalidate the current pointer and candidate, and confirm no live managed RetroArch process exists.
3. Before replacement, preserve only the current selection and candidate among complete installations. Safely remove any older inactive installation or abort if retention cannot be normalized. After a successful switch, the former current installation is therefore the sole rollback candidate without a journal or previous pointer.
4. Serialize a bounded, strict pointer document to a uniquely named file in the same directory as `active.json`; create it without following links and with user-only permissions.
5. Write the complete contents, flush the file, close it, reopen it, and strictly parse and compare it with the intended value.
6. On Linux, atomically rename it over `active.json`, then `fsync` the runtime directory. On macOS, use the same rename protocol plus the strongest supported file flush (`F_FULLFSYNC` where appropriate) and directory synchronization.
7. On Windows, flush and close the temporary file; use `ReplaceFileW` for an existing pointer with a same-volume backup name, without relying on its unsupported write-through flag, or use same-volume `MoveFileExW` with `MOVEFILE_WRITE_THROUGH` for first install. Retry only bounded sharing/antivirus races. Inspect and validate every possible result on failure because `ReplaceFileW` documents non-uniform failure states.
8. Reopen and validate the resulting `active.json` before reporting success. A platform replacement backup is recovery evidence only, never a second activation authority.
9. Release the mutation lock.

The pointer reader opens a fresh handle, enforces a 4 KiB maximum and strict UTF-8/JSON/schema rules, rejects duplicate or unknown fields, and validates the basename. Starting from the expected private app-data/runtime root, it resolves the `versions` and installation-directory components by handle without following symbolic links, junctions, or reparse points. It then verifies the manifest digest and completion marker and enforces the freshest locally known security floor before returning an authenticated launch path; format-approved internal bundle links are governed separately by the authenticated inventory. On Windows, readers share read/write/delete access and close the pointer handle promptly so a concurrent replacement does not fail merely because it is being read. A reader resolves and retains one installation ID for the duration of its operation rather than following the pointer again mid-launch.

## Recovery
- A partial download or extraction exists only in staging and is resumable after full re-hash or disposable.
- A version directory without a valid completion marker is incomplete and never selectable.
- A complete inactive version is harmless and may be offered as an approved rollback candidate or cleaned later. Pre-activation retention normalization makes the prior current installation the sole inactive complete version after a successful switch.
- Atomic pointer replacement exposes either the old or new complete installation to concurrent readers.
- A missing, malformed, oversized, stale-below-policy, or target-mismatched pointer is never resolved by guessing the newest directory. Startup may restore a verified replacement backup; otherwise it reports a broken runtime and offers repair or an explicit approved rollback.
- Rollback uses the same pointer protocol, so a crash exposes either complete selection.

No authoritative transaction journal or durable `previous.json` is used in V1. Disposable resume metadata may live inside its staging operation directory.

## Coordination
RetroFrontier is single-instance per OS user in V1. A separate OS-backed runtime mutation lock protects update, activation, rollback, repair, and cleanup from accidentally concurrent processes. Game launch and mutation are serialized under that lock. Launch writes a conservative `launching` record with a unique launch ID, installation ID, and executable path before spawning, then atomically completes it with PID and process-start identity. A restarted RetroFrontier treats an incomplete launch record as busy until platform liveness/process-path checks prove no matching child exists. This closes the app-crash window between spawn and PID persistence without making the record an update authority.

## Consequences
- Activation uses one logical authority on Windows, macOS, and Linux.
- The active executable and native libraries are never modified in place.
- Filesystem structure is sufficient for correctness recovery; there is no journal/pointer ordering problem.
- POSIX durability, Windows replacement/antivirus behavior, and game-process liveness queries still require real-platform crash and restart tests.
- Runtime-user configuration, ROMs, BIOS, normal saves, save states, metadata, SQLite, screenshots, and logs remain outside replaceable versions.
