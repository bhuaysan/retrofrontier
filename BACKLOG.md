# RetroFrontier Backlog

## Current Priority

M8 Controller and Focus is merged. Manual hardware qualification passed on
`Linux + WebKitGTK + USB Sony DualSense`; see
[`docs/M8_FINAL_HARDWARE_INPUT_REPORT.md`](docs/M8_FINAL_HARDWARE_INPUT_REPORT.md). Runtime Release
002 (`rf-runtime-1.22.2-linux-x86_64-002`) is the active qualified managed runtime release, and
Release 001 remains recorded as superseded history.

DualSense Edge and Bluetooth DualSense mapping under WebKitGTK is accepted MEDIUM backlog debt: the
face-button quirk predicate covers those variants by name, but only the USB DualSense has been
physically measured, and coverage is not qualification.

M8.5 User-Initiated Settings Metadata Scraper Workflow is implemented on
`feat/m8-5-settings-scraper`: a library scan no longer scrapes what it finds, and whole-library
metadata work is started by the user from Settings as a persistent, restartable, stoppable run.
Accepted-match evidence revalidation is unchanged. See
[`docs/METADATA.md`](docs/METADATA.md).

M8.6 System-Aware Library Shelves & Cover Profiles is implemented on
`feat/m8-6-library-shelves`: All Systems browses bounded per-system shelves instead of one flat
paginated grid, and Library cards frame artwork by system presentation profile without cropping it.
See [`docs/LIBRARY_BROWSING.md`](docs/LIBRARY_BROWSING.md).

**M9 — Saves and Save States is the next implementation milestone.**

The previously documented release gates remain open and unchanged: production key ceremony under
independent custody, public runtime hosting, immutable upstream mirroring, GPL source-offer
obligations, core policy for the remaining seven V1 systems, and cross-platform and
cross-distribution qualification (Windows and macOS launch and controller support, the Linux device
matrix, confirmed GameCube content execution, and PlayStation BIOS/content). No public-distribution
claim is made.

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
- [x] TUF-compatible runtime trust metadata (M7.5: real Ed25519 2-of-3 repository consumed by the production client)
- [ ] production key-rotation/revocation ceremony under independent custody (M10)
- [x] authenticated installed-file inventory and local modification detection
- [x] single-instance, runtime mutation lock, and game-process liveness coordination
- [x] block activation while game runs
- [x] runtime status UI
- [x] repair/reinstall UI (M7.5 Settings runtime panel)
- [ ] macOS app/core signing and quarantine strategy
- [ ] Windows Authenticode/Smart App Control and pointer replacement spike
- [x] Linux extracted-AppImage/AppRun entry-point and distribution/device matrix qualification (Fedora 44 proof; VM/manual matrix remains a release gate)
- [x] release construction tooling and a qualification release source (M7.5)
- [ ] finalize public hosting/source model and immutable upstream mirroring (M10)

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
- [x] M6.4 game detail / readiness
- [x] M6.5 metadata UX / settings
- [x] M6.6 hardening / accessibility / documentation
      - [x] accepted M6.5 candidate/action, provider/account truthfulness, focus, contrast, and
            regression findings
      - [x] carried M6.2–M6.4 focus, heading, readiness, scan, and visual hardening findings
      - [ ] M5 account lifecycle does not yet write `Invalid` after provider authentication failure
      - [ ] platform opener executable/reaping hardening and stuck-scan recovery remain native/release
            work outside this UI slice
      - [x] focused corrective pass restores truthful `ambiguous` candidate-state copy
      - [x] M66-MEDIUM-2 light-theme error/negative-status contrast — CLOSED in M6.7E by the
            semantic `--negative-text` token (`#743c00` light, `#ffb26c` dark)
      - [ ] M66-LOW-1 failed page-forward target can be committed by background refresh
      - [ ] M66-LOW-2 quota recency freezes at provider-panel mount
      - [ ] M66-LOW-3 terminal scan completion watermark is not monotonic
      - [ ] M66-LOW-4 candidate panels lack state-specific discovery copy
      - [ ] M66-LOW-5 account-clear is blocked by account-read failure
      - [ ] M66-LOW-7 landmark names include decorative `//`
      - [ ] M66-INFO-1 unused cover-image checksum fields in the DTO
      - [ ] M66-INFO-2 M5 `Invalid` account state remains unreachable
      - [ ] M66-INFO-3 metadata-operation fallback uses state generation
      - [ ] M66-INFO-4 rapid pagination can duplicate bounded queries
      - [ ] M66-INFO-5 deferral timer self-termination observation
      - [ ] M66-INFO-6 adjacent sidebar cursor glyph remains a design observation
