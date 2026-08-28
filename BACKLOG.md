# RetroFrontier Backlog

## Current Priority

The project has completed its local-library foundation, M6.2 shell/empty/scan UX, and M6.3 bounded
library browsing slice. Game detail/readiness, metadata/settings UX, launch, and later UI work
remain; runtime trust and core-policy research remain explicit release gates.

## M0 — Planning and Repository Foundation

### Documentation

- [ ] Add `PROJECT_CONTEXT.md`
- [ ] Add `PRODUCT.md`
- [ ] Add `DOMAIN.md`
- [ ] Add `ARCHITECTURE.md`
- [ ] Add `BACKLOG.md`
- [ ] Add `AGENTS.md`
- [ ] Add ADRs
- [ ] Add `CONTRIBUTING.md`
- [ ] Add `SECURITY.md`
- [ ] Add repository `LICENSE`
- [ ] Add `.gitignore`

### Git/GitHub

- [ ] Create GitHub repository
- [ ] Configure `main`
- [ ] Enable squash merge
- [ ] Protect `main` after initial bootstrap
- [ ] Add pull request template
- [ ] Add basic issue templates
- [ ] Add CI after application scaffold exists

## M0.1 — Spike: Managed RetroArch Runtime

**Model:** Luna Max implementation/research; Sol Max architecture/security review.

Goal: prove RetroFrontier can download and run an isolated RetroArch runtime on:

- [ ] Windows x86_64
- [ ] macOS arm64
- [ ] macOS x86_64
- [x] Linux x86_64 (qualified with documented limitations; cross-distribution/device release gates remain)

Validate:

- [ ] approved runtime source
- [ ] download
- [ ] extract/install
- [ ] explicit executable launch
- [ ] explicit isolated config
- [ ] explicit core directory
- [ ] audio/video
- [ ] controller visibility
- [ ] controlled saves
- [ ] no system-config leakage
- [ ] archive extraction security
- [ ] platform blockers
- [ ] candidate update/rollback mechanism

Deliverable: spike report + architecture updates. No production updater required yet.

Spike outcome:

- Linux x86_64 extracted-AppImage/AppRun launch, synthetic core/content smoke test, explicit-path isolation, hostile-config test, and Linux lifecycle qualification: complete on Fedora 44; the documented distribution/device matrix remains a release gate.
- Windows x86_64 portable artifact research: complete; real-device verification remains.
- macOS arm64 and x86_64 artifact research: complete; real-device signing, quarantine, core-loading, and update verification remains.
- Sol Max architecture/security review: complete. It accepts immutable versions, staging, a minimal active pointer, full reconstruction repair, bounded rollback, and managed approved cores; it rejects an authoritative transaction journal and selects a TUF-compatible trust model.
- macOS managed executable/core distribution remains a production security blocker pending a real Developer ID/notarization/library-validation proof. Windows and cross-distribution Linux support still require real platform experiments.

## M0.2 — Spike: ScreenScraper Authentication

**Model:** Luna Max.

Research:

- [x] developer credential requirements
- [x] distribution/embedding architecture decision
- [x] user credential options
- [x] request/thread limits and returned quota fields
- [x] client identification
- [x] cache/retry expectations
- [x] offline behavior

Deliverable: documented authentication/provider decision.

Research was finalized on 2026-08-27 and is documented in
[`docs/SCREENSCRAPER_SPIKE.md`](docs/SCREENSCRAPER_SPIKE.md). **M0.2 is complete and M5 is ready**
for its constrained V1 scope. Current ES-DE demonstrates a recoverable application-credential,
direct-client, local-cache precedent, but is not treated as ScreenScraper policy. RetroFrontier
accepts direct Rust integration, build-time release credential injection, optional OS-vault user
credentials, normalized metadata, one primary cover, source preservation, conservative quota
probing, and evidence-bound stale-match revalidation as project decisions. Automatic CHD, CUE/BIN,
GDI, M3U/multi-disc, and RVZ matching, broad media, provider-cache export, and exact M6 attribution
presentation are explicitly non-blocking deferred capabilities. The pre-M5 identity cleanup is on
`main`.

## M1 — Application Foundation

**Model:** Luna Max.

- [x] Scaffold Tauri 2
- [x] React + TypeScript + Vite
- [x] pnpm
- [x] Rust formatting/linting
- [x] frontend formatting/linting
- [x] Rust module boundaries
- [x] typed IPC conventions
- [x] structured logging
- [x] application error model
- [x] SQLite/sqlx
- [x] migrations
- [x] settings repository
- [x] tests
- [x] design tokens
- [x] minimal app shell
- [x] basic CI

