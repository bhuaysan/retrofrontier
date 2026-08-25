# ADR-004: Versioned runtime updates with rollback
- Status: Accepted in principle

## Context
In-place executable updates can leave a broken runtime after interruption/incompatibility.

## Decision
Use pinned versioned Runtime Releases, staging, integrity verification, validation, activation only when no game runs, and rollback. Never blindly track upstream latest.

## Open Detail
Final authenticity/signing and platform-specific atomic activation remain after the runtime spike.
