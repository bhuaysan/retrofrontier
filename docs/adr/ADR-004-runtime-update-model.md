# ADR-004: Versioned runtime updates with rollback
- Status: Accepted

## Context
In-place executable updates can leave a broken runtime after interruption/incompatibility.

## Decision
Use pinned versioned Runtime Releases, staging, integrity verification, validation, activation only when no game runs, and rollback. Never blindly track upstream latest.

## Spike refinement
Use uniquely identified immutable version directories, private same-filesystem staging, completion markers, and an app-owned active pointer rather than a symlink or junction. Fully validate, finalize, and smoke-test a candidate before acquiring the runtime mutation lock. Under the lock, retain only the current installation and candidate before replacing the pointer, so the former current installation becomes the sole rollback candidate without separate previous-state metadata. No authoritative transaction journal is used in V1; startup derives recovery from staging contents, completion markers, installed manifests, the active pointer, and an optional non-authoritative replacement backup.

Retain the active release and at most one previous known-good release, subject to a byte budget and minimum-free-space policy. A temporary candidate may make three trees during an update. Refuse an update if active plus candidate cannot fit; never delete the active tree to make room. V1 repair reconstructs a complete approved release into a new immutable directory rather than mutating or mixing components in place. Verified download-cache reuse is allowed, but the resulting tree must exactly match one approved manifest.

Normal game saves and emulator save states remain external to runtime versions. Normal saves do not require retaining an emulator version. Save states record core/runtime identity because compatibility is not guaranteed, but they do not justify unlimited retention or rollback to a release barred by authenticated security policy.

Trusted update metadata, highest observed security floors, and revocations survive runtime uninstall, repair, rollback, and cache cleanup. They are removed only by an explicit whole-application-data reset.

The Linux proof supports the directory model but does not prove power-loss durability or platform-specific activation behavior. TUF trust metadata is defined by ADR-012. macOS signing/notarization and real Windows/macOS pointer tests remain release gates.
