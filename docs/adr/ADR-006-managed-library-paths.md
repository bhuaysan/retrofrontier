# ADR-006: Managed user library under Documents/RetroFrontier
- Status: Accepted

## Context
Zero configuration needs a visible place for user ROM/BIOS files.

## Decision
Default to OS-resolved `Documents/RetroFrontier/ROMs` and `Documents/RetroFrontier/BIOS`. Allow external ROM roots.

## Consequences
Easy discovery and clean separation from app/runtime data. Resolve Documents through platform APIs, not hard-coded paths.
