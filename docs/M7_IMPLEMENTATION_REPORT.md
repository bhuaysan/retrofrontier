# M7 RetroArch Launch — Implementation Report

**Date:** 2026-08-30
**Branch:** `feat/m7-retroarch-launch`
**Starting commit:** `33708327e7978dac2ac0f0dd4f798e27f11213e0` ("feat: complete M6 library UI", `main`)
**Head at report time:** `02fc7be`
**Status:** Implemented and verified against synthetic authenticated runtime fixtures. Not merged,
not pushed. Real four-system emulation qualification is still outstanding and is blocked on an
approved production Runtime Release.

## Commits

| Commit | Summary |
| --- | --- |
| `e921f6b` | docs: add M7 RetroArch launch design |
| `950ad0c` | docs: add M7 implementation plan |
| `76a73db` | feat(systems): resolve M7 approved core policy |
| `55be78c` | feat(bios): record authoritative PlayStation BIOS identities |
| `ef3dff2` | feat(launch): add M7 launch domain and persistence |
| `64aae0a` | feat(runtime): extend the managed process record for launches |
| `8f04133` | feat(runtime): add the verified launch runtime boundary |
| `aec37a9` | feat(launch): add controlled RetroArch paths, config, environment, and host checks |
| `952bd7e` | feat(launch): add the managed game process adapter |
| `7f3c282` | feat(launch): add RetroArchService |
| `46b5119` | feat(launch): add LaunchApplicationService and the launch IPC surface |
| `04c01fb` | feat(library): add the frontend launch contract and useGameLaunch |
| `992acfe` | feat(library): add the Game Detail Play interaction |
| `02fc7be` | docs: record the implemented M7 launch contract |

46 files changed, 9,137 insertions, 281 deletions.

## Files and migrations added

**Migrations**

- `src-tauri/migrations/20260830000000_launch.up.sql`
- `src-tauri/migrations/20260830000000_launch.down.sql`

`game_launch_overrides` (one user-owned `core_id` per game) and `play_sessions` (game, content unit,
core, runtime installation and release, timestamps, exit code, constrained outcome). Both use
`ON DELETE RESTRICT`; `play_sessions` carries a check constraint so a session is open exactly while
it has no end time.

**Rust**

- `src-tauri/src/domain/launch.rs` — launch error codes, failure/context, response union, play
  sessions, overrides, host prerequisites, launch state.
- `src-tauri/src/repositories/launch.rs` — `LaunchRepository`.
- `src-tauri/src/services/retroarch.rs` — `RetroArchService`.
- `src-tauri/src/services/retroarch_paths.rs` — `LaunchPaths`.
- `src-tauri/src/services/retroarch_config.rs` — `RetroArchConfig`.
- `src-tauri/src/services/retroarch_env.rs` — child environment allowlist.
- `src-tauri/src/services/retroarch_host.rs` — Linux host prerequisite inspector.
- `src-tauri/src/adapters/game_process.rs` — `GameProcessLauncher`, `SpawnedGame`, `ProcessExit`.
- `src-tauri/src/application/launch.rs` — `LaunchApplicationService`.
- `src-tauri/src/commands/launch.rs` — `launch_game`, `get_launch_state`.

**Frontend**

- `src/hooks/useGameLaunch.ts` and its test.
- `src/features/library/launchStatus.ts` — normalized failure copy.
- Launch DTOs and commands in `src/platform/ipc.ts`; Play interaction in `GameDetailPage.tsx`;
  shell wiring in `AppShell.tsx`; styles in `src/styles/index.css`.

**Documentation**

- `docs/superpowers/specs/2026-08-30-m7-retroarch-launch-design.md`
- `docs/superpowers/plans/2026-08-30-m7-retroarch-launch.md`
- `docs/RETROARCH_LAUNCH.md`
- Updates to `docs/RUNTIME_MANAGER.md`, `docs/CORE_MATRIX.md`, `docs/DEVELOPMENT.md`,
  `ARCHITECTURE.md`, `DOMAIN.md`, `BACKLOG.md`.

