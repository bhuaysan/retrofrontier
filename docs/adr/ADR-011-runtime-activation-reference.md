# ADR-011: Runtime activation pointer and recovery journal
- Status: Proposed

## Context
RetroFrontier must switch between replaceable RetroArch runtime versions without overwriting a running executable, depending on platform-specific link privileges, or losing the active version after a crash or power interruption.

The managed runtime spike found that a small cross-platform reference can carry release identity and manifest identity, while symlink/junction behavior differs by platform. It also found that a candidate needs a transaction record and health result so startup can distinguish a complete release from interrupted staging.

## Proposed decision
Use an app-owned runtime/active.json pointer as the activation authority. The pointer contains a schema version, monotonically increasing generation, safe release ID, manifest identity, and activation timestamp. Store operation journals and a previous valid pointer under runtime/transactions and runtime/previous.json.

Runtime versions are immutable after validation. Candidates are built under a new same-filesystem staging directory. After integrity, archive-policy, platform, and prerequisite validation, and after all game processes exit, replace the small pointer using the platform's same-directory atomic or near-atomic file replacement primitive. Run a bounded smoke test and record candidate health before deleting any older version.

The database may mirror the active release for queries, but it is not the filesystem activation authority. Symlinks and junctions are optional optimizations and are not required for correctness.

## Consequences
- Activation has the same logical model on Windows, macOS, and Linux.
- The active executable and its DLLs or app bundle are never modified in place.
- Startup can discard incomplete staging and restore a previous valid pointer.
- A journal, pointer backup, generation checks, and filesystem durability limitations must be tested.
- Runtime-user configuration, saves, states, metadata, SQLite, ROMs, and BIOS stay outside replaceable versions.

## Open questions
- Exact Windows replacement/retry behavior under antivirus file locks.
- POSIX directory durability guarantees across supported filesystems.
- macOS signing and quarantine validation for the app bundle and downloaded cores.
- Whether the final security review requires additional anti-rollback or manifest policy fields.