- [x] M6.7 design fidelity / polish
      - [x] M6.7A library composition, M6.7B game card fidelity, M6.7C game detail fidelity,
            M6.7D settings and scan fidelity, M6.7E accessibility and theme polish
      - [x] stationary shared shell chrome across every route, including Settings
      - [x] final corrective pass: HIGH-1 sidebar vertical overflow containment
      - [x] final corrective pass: MED-1 light-theme active-control contrast (6.35:1)
      - [x] final corrective pass: MED-3 Settings no longer narrows the shared `.app-main`
      - [x] final corrective pass: MED-4 M6.7 documentation closure
      - [x] MED-2 smooth cursor / vector Favorite star — CLOSED AS INVALID, accepted product
            exception to the hard-edge pixel language; must not be restored to pixel snapping
      - [ ] final delta review and M6 acceptance
      - The remaining M66 LOW/INFO items above are accepted follow-ups, not M6 closure blockers.
      - [x] M66-LOW-6 required status/contrast/deferral documentation synchronization
      - [ ] M6.3 LOW-6 bounded per-filter count contract remains deferred
      - [ ] M6.3 LOW-7 design-approved 960×640 density remains deferred
      - [ ] M6.4 INFO-3 modified-click native-link fallback remains deliberate
      - [ ] full Unicode case folding remains a search-quality deferral
      - [ ] metadata event volume beyond the bounded consumer remains deferred
      - [ ] provider capability expansion remains governed by M5
      - [ ] archive/ZIP import remains out of scope

- [x] navigation shell
- [x] empty state
- [x] library UI
- [x] GameCard
- [x] search
- [x] system filters
- [x] favorites
- [x] game details
- [x] runtime readiness
- [x] BIOS readiness
- [x] scan progress/issues
- [x] settings entry points

## M7 — RetroArch Launch

**Model:** Luna Max.

Implemented for Linux x86_64 against synthetic authenticated runtime fixtures. Core policy is
resolved for NES, SNES, PlayStation, and GameCube only; the other seven V1 systems stay explicitly
unresolved. See [`docs/RETROARCH_LAUNCH.md`](docs/RETROARCH_LAUNCH.md).

- [x] RetroArchService
- [x] explicit managed executable
- [x] core selection
- [x] explicit config
- [x] save/state/system paths
- [x] content launch target
- [x] prerequisite validation
- [x] child process
- [x] process monitoring
- [x] return to RetroFrontier
- [x] play sessions
- [x] per-game core override
- [x] normalized launch errors

Deferred from M7:

- [ ] macOS and Windows launch adapters
- [ ] core policy for the remaining seven V1 systems
- [ ] per-game override management UI and non-core overrides
- [ ] per-region PlayStation BIOS enforcement and SNES coprocessor firmware detection
- [x] real Linux runtime installation and launch qualification (M7.5; NES and SNES pass, GameCube
      partial, PlayStation blocked on an approved BIOS dump and legal content)

## M7.5 — Real Managed Runtime and Linux Qualification

**Model:** Luna Max.

See [`docs/M7_5_RUNTIME_QUALIFICATION.md`](docs/M7_5_RUNTIME_QUALIFICATION.md) and
[`docs/M7_5_IMPLEMENTATION_REPORT.md`](docs/M7_5_IMPLEMENTATION_REPORT.md).