## Architecture implemented

```text
React (useGameLaunch / GameDetailPage)
  -> launch_game(gameId, contentUnitId?) / get_launch_state
  -> LaunchApplicationService
       -> LibraryRepository, LaunchRepository, SystemCatalog, BiosService
       -> RuntimeManager (verified_launch_runtime, lock_for_launch, process record)
       -> RetroArchService -> LaunchPaths / RetroArchConfig / environment / host inspector
                           -> GameProcessLauncher
```

`RuntimeManager` kept every existing responsibility and gained exactly two entry points.
`verified_launch_runtime()` returns the absolute authenticated AppRun, absolute per-component core
paths with their release-declared systems, and absolute support-asset paths, from the *same* single
active-installation verification that already produces runtime status — the reconciliation was
extracted so status, installed-core availability, and launch paths can never come from separate
verifications. `lock_for_launch()` hands over the existing OS runtime mutation lock.

`RetroArchService` owns core and content resolution, prerequisite validation, configuration,
environment, and spawning, and touches no SQLite. `LaunchApplicationService` owns ordering,
serialization, process-record transitions, session persistence, monitoring, and reconciliation, and
builds no command line, configuration, or environment. React remains presentation only: it supplies
a `GameId` and optionally a `ContentUnitId`, never a path.

## Core-policy decisions

Resolved for four reference systems; the other seven V1 systems keep `CorePolicyDecision::Unresolved`
and return `corePolicyUnresolved`. Identities, libretro core names, licences, and upstream sources
were verified against the libretro core documentation rather than assumed.

| System | `CoreId` | libretro core | Licence |
| --- | --- | --- | --- |
| `nes` | `nestopia` | `nestopia_libretro` | GPL-2.0 |
| `snes` | `bsnes-mercury-balanced` | `bsnes_mercury_balanced_libretro` | GPL-3.0 |
| `playstation` | `beetle-psx` | `mednafen_psx_libretro` | GPL-2.0 |
| `nintendo_gamecube` | `dolphin` | `dolphin_libretro` | GPL-2.0 |

Decisions worth flagging for review:

- **"bsnes Balanced" resolved to bsnes-mercury Balanced.** No upstream libretro core is literally
  named `bsnes_balanced`; the maintained Balanced-profile build libretro publishes is
  `bsnes_mercury_balanced_libretro`, documented as built from the balanced profile. This is a naming
  clarification of the approved decision, not a substitution of another emulator family, and it is
  recorded in the design and the core matrix.
- **Managed component identity is now translated, not assumed.** `SystemsApplicationService`
  previously compared raw verified runtime component identifiers against `CorePolicy::default_core_id`,
  which silently assumed the two identifiers were equal. Verified components are now mapped through
  the catalog, so an installed but unapproved component can never be reported as an available core.
- **PlayStation BIOS identities were closed.** BIOS identity became per accepted file, so a genuine
  dump under another documented filename is `presentInvalid` rather than valid-but-unloadable.
  `BiosHashAlgorithm` gained MD5 because the approved core publishes its accepted dumps as MD5 only;
  inventing SHA-256 values would have been unverifiable. `scph1001.bin` was removed because the
  approved core does not look it up. PlayStation stays `Required`; the core's bundled OpenBIOS
  fallback is deliberately not relied on and its BIOS-override option is not enabled.
- **SNES coprocessor firmware is deferred, not modelled as required.** bsnes-mercury documents
  optional per-title firmware; marking every SNES title BIOS-required would be false.
- **No licence or distribution conflict was found.** The only gating item is unchanged from M2 and
  ADR-012: there is still no approved production Runtime Release source, TUF root, or hosting
  decision, so no managed runtime can actually be installed. The four resolved rows are therefore
  marked *policy resolved, managed release pending*.