## M2 — Managed Runtime Foundation

**Model:** Luna Max implementation; Sol Max review for updater/security.

- [x] runtime manifest schema
- [x] platform/architecture IDs
- [x] runtime state model
- [x] download staging
- [x] integrity verification
- [x] safe archive extraction
- [x] managed install
- [x] detection
- [x] repair
- [ ] update discovery
- [x] safe activation
- [x] rollback
- [x] minimal active pointer and filesystem-derived startup recovery
- [ ] TUF-compatible runtime trust metadata and key-rotation/revocation ceremony
- [x] authenticated installed-file inventory and local modification detection
- [x] single-instance, runtime mutation lock, and game-process liveness coordination
- [x] block activation while game runs
- [x] runtime status UI
- [ ] repair UI
- [ ] macOS app/core signing and quarantine strategy
- [ ] Windows Authenticode/Smart App Control and pointer replacement spike
- [x] Linux extracted-AppImage/AppRun entry-point and distribution/device matrix qualification (Fedora 44 proof; VM/manual matrix remains a release gate)
- [ ] finalize hosting/source model

## M3 — Systems, Cores, BIOS

**Model:** Luna Max.

- [x] system registry with stable identifiers and aliases
- [x] supported file extensions in the system catalog
- [x] default/approved-core policy model with explicit unresolved decisions
- [ ] resolve the default-core matrix
- [ ] core licenses and approved distribution sources
- [x] platform/architecture availability model
- [x] BIOS requirements
- [x] BIOS discovery
- [x] BIOS hashing/validation when authoritative identities exist
- [x] per-system BIOS/readiness status UI
- [ ] map user BIOS folders to any future core-required internal layout

M3 research still open: the repository's core matrix does not approve a default or alternative
core for any system, and it does not provide authoritative BIOS identities/hashes. The catalog
keeps both uncertainties explicit; filename candidates are not treated as valid identities.

M3 review follow-up: complete. Systems/readiness now consumes one coherent
verified runtime snapshot for status and verified core availability.

Systems:

- [x] NES
- [x] SNES
- [x] Nintendo 64
- [x] Game Boy
- [x] Game Boy Color
- [x] Game Boy Advance
- [x] Sega Mega Drive / Genesis
- [x] PlayStation
- [x] Sega Saturn
- [x] Sega Dreamcast
- [x] Nintendo GameCube

## M4 — Library Scanner

**Model:** Luna Max.

- [x] reuse one verified runtime snapshot for systems/readiness queries before production runtime artifacts or frequent refreshes (M3 MEDIUM-1)
- [x] managed ROM folder structure
- [x] persist content roots
- [x] external ROM roots
- [x] recursive discovery
- [x] system hints
- [x] format classification
- [x] hashing
- [x] single-file content
- [x] CUE/BIN
- [x] CHD
- [x] GDI
- [x] M3U
- [x] multi-disc
- [x] persist Game/Content Unit/Content File
- [x] reconcile removed files
- [x] safe moved-content reconciliation
- [x] manual rescan
- [x] filesystem watcher
- [x] coalesced progress
- [x] scan issues
- [x] integration tests with synthetic fixtures

V1: no automatic rename, move, conversion, or deletion.

### M4 corrective-pass behavior

The M4 scanner also enforces canonical containment before reading descriptor or playlist members,
caps descriptor reads at 256 KiB, preserves verified identity across transient hash failures,
tracks absence authority per enumerated directory/protected subtree, keeps consumed move candidates
live and unique, and never falls back to standalone M3U units. CUE/GDI compatibility includes
BOM/CRLF handling, unquoted CUE filenames, preserved Windows separators, and harmless trailing GDI
text. These are scanner behavior clarifications, not a new milestone or a change to the normalized
Game/ContentUnit/ContentFile model.

The pre-M5 identity cleanup preserves a prior `GameId` when new M3U ownership is proven by persisted
content membership to have exactly one logical predecessor. Contested playlist ownership remains
separate and produces an explicit reconciliation issue. Move matching now requires a complete
one-to-one candidate relationship instead of allowing enumeration order to award an old file
identity. Same-path byte replacement retains local IDs and updates hashes/fingerprints; stale
provider-match handling remains M5 work.

The remaining M4 review performance, schema, IPC, watcher, startup, and large-scale follow-ups
remain deferred to their appropriate later milestone.

## M5 — Metadata

**Model:** Luna Max.