- [x] declarative, reproducible Runtime Release definition
- [x] release construction and TUF publication tooling (`release-tools` feature)
- [x] derived, proof-verified installed inventory
- [x] configured trusted release source with an honest production/qualification distinction
- [x] runtime install/repair application service, IPC, and Settings UX
- [x] real AppImage extraction (SquashFS superblock validation)
- [x] real installation through RuntimeManager
- [x] real launch through the M7 path, including crash/restart reconciliation
- [ ] confirmed GameCube content execution
- [ ] PlayStation qualification (needs an approved BIOS dump and legal content)
- [ ] cross-distribution matrix

## M8 — Controller and Focus

**Model:** Luna Max.

See [`docs/CONTROLLER_AND_FOCUS.md`](docs/CONTROLLER_AND_FOCUS.md),
[ADR-014](docs/adr/ADR-014-input-acquisition-boundary.md), and
[`docs/M8_IMPLEMENTATION_REPORT.md`](docs/M8_IMPLEMENTATION_REPORT.md), and — for the findings from
the physical DualSense/Wayland sessions —
[`docs/M8_HARDWARE_CORRECTIVE_REPORT.md`](docs/M8_HARDWARE_CORRECTIVE_REPORT.md) and
[`docs/M8_FINAL_HARDWARE_INPUT_REPORT.md`](docs/M8_FINAL_HARDWARE_INPUT_REPORT.md).

- [x] semantic input actions
- [x] keyboard mapping
- [x] controller mapping, deadzone, hysteresis, dominant axis, and bounded repeat
- [x] focus registry keyed by stable semantic identities
- [x] geometry-derived spatial navigation for the responsive grid
- [x] row/action/media focus behavior under the accepted A6 V5 language
- [x] confirm/back/context
- [x] temporary focus scopes
- [x] Library → Detail → Library focus restoration without DOM queries or timeouts
- [x] window-focus gating and RetroArch focus ownership
- [x] controller footer derived from the focus model
- [x] primary settings navigation
- [x] explicit Library controller navigation zones (sidebar / main)
- [x] direct Library Search controller action (Standard Gamepad button 3 / Triangle / Y)
- [x] managed RetroArch launches fullscreen
- [x] managed RetroArch joypad-autoconfig profiles as an authenticated Runtime Release component
- [x] regression tests
- [x] manual Linux controller/RetroArch qualification — PASS on
      `Linux + WebKitGTK + USB Sony DualSense`; see
      [`docs/M8_FINAL_HARDWARE_INPUT_REPORT.md`](docs/M8_FINAL_HARDWARE_INPUT_REPORT.md). Covers
      Library controller navigation, the Search and Context face-button behaviour, managed RetroArch
      fullscreen launch, controller operation in the RetroArch menu and in-game, focus and input
      return after exit, and RetroFrontier ignoring controller input while RetroArch is still
      running.
- [ ] Windows and macOS controller qualification
- [ ] controller remapping UI and persistence (B10)

### Qualify DualSense Edge and Bluetooth controller mapping on WebKitGTK

Accepted MEDIUM debt from the final M8 source review. Not an M8 merge blocker.

```text
Current physically qualified controller path:
Linux + WebKitGTK + USB Sony DualSense
```

The WebKitGTK/Linux face-button quirk predicate in `src/input/gamepadQuirks.ts` recognizes
DualSense devices broadly through a `/dualsense/i`-style name match, but only the USB Sony
DualSense has actually been measured on hardware. Being covered by the predicate is not evidence
of physical qualification, and this item must not be closed by reasoning about the code alone.

- [ ] physically measure raw `Gamepad.buttons` indices for a DualSense Edge under Linux/WebKitGTK
- [ ] physically qualify a Bluetooth DualSense under Linux/WebKitGTK
- [ ] verify whether the existing WebKitGTK/DualSense face-button transposition applies to those
      variants, or whether either reports canonical Standard Gamepad indices already
- [ ] review whether the current `/dualsense/i`-style predicate remains appropriate for the
      measured device set
- [ ] narrow or expand the predicate only from actual physical evidence, never from device-name
      inference