## Runtime and process safety behaviour

- **Record schema 3.** Adds `launch_id` and `play_session_id` and makes process identity optional so
  a conservative `launching` record exists before the spawn.
- **A deliberate ordering deviation.** The M7 brief's conceptual list records the process after the
  spawn. ADR-011's ordering is used instead — session, `launching` record, spawn, identity, `running`
  record — because the window between `exec` and persisting a PID is exactly where a crash would
  leave a live managed RetroArch invisible to RuntimeManager. The post-spawn requirement is kept:
  identity is completed immediately, and a child whose identity cannot be established or durably
  recorded is terminated rather than left running.
- **Phase-specific validation.** A `running` record requires PID, start-time ticks, and the observed
  executable; a `launching` record must claim none of them, so a fabricated identity cannot pass for
  a real one.
- **Liveness fails closed in both phases.** A previous-boot record is proven dead. A `running` record
  keeps the existing boot-id, start-time, and canonical `/proc/<pid>/exe` checks unchanged — an
  identity mismatch stays uncertain and blocking, and PID alone is never identity. A `launching`
  record is decided by a bounded `/proc` scan matching an executable inside `runtime/versions/` or an
  `argv[0]` equal to the authenticated AppRun, which covers a script AppRun and over-detects rather
  than under-detects.
- **Lock scope.** The OS runtime mutation lock is held from before verification until the `running`
  record is committed, so an activation cannot interleave with verification-to-spawn.
- **Concurrency.** An in-process mutex, in-process active state, the durable record, and the mutation
  lock cooperate; a second attempt returns `gameAlreadyRunning`.
- **Restart reconciliation.** No record ⇒ open sessions become `interrupted`. A live `running` record
  ⇒ the session stays running, the process is adopted and polled, and closes as `interrupted` when
  proven gone (no exit code exists for a process RetroFrontier did not fork). An uncertain record ⇒
  nothing closed, nothing deleted, launches refused, `blocked` reported. SQLite never overrides the
  OS verdict.
- **Monitoring.** The child is waited on via `spawn_blocking`, so the Tauri UI thread is never
  blocked and RetroArch exiting never terminates RetroFrontier. A non-zero exit after a successful
  start is classified as `crashed`, not as runtime corruption.

## Isolation behaviour

One generated `runtime-user/config/retroarch.cfg`, rewritten atomically (`0600`) before every launch;
no per-game configuration files. Every writable RetroArch directory is RetroFrontier-owned;
`libretro_directory` is the only value pointing into the verified immutable version tree.
`config_save_on_exit` and the four `*_in_content_dir` switches are `false`.

`system_directory` is composed: a symlink per validated user BIOS file pointing at the file where the
user put it, plus `dolphin-emu/Sys` pointing at the verified managed support component. Only links
RetroFrontier created are replaced; user BIOS files are never modified, moved, renamed, or copied,
and no user data enters an authenticated runtime tree.

The child environment is an allowlist — display/session, D-Bus, audio, GPU-selection, identity, and
locale variables — plus `PATH=/usr/bin:/bin` and RetroFrontier-owned `XDG_*` base directories.
`LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, `RETROARCH*`, `LIBRETRO*`, and a hostile
`XDG_CONFIG_HOME` are absent by construction. Missing host graphics, audio, or input capabilities
become launch diagnostics; only a missing display session blocks.

## Tests run

All commands were executed and observed passing at `02fc7be`.

**Rust** — `cargo test --manifest-path src-tauri/Cargo.toml`: **393 passed, 0 failed, 1 ignored**
(the pre-existing opt-in local real-BIOS inspection test). Baseline on `main` was 308, so M7 added
85 Rust tests.

Focused suites:

| Suite | Tests |
| --- | --- |
| `application::launch` | 18 |
| `services::retroarch*` (service, paths, config, env, host) | 33 |
| `application::runtime_manager` | 21 |
| `adapters::runtime_process` | 9 |
| `adapters::database` (migrations) | 8 |
| `repositories::launch` | 6 |
| `adapters::game_process` | 5 |
| `domain::launch` | 5 |

**Frontend** — `pnpm test`: **353 passed across 23 files** (baseline 345 across 23; M7 added 8
`useGameLaunch` tests plus 8 Game Detail launch-interaction tests, and two M6 "no launch action"
assertions were converted to assert the action M7 now owns).

**Verification suite** — `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`,
`pnpm build`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test`, and `git diff --check` all clean.

