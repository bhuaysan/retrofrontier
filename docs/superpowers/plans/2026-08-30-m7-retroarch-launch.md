# M7 RetroArch Launch Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Follow red-green-refactor: every production behaviour change starts with a failing test that demonstrates the missing behaviour.

**Goal:** Resolve a library game to a controlled managed RetroArch launch — approved core, deterministic content target, verified runtime, validated BIOS and host prerequisites, RetroFrontier-owned configuration, durable managed-process identity, persisted play session, asynchronous monitoring, and normalized errors reaching a small Game Detail Play interaction.

**Architecture:** React → typed Tauri command → `LaunchApplicationService` → `RetroArchService` → `RuntimeManager` / `LibraryRepository` / `LaunchRepository` / `BiosService` / launch adapters. `RuntimeManager` keeps every existing responsibility and gains only a verified launch-runtime read boundary and a launch lock accessor.

**Tech Stack:** Rust (Tauri 2, sqlx/SQLite, tokio), React 19 / TypeScript 6 / Vitest, existing design tokens and IPC conventions.

**Spec:** `docs/superpowers/specs/2026-08-30-m7-retroarch-launch-design.md`

## Global Constraints

- Work only on `feat/m7-retroarch-launch`; base commit `33708327e7978dac2ac0f0dd4f798e27f11213e0`; design checkpoint `e921f6b`. Do not merge, push, or rewrite history. Preserve the pre-existing untracked `M*_REVIEW.md` and `docs/M5_IMPLEMENTATION_REPORT.md` artifacts exactly.
- Resolve core policy for `nes`, `snes`, `playstation`, `nintendo_gamecube` only. The other seven V1 systems must remain `CorePolicyDecision::Unresolved` and must not become launchable through any fallback.
- Never use `retroarch` from `PATH`, a user's RetroArch installation, a user core directory, an arbitrary core path, or an arbitrary `Sys` path. Launch only the authenticated AppDir `AppRun`; never infer `usr/bin/retroarch`.
- React never supplies or receives an executable, core, BIOS, save, system, or content filesystem path.
- Do not weaken TUF/runtime trust, installed-tree verification, or managed-process identity. Never reduce identity to a PID. Never delete an uncertain process record.
- No ROM, BIOS, runtime payload, or generated installation may enter Git. All fixtures are synthetic.
- Do not start M8 controller/focus, M9 save states, or M10 packaging work, and do not perform unrelated refactors.
- Prefer small reviewable commits; run focused tests continuously and the full suite at the end.

---

### Task 1: Approved core policy for the four reference systems

**Files:** `src-tauri/src/domain/core.rs`, `src-tauri/src/domain/system.rs`, `src-tauri/src/application/systems.rs`, `docs/CORE_MATRIX.md`

- [ ] **Step 1 (RED):** Assert `SystemCatalog::v1()` resolves NES→`nestopia`, SNES→`bsnes-mercury-balanced`, PlayStation→`beetle-psx`, GameCube→`dolphin`; that the other seven remain `Unresolved` with a non-empty research item; that `catalog.validate()` passes; that each `CoreDefinition` declares the Linux/x86_64 target, its libretro name, licence, and `managed_component_id`; and that `core_for_component` maps a component identifier back to exactly one approved definition.
- [ ] **Step 2:** Add `CoreDefinition::supports_target`, `SystemCatalog::core`, `SystemCatalog::core_for_component`, and `SystemCatalog::approves_core_for_system`.
- [ ] **Step 3:** Add the four `CoreDefinition`s and their `CorePolicy::resolved(...)` entries; keep the seven unresolved rows untouched.
- [ ] **Step 4 (RED→GREEN):** Assert `SystemsApplicationService` availability translates verified runtime component identifiers through the catalog, so an unapproved component never appears as an available `CoreId`; update `available_core_ids` accordingly.
- [ ] **Step 5:** Update `docs/CORE_MATRIX.md` rows and status.

### Task 2: Authoritative BIOS identities

**Files:** `src-tauri/src/domain/bios.rs`, `src-tauri/src/domain/system.rs`, `src-tauri/src/services/bios.rs`

- [ ] **Step 1 (RED):** Assert a requirement made of per-file identities validates `scph5501.bin` only against its own documented MD5, reports `presentInvalid` when the right content carries the wrong documented filename, still reports `notCoveredByCatalog` for a requirement with no identities, and that `BiosRequirementStatus` keeps its existing serialized field names.
- [ ] **Step 2:** Add `BiosHashAlgorithm::Md5` and `BiosFileIdentity { filename, size_bytes, digests }`; replace `expected_filenames`/`expected_hashes`/`expected_size_bytes` on `BiosRequirement` with `accepted_files`, deriving the unchanged status fields.
- [ ] **Step 3:** Compute MD5 in `BiosService` only when an accepted identity needs it; keep reporting the observed SHA-256.
- [ ] **Step 4:** Replace the PlayStation requirement with the three documented filename/MD5 identities and drop `scph1001.bin`; leave Saturn, Dreamcast, and GBA identities unresolved.

