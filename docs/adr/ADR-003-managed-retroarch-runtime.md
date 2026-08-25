# ADR-003: Managed downloaded RetroArch runtime
- Status: Accepted

## Context
Existing RetroArch installs may contain unknown versions/configs/cores. Bundling RetroArch couples installer and runtime.

## Decision
Do not bundle RetroArch. Download/manage an isolated runtime after installation. Launch only that runtime with explicit RetroFrontier-controlled paths/config.

## Consequences
Predictable zero-config behavior and smaller installer, but first-run networking and secure update/distribution become required.

## Spike refinement
The managed runtime is a versioned, immutable release selected by an app-owned active pointer. The launch contract resolves an absolute executable and an approved managed core, and explicitly controls RetroArch configuration, core-info, system/BIOS, save, state, screenshot, asset, cache, history, option, and log paths. On Linux x86_64, the absolute executable is the authenticated extracted AppDir `AppRun` entry point; the implementation must not substitute an inner payload path. Symlinks and junctions are not required for activation pointers, but format-approved in-tree links such as an AppDir `AppRun` must be authenticated and validated.

V1 loads only cores authenticated as part of an approved managed Runtime Release. Arbitrary external cores and custom RetroArch runtimes remain out of V1 because cores execute unrestricted native code in the RetroArch process.

This decision is experimentally supported on one Fedora 44 Linux x86_64 host using an extracted official AppImage/AppDir and its `AppRun` entry point. Linux host-library/device boundaries and cross-distribution behavior remain release gates. Windows portable archive behavior is documented but untested. macOS runtime and core signing, quarantine, notarization, architecture matching, and library validation are a production security gate. The managed-runtime decision is therefore accepted, while each platform distribution adapter remains subject to its proof gate.