Constrained V1 scope: one ScreenScraper provider; direct Rust integration; RetroFrontier-owned
application credentials; optional OS-vault user credentials; provider-aware persistent scheduling;
dynamic quota/error handling; deterministic returned-evidence matching for supported single-file
content; candidate-only heuristic search; normalized metadata; one primary cover; local offline
cache; refresh; stale evidence revalidation; and provider state isolated from M4 local identity.
Container-specific automatic matching and broad media scraping are not required for M5 completion.

- [x] MetadataProvider interface
- [x] ScreenScraper adapter
- [x] request queue
- [x] rate-limit handling
- [x] retry/backoff
- [x] local cache
- [x] failed/deferred state
- [x] offline behavior
- [x] game matching
- [x] normalized metadata
- [x] cover/media download
- [x] media cache
- [x] metadata refresh

M5 is implemented. The provider boundary is provider-neutral, the ScreenScraper adapter owns every
provider-specific detail, and `metadata_jobs` plus `provider_scheduler_state` make the queue and its
quota deferrals restart-safe. Deterministic matching covers ordinary single-file ROM content and
requires agreeing returned evidence; heuristic results stay candidates. Normalized metadata,
provider identity, evidence, and one primary cover persist separately, and changed M4 evidence marks
a match stale while retaining the last-known-good snapshot. Optional personal credentials live in the
OS credential vault behind an injectable abstraction; SQLite holds only an opaque reference. The
previously deferred SQLite write-concurrency item is resolved by ADR-013 (WAL, busy timeout, short
writers). See [`docs/METADATA.md`](docs/METADATA.md).

Deliberately not in M5: automatic deterministic matching for CHD, CUE/BIN, GDI, M3U/multi-disc, RVZ,
and disc-system single-file images; broad media scraping; portable provider-cache export; and the
visible attribution presentation, which belongs to M6.

## M6 — Library UI

**Model:** Luna Max.

- [x] M6.1 backend enablement: bounded library queries, summaries, local detail, favorites, issue
  pages, typed root errors, cached-cover delivery, and metadata invalidation contracts
- [x] M6.2 shell / empty library / scan UX (complete and reviewed)
- [x] M6.3 library browsing
- [ ] M6.4 game detail / readiness
- [ ] M6.5 metadata UX / settings
- [ ] M6.6 hardening / accessibility / documentation

- [x] navigation shell
- [x] empty state
- [x] library UI
- [x] GameCard
- [x] search
- [x] system filters
- [x] favorites
- [ ] game details
- [ ] runtime readiness
- [ ] BIOS readiness
- [x] scan progress/issues
- [x] settings entry points

## M7 — RetroArch Launch

**Model:** Luna Max.

- [ ] RetroArchService
- [ ] explicit managed executable
- [ ] core selection
- [ ] explicit config
- [ ] save/state/system paths
- [ ] content launch target
- [ ] prerequisite validation
- [ ] child process
- [ ] process monitoring
- [ ] return to RetroFrontier
- [ ] play sessions
- [ ] per-game core override
- [ ] normalized launch errors

## M8 — Controller and Focus

**Model:** Luna Max.

- [ ] semantic input actions
- [ ] keyboard mapping
- [ ] controller mapping
- [ ] focus graph
- [ ] row/action/media focus behavior
- [ ] confirm/back/context
- [ ] controller footer
- [ ] primary settings navigation
- [ ] regression tests

## M9 — Saves and Save States

**Model:** Luna Max; Sol only for risky compatibility/migration design.

- [ ] controlled save dirs
- [ ] preserve saves across runtime replacement
- [ ] save-state discovery
- [ ] save-state metadata
- [ ] core version
- [ ] runtime release
- [ ] screenshots where supported
- [ ] compatibility warning/fallback design

## M10 — Packaging and V1

**Model:** Luna Max implementation; Sol Max release-readiness review.

- [ ] Windows packaging
- [ ] macOS arm64
- [ ] macOS x86_64
- [ ] Linux x86_64
- [ ] app updater strategy
- [ ] signing strategy
- [ ] macOS notarization where applicable
- [ ] license notices
- [ ] runtime/core attribution
- [ ] clean-machine smoke tests
- [ ] runtime install/update/rollback tests
- [ ] V1 checklist
- [ ] Sol architecture/security review

## Post-V1 Candidates

- collections expansion
- advanced statistics
- metadata editor
- ROM verification/conversion
- automatic M3U generation
- duplicate management
- custom RetroArch
- more metadata providers
- achievements
- netplay
- cloud sync
- additional systems
