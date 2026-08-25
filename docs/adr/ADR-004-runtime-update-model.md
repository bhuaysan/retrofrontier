# ADR-004: Versioned runtime updates with rollback
- Status: Accepted in principle

## Context
In-place executable updates can leave a broken runtime after interruption/incompatibility.

## Decision
Use pinned versioned Runtime Releases, staging, integrity verification, validation, activation only when no game runs, and rollback. Never blindly track upstream latest.

## Open Detail
Final authenticity/signing and platform-specific atomic activation remain after the runtime spike.

## Spike refinement
Use immutable version directories, an operation journal, and an app-owned active pointer file rather than requiring a symlink or junction. Replace only the small pointer after candidate validation and no-game checks, then run a bounded smoke test. Startup reconciles interrupted transactions and can restore the previous valid pointer.

Keep at least one previous known-good release and retain up to two when disk permits. V1 repair should reinstall a complete approved candidate into fresh staging rather than create a mixed-version partial repair. Runtime rollback never deletes or migrates ROMs, BIOS, saves, save states, metadata, or the database.

The Linux proof supports this model, but does not prove power-loss durability or platform-specific activation behavior. Manifest authenticity, macOS app/core signing and quarantine, and real Windows/macOS tests remain open.
