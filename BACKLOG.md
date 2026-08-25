# RetroFrontier Backlog

## Current Priority
The project is in planning/foundation. Risky runtime and metadata assumptions should be tested before broad feature implementation.

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
- [ ] developer credential requirements
- [ ] distribution/embedding rules
- [ ] user credential options
- [ ] request/thread limits
- [ ] client identification
- [ ] cache/retry expectations
- [ ] offline behavior

Deliverable: documented authentication/provider decision.

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

- [ ] system registry
- [ ] content formats
- [ ] default-core matrix
- [ ] core licenses
- [ ] platform/architecture availability
- [ ] BIOS requirements
- [ ] BIOS discovery
- [ ] BIOS hashing/validation
- [ ] actionable BIOS UI
- [ ] map user BIOS folders to internal layout

Systems:
- [ ] NES
- [ ] SNES
- [ ] Nintendo 64
- [ ] Game Boy
- [ ] Game Boy Color
- [ ] Game Boy Advance
- [ ] Sega Mega Drive / Genesis
- [ ] PlayStation
- [ ] Sega Saturn
- [ ] Sega Dreamcast
- [ ] Nintendo GameCube

## M4 — Library Scanner
**Model:** Luna Max.

- [ ] managed ROM folder structure
- [ ] persist content roots
- [ ] external ROM roots
- [ ] recursive discovery
- [ ] system hints
- [ ] format classification
- [ ] hashing
- [ ] single-file content
- [ ] CUE/BIN
- [ ] CHD
- [ ] M3U
- [ ] multi-disc
- [ ] persist Game/Content Unit/Content File
- [ ] reconcile removed files
- [ ] safe moved-content reconciliation
- [ ] manual rescan
- [ ] filesystem watcher
- [ ] coalesced progress
- [ ] scan issues
- [ ] integration tests with synthetic fixtures

V1: no automatic rename, move, conversion, or deletion.

## M5 — Metadata
**Model:** Luna Max.

- [ ] MetadataProvider interface
- [ ] ScreenScraper adapter
- [ ] request queue
- [ ] rate-limit handling
- [ ] retry/backoff
- [ ] local cache
- [ ] failed/deferred state
- [ ] offline behavior
- [ ] game matching
- [ ] normalized metadata
- [ ] cover/media download
- [ ] media cache
- [ ] metadata refresh

## M6 — Library UI
**Model:** Luna Max.

- [ ] navigation shell
- [ ] empty state
- [ ] library UI
- [ ] GameCard
- [ ] search
- [ ] system filters
- [ ] favorites
- [ ] game details
- [ ] runtime readiness
- [ ] BIOS readiness
- [ ] scan progress/issues
- [ ] settings entry points

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