### Task 3: Launch domain

**Files:** `src-tauri/src/domain/launch.rs` (new), `src-tauri/src/domain/mod.rs`

- [ ] **Step 1 (RED):** Assert every `LaunchErrorCode` serializes to its stable camelCase code, that `LaunchResponse` is a `status`-tagged union, that `LaunchFailure` never carries a filesystem path, and that `PlaySessionOutcome` round-trips through its database representation.
- [ ] **Step 2:** Add `PlaySessionId`, `PlaySession`, `PlaySessionOutcome`, `GameLaunchOverride`, `LaunchErrorCode`, `LaunchFailure`, `LaunchFailureContext`, `LaunchContentOption`, `LaunchDiagnostic`, `HostPrerequisite`, `RunningGameSession`, `LaunchState`, `LaunchResponse`, and the safe user-facing message table.

### Task 4: Launch persistence

**Files:** `src-tauri/migrations/20260830000000_launch.{up,down}.sql`, `src-tauri/src/repositories/launch.rs` (new), `src-tauri/src/repositories/mod.rs`, `src-tauri/src/adapters/database.rs`

- [ ] **Step 1 (RED):** Migration tests — forward migration from the M6 schema, down migration, foreign keys to `games`/`content_units` with `ON DELETE RESTRICT`, the `running ⇔ ended_at IS NULL` check, override upsert/replace, open-session lookup, and proof that a scanner reconciliation leaves overrides and session history intact.
- [ ] **Step 2:** Write the migrations and `LaunchRepository` (`core_override`, `set_core_override`, `clear_core_override`, `start_session`, `complete_session`, `open_sessions`, `session`).

### Task 5: Managed process record schema v3

**Files:** `src-tauri/src/domain/runtime.rs`, `src-tauri/src/adapters/runtime_process.rs`

- [ ] **Step 1 (RED):** Assert a `running` record still requires PID, start-time ticks, boot id, and an absolute observed executable; that a `launching` record requires all PID fields to be absent; that schema 2 and schema 4 both stay blocking and undeleted; that a `launching` record from a previous boot is cleared; that a `launching` record in the current boot with a live managed process keeps blocking; and that the existing stale-PID, PID-reuse, wrong-executable, and corrupt-record behaviours are unchanged.
- [ ] **Step 2:** Bump the schema constant to 3, add `launch_id`/`play_session_id`, make the PID triple optional, and tighten `validate()` per phase.
- [ ] **Step 3:** Add the bounded `/proc` scan (`exe` inside `runtime/versions/`, or `argv[0]` equal to the expected AppRun) and wire it into `LinuxManagedProcessInspector` for the `launching` phase, failing closed on any scan error.
- [ ] **Step 4:** Add `make_launching_record`, `make_running_record`, and keep `write_process_record` refusing records that do not target the managed runtime.

### Task 6: Runtime launch boundary

**Files:** `src-tauri/src/application/runtime_manager.rs`, `src-tauri/src/application/runtime.rs`

- [ ] **Step 1 (RED):** Assert `verified_launch_runtime()` returns an absolute `AppRun` inside the active installation, absolute paths for every authenticated core component with their approved systems, absolute support-asset paths, and an error when the runtime is not `Ready`/`RollbackAvailable`; assert `lock_for_launch()` is mutually exclusive with runtime mutation.
- [ ] **Step 2:** Implement both methods on `RuntimeManager`, reusing the existing pointer/trust/manifest/marker/inventory verification plus `validate_app_run`; expose them through `RuntimeApplicationService`.

### Task 7: RetroFrontier-owned RetroArch configuration

**Files:** `src-tauri/src/services/retroarch_paths.rs`, `src-tauri/src/services/retroarch_config.rs`, `src-tauri/src/services/retroarch_env.rs`, `src-tauri/src/services/retroarch_host.rs` (all new)

- [ ] **Step 1 (RED):** Assert the generated config sets every controlled directory (libretro, core info, core options, system, saves, states, screenshots, assets, shaders, playlists, cache, history, remaps, autoconfig, thumbnails, overlays, databases, filters, recordings, menu, log) to an absolute RetroFrontier-owned path, that `libretro_directory` points inside the verified version tree, that `config_save_on_exit` and the four `*_in_content_dir` keys are `false`, and that no value points at a host RetroArch directory.
- [ ] **Step 2 (RED):** Assert the constructed environment drops `LD_PRELOAD`, `LD_LIBRARY_PATH`, `RETROARCH_*`, `LIBRETRO_*`, and a hostile `XDG_CONFIG_HOME`; sets `PATH` to a fixed minimal value and the four `XDG_*` base dirs into `runtime-user/xdg/`; and preserves the display, session, D-Bus, audio, graphics-selection, identity, and locale variables when present.
- [ ] **Step 3 (RED):** Assert host prerequisites block only a missing display session and otherwise emit non-blocking graphics/audio/input diagnostics.
- [ ] **Step 4:** Implement `LaunchPaths` (from the OS app-data root), the atomic `0600` config writer, the environment builder, and the host inspector behind injectable traits.