Coverage highlights: the four resolved and seven unresolved core policies; overrides unable to escape
approved policy; per-file BIOS identity and the documented PlayStation identities; content resolution
for single-file, CHD, CUE/BIN, and M3U with member tracks and root escapes refused; every controlled
configuration path; hostile environment removal with session variables preserved; Dolphin `Sys` only
from verified managed data; the full launch failure matrix; clean exit, non-zero exit, early exit,
spawn failure, identity failure, stale PID, PID reuse, wrong executable, stale boot id, corrupt
record, concurrent launches, restart with a live child, restart with a dead child, an uncertain
record, and runtime mutation blocked during a game and allowed after a verified exit.

Process lifecycle coverage uses a synthetic shell `AppRun` inside a synthetic managed installation
while exercising the *real* durable process record and the *real* OS mutation lock. No ROM, BIOS,
runtime download, network access, or credential is required. One test-harness `ETXTBSY` retry was
added when spawning a freshly written executable under parallel test threads; it is confined to test
helpers.

## Repository hygiene

`git status` before and after is identical apart from the intended tracked changes: the 28
pre-existing untracked `M*_REVIEW.md` files and `docs/M5_IMPLEMENTATION_REPORT.md` remain untracked
and unmodified. (`docs/M5_IMPLEMENTATION_REPORT.md` was briefly swept into the documentation commit
by a broad `git add`; the commit was amended to remove it from the index and it is untracked again.)
No ROM, BIOS file, runtime payload, generated installation, credential, or build artifact was added.

## Deferred work

- macOS and Windows launch adapters.
- Core policy for the remaining seven V1 systems.
- Per-game override management UI and non-core overrides (persistence and resolution exist and are
  tested; there is deliberately no IPC mutation command yet).
- Per-region PlayStation BIOS enforcement — any one of the three documented dumps satisfies the
  requirement today.
- SNES per-title coprocessor firmware detection; GameCube optional IPL support.
- Save-state management (M9), controller/focus (M8), packaging (M10).

## Manual qualification still required

The automated suite proves the launch architecture, not emulation. Before any public Linux claim:

1. An approved production Runtime Release, TUF root, and hosting decision must exist (ADR-012).
2. The four-system manual procedure in `docs/RETROARCH_LAUNCH.md` must be performed with legally
   owned content and BIOS files — launch, video/audio/controller, save location, mutation blocked
   while running, focus return, crash-and-restart adoption, and the Dolphin `Sys` link.
3. The distribution and device matrix in `docs/spikes/LINUX_RUNTIME_QUALIFICATION.md` remains open,
   as do power-loss durability and native standalone X11.

## Milestone boundary assessment

Met: four systems have resolved controlled core policy; approved core resolution is enforced and
unmanaged cores cannot launch; runtime prerequisite verification is enforced; BIOS validation happens
before spawn; controlled explicit configuration, the managed AppRun, and managed core paths are used;
the content launch target is deterministic; process identity is durably tracked; runtime mutations
stay blocked while RetroArch is alive; process exit is monitored; play sessions and per-game core
overrides are persisted; restart/recovery is safe; normalized errors reach the UI; M6 library and
detail behaviour has no regression; the automated suite passes.

Not claimed: that a real emulator has been launched by this code. That requires the production
Runtime Release and the manual qualification above.