- [ ] keep canonical RetroFrontier Standard Gamepad semantics unchanged — the quirk normalizes a
      misreporting engine, it does not redefine the project's action mapping

No M8 controller source is changed for this item; it is recorded as debt deliberately.

## M8.5 — User-Initiated Settings Metadata Scraper Workflow

**Model:** Luna Max.

A library scan discovers content locally. It no longer means "send this to ScreenScraper". Whole-
library metadata work is an explicit user action started from
`Settings → Metadata → ScreenScraper → Library Scraper`, and it runs as a persistent scrape run
above the existing M5 queue rather than as a second pipeline. See
[`docs/METADATA.md`](docs/METADATA.md).

- [x] scrape-run domain: modes, run states, item states, terminal-result classification
- [x] persistent runs and a target snapshot fixed at start
- [x] one active run per provider, enforced by a partial unique index
- [x] Missing Metadata eligibility — untouched games only; a no-match, ambiguous set, unsupported
      shape or parked failure is an answer and is not re-asked
- [x] Refresh Matched eligibility — accepted matches that still name a provider game
- [x] interactive/bulk scheduling bands, with in-place promotion of a compatible bulk job
- [x] bounded feeder, transactional across `metadata_jobs` and run items
- [x] outcome reconciliation read back from authoritative M5 state
- [x] automatic first-time scraping removed; accepted-match revalidation retained
- [x] worker wake-up for explicit work, without bypassing quota, deferral or retry timing
- [x] restart recovery of an active run
- [x] cooperative stop that keeps written metadata and promoted interactive work
- [x] scrape IPC surface and the `useMetadataScrape` frontend model
- [x] Settings Library Scraper UI with game-level progress and truthful provider-wait copy
- [x] controller and keyboard coverage, including the stop confirmation focus scope
- [x] Library `needs review` filter and the REVIEW MATCHES route into it
- [x] scale coverage at 5,000 and 20,000 games with a bounded active queue

Deliberately not in M8.5:

- [ ] scrape by system, filter, favorites, or selection
- [ ] user-visible pause and resume, and deliberate resumption of a stopped run
- [ ] scrape run history UI
- [ ] retry-all no-match, retry-all failed, or resolve-all ambiguous
- [ ] cover-only bulk operations and broader artwork categories
- [ ] any ETA, completion percentage from job counts, or predicted provider reset instant

## M8.6 — System-Aware Library Shelves & Cover Profiles

**Model:** Luna Max.

All Systems is a bounded browse view rather than one flat paginated grid of every system mixed
together, and a system's cover artwork is framed by that system's own presentation profile instead
of one shared 3:4 box. See [`docs/LIBRARY_BROWSING.md`](docs/LIBRARY_BROWSING.md).

- [x] bounded system shelf projection — one set-oriented query, never one per system
- [x] shelf semantics derived from the grid's own filter predicate and list projection
- [x] catalog shelf ordering; empty systems omitted; unknown systems appended, never dropped
- [x] bounded preview with the system's full match total, and a semantic View All
- [x] View All sets the same system filter the sidebar sets — no new route
- [x] Search, Favorites and the M8.5 review filter keep shelf mode and compose
- [x] focused shelf query model with bounded, coalesced metadata invalidation
- [x] system cover presentation profiles, applied in shelves and in the full grid alike
- [x] Library card artwork contained rather than cropped; `GameCover` stays system-agnostic
- [x] controller shelf navigation on the existing geometric focus engine
- [x] deterministic Game Detail return chain: game → its shelf's View All → first visible → heading
- [x] scale coverage at 5,000 and 20,000 games with a response bounded by system count

Deliberately not in M8.6:

- [ ] infinite horizontal scrolling through a full system library
- [ ] user system ordering, drag-to-reorder, or cover-ratio settings
- [ ] region-specific box profiles or per-game ratio overrides
- [ ] automatic ratio detection from image dimensions
- [ ] recently-played, favorites-only, genre, or custom-collection shelves
- [ ] carousel animation or shelf autoplay

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