### Task 8: Managed game process adapter

**Files:** `src-tauri/src/adapters/game_process.rs` (new), `src-tauri/src/adapters/mod.rs`

- [ ] **Step 1 (RED):** Using a synthetic executable `AppRun` inside a synthetic managed installation, assert a successful spawn reports a PID, that the child inherits only the constructed environment, that `wait` yields a clean exit, a non-zero exit, and a signal termination as distinct normalized results, and that a missing executable is a spawn failure.
- [ ] **Step 2:** Implement `GameProcessLauncher`/`SpawnedGame` over `std::process::Command` with `env_clear` plus the composed environment, absolute program, explicit working directory, and a blocking-pool `wait`.

### Task 9: RetroArchService

**Files:** `src-tauri/src/services/retroarch.rs` (new), `src-tauri/src/services/mod.rs`

- [ ] **Step 1 (RED):** Assert content-target resolution picks the descriptor for CUE/BIN, the playlist for M3U and multi-disc, the standalone file for single-file and CHD, never a member track, and rejects a target that escapes its content root; assert core resolution honours a valid override, rejects an override that is not approved for the system, rejects an unresolved policy, and rejects a core that is not installed or not target-compatible.
- [ ] **Step 2:** Implement resolution, `LaunchContext` construction, the composed system directory (validated BIOS symlinks plus a verified managed Dolphin `Sys`), config writing, and spawn delegation.

### Task 10: LaunchApplicationService

**Files:** `src-tauri/src/application/launch.rs` (new), `src-tauri/src/application/mod.rs`

- [ ] **Step 1 (RED):** Cover game not found, unavailable game, single-unit auto-selection, multi-unit selection required, foreign unit, unavailable content, default core, allowed override, invalid override, runtime missing, runtime broken, approved core absent, BIOS missing, BIOS invalid, host prerequisite failure, and game already running.
- [ ] **Step 2 (RED):** Cover the lifecycle — session persisted as `running`, `launching → running` record transition, clean exit closes the session as `completed`, non-zero exit as `crashed`/`failedToStart`, early exit as `processExitedDuringLaunch`, identity failure terminates the child and fails closed, record write failure fails closed, concurrent launches return `gameAlreadyRunning`, runtime mutation blocked during a game and allowed after a verified exit, and restart reconciliation (live child adopted, dead child interrupted, uncertain record blocking).
- [ ] **Step 3:** Implement the orchestration, launch mutex, ordering from the spec, asynchronous monitoring, event publication, and `reconcile_on_startup`.

### Task 11: IPC surface

**Files:** `src-tauri/src/commands/launch.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1 (RED):** Assert `launch_game` accepts only `gameId` and an optional `contentUnitId`, never a path, and that every anticipated problem is an `Ok(LaunchResponse::Failed)` rather than an IPC error.
- [ ] **Step 2:** Add `launch_game` and `get_launch_state`, register them, construct the service in `initialize_state`, and run startup reconciliation.

### Task 12: Frontend launch contract and hook

**Files:** `src/platform/ipc.ts`, `src/hooks/useGameLaunch.ts` (new) and its test

- [ ] **Step 1 (RED):** Assert the hook starts idle, moves to launching, becomes running on a `started` response, surfaces a normalized failure without parsing strings, exposes content-selection options, relaunches with a chosen unit, and returns to idle on the exit event.
- [ ] **Step 2:** Mirror the DTOs in `ipc.ts` and implement the hook with race-safe request generations.

### Task 13: Game Detail Play interaction

**Files:** `src/features/library/GameDetailPage.tsx`, its test, `src/app/AppShell.tsx`, `src/styles/index.css`

- [ ] **Step 1 (RED):** Assert the Play action renders beside Favorite, shows `LAUNCHING…` while pending and `RUNNING` while a game is active, renders the normalized failure copy, renders the content-selection list and relaunches with the chosen unit, and returns to the normal state after exit — plus an M6 readiness regression assertion.
- [ ] **Step 2:** Implement the interaction with existing primitives and tokens; do not restyle M6.

### Task 14: Documentation

**Files:** `docs/RETROARCH_LAUNCH.md` (new), `docs/RUNTIME_MANAGER.md`, `docs/CORE_MATRIX.md`, `ARCHITECTURE.md`, `DOMAIN.md`, `BACKLOG.md`, `docs/DEVELOPMENT.md`

- [ ] Record the implemented launch contract, paths, config keys, environment policy, host prerequisites, error codes, record schema v3, and the manual four-system qualification procedure.

### Task 15: Verification and report

- [ ] `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`, `pnpm build`
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`
- [ ] `git diff --check`; compare `git status` before and after
- [ ] Write `docs/M7_IMPLEMENTATION_REPORT.md`
