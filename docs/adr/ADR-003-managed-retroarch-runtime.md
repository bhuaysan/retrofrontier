# ADR-003: Managed downloaded RetroArch runtime
- Status: Accepted

## Context
Existing RetroArch installs may contain unknown versions/configs/cores. Bundling RetroArch couples installer and runtime.

## Decision
Do not bundle RetroArch. Download/manage an isolated runtime after installation. Launch only that runtime with explicit RetroFrontier-controlled paths/config.

## Consequences
Predictable zero-config behavior and smaller installer, but first-run networking and secure update/distribution become required.
