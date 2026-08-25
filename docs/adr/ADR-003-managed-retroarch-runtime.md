# ADR-003: Managed downloaded RetroArch runtime
- Status: Accepted

## Context
Existing RetroArch installs may contain unknown versions/configs/cores. Bundling RetroArch couples installer and runtime.

## Decision
Do not bundle RetroArch. Download/manage an isolated runtime after installation. Launch only that runtime with explicit RetroFrontier-controlled paths/config.

## Consequences
Predictable zero-config behavior and smaller installer, but first-run networking and secure update/distribution become required.

## Spike refinement
The managed runtime is a versioned, immutable release selected by an app-owned active pointer. The launch contract resolves an absolute executable and an approved managed core, and explicitly controls RetroArch configuration, core-info, system/BIOS, save, state, screenshot, asset, cache, history, option, and log paths. Symlinks and junctions are not required.

V1 loads only cores authenticated as part of an approved managed Runtime Release. Arbitrary external cores and custom RetroArch runtimes remain out of V1 because cores execute unrestricted native code in the RetroArch process.

This decision is experimentally supported on one Linux x86_64 host using an extracted official AppImage payload. Windows portable archive behavior is documented but untested. macOS runtime and core signing, quarantine, notarization, architecture matching, and library validation are a production security gate. The managed-runtime decision is therefore accepted, while each platform distribution adapter remains subject to its proof gate.
