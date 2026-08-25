# ADR-003: Managed downloaded RetroArch runtime
- Status: Accepted

## Context
Existing RetroArch installs may contain unknown versions/configs/cores. Bundling RetroArch couples installer and runtime.

## Decision
Do not bundle RetroArch. Download/manage an isolated runtime after installation. Launch only that runtime with explicit RetroFrontier-controlled paths/config.

## Consequences
Predictable zero-config behavior and smaller installer, but first-run networking and secure update/distribution become required.

## Spike refinement
The managed runtime is a versioned, immutable release selected by an app-owned active pointer. The launch contract resolves an absolute executable and explicitly controls RetroArch configuration, core, core-info, system/BIOS, save, state, screenshot, asset, cache, history, option, and log paths. Symlinks and junctions are not required.

This decision is experimentally supported on Linux x86_64 using an extracted official AppImage payload. Windows portable archive behavior is documented but untested here. macOS runtime and core signing/quarantine behavior remains an explicit release gate.
