# ADR-009: Controlled default-core policy
- Status: Accepted

## Context
Zero configuration means new users should not have to understand core selection.

## Decision
Maintain a controlled default core per system, chosen after license, source, platform, format, BIOS, accuracy, performance, and maintenance evaluation. Exact matrix remains to be validated.

## Consequences
Predictable/testable experience, but RetroFrontier must maintain the matrix.

## Runtime security refinement
Libretro cores are executable native libraries with the permissions of the RetroArch process. In V1, every loadable core must be a separately identified TUF target and component of an approved managed Runtime Release, covered by trusted metadata, exact archive and installed-payload hashes, platform/architecture checks, license metadata, OS code-signing policy where applicable, and the authenticated core allowlist.

Per-game overrides may select only from installed approved managed cores. V1 does not load cores from arbitrary paths, a system RetroArch installation, user downloads, or an in-app general core store. A future advanced mode requires a separate product and security decision and must not weaken the managed default.
