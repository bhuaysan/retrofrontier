# M7 RetroArch Launch Design

## Status

Approved design for the M7 increment. Linux x86_64 only. macOS is the second platform; Windows
remains a V1 target and is deliberately not implemented here.

Starting point: `feat/m7-retroarch-launch`, branched from `main` at
`33708327e7978dac2ac0f0dd4f798e27f11213e0` ("feat: complete M6 library UI").

## Scope

M7 turns a library game into a controlled managed RetroArch launch:

1. resolve a `GameId` (plus an optional `ContentUnitId`) to one deterministic content target;
2. resolve an approved managed core from static policy plus a user-owned per-game override;
3. verify the managed runtime, the selected core, required BIOS, and Linux host prerequisites;
4. construct a RetroFrontier-owned RetroArch configuration and child environment;
5. spawn the authenticated managed `AppRun` as a child process;
6. durably record managed-process identity so runtime mutation stays blocked;
7. record a Play Session;
8. monitor the child asynchronously and return the UI to a stable state on exit;
9. reconcile safely after a RetroFrontier crash or restart.

M7 explicitly does **not** implement the M8 controller/focus graph, M9 save-state management, or
M10 packaging/release work, and it does not resolve core policy for the seven systems outside the
four M7 reference systems.

## Approved core policy for the four reference systems

The current `SystemCatalog::v1()` leaves every core policy `Unresolved`. M7 resolves exactly four.

Upstream facts were verified against the libretro core documentation sources during this design
(`github.com/libretro/docs`, `docs.libretro.com`), not assumed:

| System | RetroFrontier `CoreId` | libretro core name | Licence | Upstream source |
| --- | --- | --- | --- | --- |
| `nes` | `nestopia` | `nestopia_libretro` | GPLv2 (as documented by libretro) | libretro Nestopia, tracking the Nestopia JG upstream (`gitlab.com/jgemu/nestopia`) |
| `snes` | `bsnes-mercury-balanced` | `bsnes_mercury_balanced_libretro` | GPLv3 (as documented by libretro) | `github.com/libretro/bsnes-mercury`, Balanced profile |
| `playstation` | `beetle-psx` | `mednafen_psx_libretro` | GPLv2 (as documented by libretro) | `github.com/libretro/beetle-psx-libretro` |
| `nintendo_gamecube` | `dolphin` | `dolphin_libretro` | GPLv2 (as documented by libretro) | `github.com/libretro/dolphin` |

### "bsnes Balanced" naming resolution

The approved intent is the bsnes *Balanced* profile. There is no current upstream libretro core
literally named `bsnes_balanced`; the maintained Balanced-profile build published by libretro is
**bsnes-mercury Balanced** (`bsnes_mercury_balanced_libretro`), documented by libretro as "built
from the 'balanced' profile", based on bsnes v094 with accuracy backports. RetroFrontier therefore
records `bsnes-mercury-balanced` as the exact approved managed component identity and documents
that it is the concrete realisation of the approved "bsnes Balanced" decision. This is a naming
clarification, not a substitution of a different emulator family.

`bsnes-mercury` is GPLv3. That is compatible with RetroFrontier's own `GPL-3.0-or-later`
intent and with ADR-012's requirement that every component carry recorded licence metadata. It
does **not** by itself authorise redistribution; the managed Runtime Release that ships the core
remains subject to ADR-012 approval and the still-open release-hosting decision.

### Licence / distribution conflict check

No conflict was found that blocks M7 implementation. All four cores are GPLv2/GPLv3 libretro
cores with public upstream sources, already built for Linux x86_64 by the libretro project. The
remaining gating item is unchanged from M2/ADR-012 and is *not* an M7 regression: RetroFrontier
still has no approved production Runtime Release source, TUF root, or hosting decision, so no
managed runtime can actually be installed yet. M7 is therefore implemented and tested against
synthetic authenticated runtime fixtures, exactly as M2 was, and the four core rows in
`docs/CORE_MATRIX.md` are marked *policy resolved, managed release pending*.

### Platform targets

Each `CoreDefinition` declares only `linux`/`x86_64` targets in M7, because that is the only
platform qualified by `docs/spikes/LINUX_RUNTIME_QUALIFICATION.md`. Core resolution rejects a core
whose declared targets do not include the running platform/architecture with `coreNotApproved`.
Additional targets are added when their runtime distribution is qualified.

### `CoreId` and managed component identity

`CoreDefinition::managed_component_id` is the authenticated `RuntimeComponent::id` inside a
Runtime Release. Today `SystemsApplicationService` compares raw verified component identifiers
against `CorePolicy::default_core_id`, which silently assumes the two identifiers are equal. M7
fixes that by translating verified component identifiers through the catalog
(`SystemCatalog::core_for_component`), so:

- `CoreAvailabilityStatus::available_core_ids` now contains approved `CoreId`s that are actually
  installed and verified, never arbitrary unapproved component identifiers;
- launch resolves the managed core path through `managed_component_id`, never through `CoreId`
  string equality.

For the four M7 cores the two identifiers are chosen to be equal, but nothing depends on that.

### Systems that stay unresolved

`nintendo_64`, `game_boy`, `game_boy_color`, `game_boy_advance`, `mega_drive`, `sega_saturn`, and
`sega_dreamcast` keep `CorePolicyDecision::Unresolved`. A launch request for them returns
`corePolicyUnresolved`. There is no fallback core, no "first installed core", no `PATH` lookup,
and no per-game override escape hatch (an override is only valid if the core is approved *for that
system*, and an unresolved system approves nothing).

## BIOS policy

### PlayStation — closing the authoritative identity gap

`SystemCatalog::v1()` currently declares four candidate PlayStation filenames and **no**
authoritative identities, so every present file reports `notCoveredByCatalog`. M7 closes this using
the libretro Beetle PSX core documentation, which is the authoritative statement of which BIOS
dumps the approved core accepts:

| Filename | Description | MD5 |
| --- | --- | --- |
| `scph5500.bin` | PS1 JP BIOS | `8dd7d5296a650fac7319bce665a6a53c` |
| `scph5501.bin` | PS1 US BIOS | `490f666e1afb15b7362b406ed1cea246` |
| `scph5502.bin` | PS1 EU BIOS | `32736f17079d0b2b7024407c39bd3050` |

Consequences and deliberate decisions:

- **The published identities are MD5.** `BiosDigest` currently supports SHA-256 only, and a
  published MD5 cannot be converted into a SHA-256 without the file. M7 therefore extends
  `BiosHashAlgorithm` with `Md5` rather than inventing unverifiable SHA-256 values. `BiosService`
  keeps reporting the observed SHA-256 for every inspected candidate (unchanged IPC field) and
  additionally computes MD5 when a requirement's accepted identities need it.
- **Identity becomes per file, not per requirement.** Today a requirement holds
  `expected_filenames` and a flat `expected_hashes` list, so `scph5501.bin` containing the JP dump
  would validate. M7 replaces that with `accepted_files: Vec<BiosFileIdentity>`
  (`filename`, optional `size_bytes`, `digests`). Each documented dump is only valid under its own
  documented filename, which is exactly how Beetle PSX looks BIOS up. The serialized
  `BiosRequirementStatus` shape (`expectedFilenames`, `expectedSizeBytes`, `sha256`, …) is
  unchanged, so no frontend contract changes.
- **`scph1001.bin` is removed from the PlayStation candidates.** The approved core does not look
  that filename up, so accepting it would produce a "valid BIOS" state that still fails at launch.
  RetroFrontier never renames or copies user BIOS files, so the user-facing requirement is to place
  one of the three documented filenames in `Documents/RetroFrontier/BIOS`.
- **No expected sizes are asserted.** The digest already pins identity exactly; an unverified size
  assertion could only turn a genuine dump into `presentInvalid`.
- **Region enforcement is deliberately deferred.** RetroFrontier cannot currently determine content
  region reliably, so the requirement is satisfied when *at least one* of the three documented
  dumps is present and valid. This is recorded as a known M7 limitation.
- **OpenBIOS is never silently used.** Beetle PSX can boot with its bundled OpenBIOS. RetroFrontier
  keeps PlayStation `BiosPolicy::Required`, validates before spawn, and returns `biosMissing` /
  `biosInvalid` rather than starting a session on a fallback implementation. The generated config
  does not enable the core's "Override BIOS" option.

If a future review shows these identities are not trustworthy, the correct response is to remove
them and let PlayStation report `biosNotCoveredByCatalog` again — never to weaken validation.

### SNES

SNES keeps `BiosPolicy::NotRequired`. bsnes-mercury documents *coprocessor* firmware
(`dsp1*.rom`, `dsp2*`, `dsp3*`, `dsp4*`, `cx4.data.rom`, `st010*`, `st011*`, `st018*`,
`sgb.boot.rom`) that only a small number of enhancement-chip titles need, and the core ships HLE
options for many of them. Marking every SNES title as BIOS-required would be false. Per-title
coprocessor firmware detection needs cartridge-level identification that RetroFrontier does not
have in M7; it is recorded as deferred work in `docs/CORE_MATRIX.md` and is **not** modelled as a
system-level requirement.

### GameCube

GameCube keeps `BiosPolicy::NotRequired`. The GameCube IPL is optional for Dolphin and lives under
the emulator's own user directory rather than the RetroFrontier BIOS root; supporting it is
deferred.

### NES

NES keeps `BiosPolicy::NotRequired`. Nestopia's only BIOS (`disksys.rom`) is for Famicom Disk
System content, and `.fds` is not a supported V1 NES extension.

## Architecture

```text
React (GameDetailPage / useGameLaunch)
  -> typed Tauri command (launch_game, get_launch_state)
  -> LaunchApplicationService          (application/launch.rs)
       -> LibraryRepository            (game, content units, content roots)
       -> LaunchRepository             (play_sessions, game_launch_overrides)
       -> SystemCatalog                (static approved core / BIOS policy)
       -> BiosService                  (BIOS validation)
       -> RuntimeManager               (verified launch runtime + mutation lock)
       -> RetroArchService             (services/retroarch.rs)
            -> LaunchPaths             (app-owned RetroArch directories)
            -> RetroArchConfig         (generated RetroFrontier-owned config)
            -> ChildEnvironment        (constructed allowlist environment)
            -> HostPrerequisiteInspector
            -> GameProcessLauncher     (adapters/game_process.rs)
       -> managed process record       (adapters/runtime_process.rs, schema v3)
```

`RuntimeManager` keeps every existing responsibility (install, update, repair, rollback, trust,
activation, installed-tree verification, mutation safety). It gains exactly two read/coordination
methods and nothing else:

- `verified_launch_runtime()` — one trust-consistent verification of the active installation that
  additionally returns absolute managed paths (`AppRun`, per-component core paths, support-asset
  paths). It reuses the same pointer read, trust check, manifest check, completion-marker check and
  installed-inventory verification as `verified_snapshot()`, plus `validate_app_run`.
- `lock_for_launch()` — hands the caller the existing OS-backed runtime mutation lock guard.

`RetroArchService` owns approved-core resolution, content-target resolution, prerequisite
validation, configuration generation, environment construction, spawning, monitoring, and result
normalization. `LaunchApplicationService` orchestrates that work with persistence, the catalog, the
runtime boundary, the launch mutex, and the durable process record. `RetroArchService` never
touches SQLite; `LaunchApplicationService` never builds command lines, configuration files, or
environments.

React never receives or supplies an executable path, core path, BIOS path, save path, system path,
or content path.

## Public launch contract

```ts
launchGame({ gameId: number, contentUnitId?: number | null }): Promise<LaunchResponse>
getLaunchState(): Promise<LaunchState>
onGameLaunchStateChanged(handler): Promise<UnlistenFn>
```

`LaunchState` is the durable running-game projection used on mount and after a restart:

```ts
interface LaunchState { running: RunningGameSession | null; blocked: boolean }
interface RunningGameSession {
  sessionId: number; gameId: number; contentUnitId: number; coreId: string; startedAt: number;
}
interface LaunchDiagnostic { kind: HostPrerequisite; message: string }
```

`blocked` is true only in the uncertain-process-record case, where a launch would return
`gameAlreadyRunning` even though no running session can be described.

`LaunchResponse` is a discriminated union, so React never parses free text:

```ts
type LaunchResponse =
  | { status: 'started'; session: RunningGameSession; diagnostics: LaunchDiagnostic[] }
  | { status: 'contentSelectionRequired'; options: LaunchContentOption[] }
  | { status: 'failed'; error: LaunchFailure };

interface LaunchFailure {
  code: LaunchErrorCode;          // stable camelCase code
  message: string;                // safe, user-facing, generated in Rust
  context: LaunchFailureContext;  // typed, optional structured detail
}
```

The command returns `Result<LaunchResponse, AppError>`. Every anticipated launch problem is a
`failed` response, not an IPC error; `AppError` remains reserved for the existing infrastructure
failures. A launch problem is never surfaced as a raw OS error, `errno` string, or filesystem path.

`LaunchErrorCode` (stable, exhaustive for M7):

`gameNotFound`, `gameUnavailable`, `contentSelectionRequired`, `contentUnavailable`,
`runtimeNotReady`, `corePolicyUnresolved`, `coreNotInstalled`, `coreNotApproved`, `biosMissing`,
`biosInvalid`, `biosNotCoveredByCatalog`, `hostPrerequisiteMissing`, `gameAlreadyRunning`,
`configPreparationFailed`, `spawnFailed`, `processIdentityFailed`, `processExitedDuringLaunch`,
`sessionPersistenceFailed`, `internalLaunchFailure`.

`LaunchFailureContext` carries only already-safe identifiers: `coreId`, `systemId`,
`biosRequirementIds`, `runtimeState`, `hostPrerequisite`, and `exitCode`. `contentSelectionRequired`
carries `options: LaunchContentOption[]` (`contentUnitId`, `kind`, `localTitle`, `fileCount`,
`availability`) — the same bounded projection Game Detail already renders. No absolute paths.

## Content unit selection

1. Load the `Game`. Absent → `gameNotFound`.
2. `Game.availability != Available` → `gameUnavailable`.
3. Load the game's content units with their ordered membership.
4. A unit is **launchable** when:
   - `ContentUnit.availability == Available`;
   - its ordinal-0 membership exists and its role is `Standalone`, `Descriptor`, or `Playlist`
     (never `Track`, `Disc`, `DiscTrack`, or `DiscDescriptor`);
   - the ordinal-0 file's `availability == Available`;
   - the ordinal-0 file's `relative_path == ContentUnit.primary_relative_path`;
   - the owning `ContentRoot` is enabled and not `unavailable`/`unsafe`.
5. No launchable unit → `contentUnavailable`.
6. Exactly one launchable unit and no explicit request → select it.
7. More than one launchable unit and no explicit request → `contentSelectionRequired` listing
   **all** launchable units. Row order, insertion order, and `id` order are never used as a
   tie-break.
8. Explicit `contentUnitId`:
   - not a unit of this game → `contentUnavailable` (a foreign unit is never launched, and the
     failure deliberately does not confirm that the id exists elsewhere);
   - not currently launchable → `contentUnavailable`.

**Content target** = `ContentRoot.path` joined with `ContentUnit.primary_relative_path`, canonicalised
and required to stay inside the canonicalised content root. Because the M4 scanner already stores
the descriptor (`.cue`), the playlist (`.m3u`), or the standalone file (`.nes`, `.sfc`, `.chd`,
`.iso`, `.rvz`) as `primary_relative_path` at ordinal 0, this yields the correct primary target for
single-file ROMs, CHD, CUE/BIN, GDI, M3U, and multi-disc sets without collapsing multi-file content
into a single-file model and without ever launching a member track. The resolved absolute path must
exist and be a regular file at launch time; otherwise `contentUnavailable`.

## Core selection

```text
per-game override (valid) -> approved system default -> verified installed managed core
```

1. `GameLaunchOverride` for the game, if present, supplies a `CoreId`. It is valid only when the
   core is in `CorePolicy::approved_core_ids` for **this** system, the catalog knows the
   `CoreDefinition`, and the definition supports the running platform target. An invalid override
   returns `coreNotApproved` — it never falls through to the default core, and it is never deleted
   automatically.
2. Otherwise `CorePolicy::default_core_id`. `CorePolicyDecision::Unresolved` → `corePolicyUnresolved`.
3. The chosen `CoreDefinition::managed_component_id` must be present in the verified launch runtime
   as an installed, authenticated `ComponentKind::Core` component whose declared systems include the
   game's system. Otherwise `coreNotInstalled`.

An override stores a `CoreId`, never a filesystem path. There is no mechanism to load a core from
`PATH`, from a user directory, from an arbitrary path, or from a system RetroArch installation.

## Per-game core override persistence

```sql
CREATE TABLE game_launch_overrides (
    game_id    INTEGER PRIMARY KEY,
    core_id    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT
);
```

This is user-owned state, deliberately in its own table beside `game_user_state` for the same
reason: scanner reconciliation and provider refresh write `games`, `content_units`,
`content_files`, and the `provider_*` tables, and must never reset a user decision. No RetroArch
setting other than the core choice is stored here; video, shader, and input overrides are a later
product decision.

M7 implements persistence and resolution plus their tests. It deliberately does **not** add an
override-management UI or an IPC mutation command, because the M7 frontend scope is the Play
interaction only; the capability is documented as UI-deferred.

## Play sessions

```sql
CREATE TABLE play_sessions (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id                 INTEGER NOT NULL,
    content_unit_id         INTEGER NOT NULL,
    core_id                 TEXT NOT NULL,
    runtime_installation_id TEXT NOT NULL,
    runtime_release_id      TEXT NOT NULL,
    started_at              INTEGER NOT NULL,
    ended_at                INTEGER,
    exit_code               INTEGER,
    outcome                 TEXT NOT NULL
        CHECK (outcome IN ('running','completed','failed_to_start','crashed','interrupted')),
    created_at              INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL,
    CHECK ((outcome = 'running' AND ended_at IS NULL)
        OR (outcome <> 'running' AND ended_at IS NOT NULL)),
    FOREIGN KEY (game_id)         REFERENCES games (id)         ON DELETE RESTRICT,
    FOREIGN KEY (content_unit_id) REFERENCES content_units (id) ON DELETE RESTRICT
);
```

Outcome classification:

| Outcome | Meaning |
| --- | --- |
| `running` | The session is open; a managed process is believed alive. |
| `completed` | The child exited with status 0. |
| `failedToStart` | The child exited non-zero within the launch window, or spawn/identity handling terminated it. |
| `crashed` | The child was terminated by a signal, or exited non-zero after the launch window. |
| `interrupted` | RetroFrontier could not observe the exit (restart reconciliation, monitor failure). |

No raw stderr, no log blob, and no arbitrary provider/OS text is stored in a session row. RetroArch's
own log goes to the app-owned `logs/retroarch/` directory through the generated config.

Play-session history is **product data**. It is never consulted to answer "is a managed game process
alive?"; that answer comes only from `runtime/game-process.json` plus OS process identity.

## Process safety authority

### Record schema v3

`runtime/game-process.json` remains the authority. `MANAGED_PROCESS_RECORD_SCHEMA_VERSION` moves
from `2` to `3` and the record gains launch/session identity plus an explicit pre-spawn phase:

```rust
pub struct ManagedProcessRecord {
    pub schema_version: u32,                 // 3
    pub phase: ManagedProcessPhase,          // launching | running
    pub launch_id: SafeIdentifier,
    pub play_session_id: i64,
    pub boot_id: String,
    pub installation_id: SafeIdentifier,
    pub expected_apprun_path: String,        // absolute, inside runtime/versions/<id>/
    pub pid: Option<u32>,                    // Some only in `running`
    pub process_start_time_ticks: Option<u64>,
    pub expected_executable_path: Option<String>,
}
```

Validation: `running` requires `pid`, `process_start_time_ticks`, and an absolute
`expected_executable_path`; `launching` requires all three to be absent. A record whose declared
`schema_version` is anything other than 3 is *not* deleted: it is uncertain, blocks every runtime
mutation, and makes startup report `Broken`/repair-required — the existing behaviour, unchanged.
Because M2–M6 could never launch a game, no v2 record with a live process can exist in practice.

Process identity validation is not weakened. `running` records are still checked with boot id,
`/proc/<pid>/stat` start-time ticks, and `/proc/<pid>/exe` canonical equality; a PID/start-time
match with a different executable stays *uncertain and blocking*, never "gone". PID alone is never
identity.

### Why a pre-spawn `launching` record

ADR-011 already requires a conservative launching record written *before* spawn, because the window
between `fork`/`exec` and durably persisting the PID is exactly where a RetroFrontier crash can
leave a live managed RetroArch that no safety check knows about. The prompt's conceptual ordering
records the process only after spawn; M7 uses the safer ADR-011 ordering and keeps the post-spawn
requirement as well:

1. reserve the Play Session row (`running`);
2. write the durable `launching` record (no PID) and fsync it;
3. spawn;
4. build strong process identity and atomically replace the record with `running`;
5. if step 4 cannot be completed, terminate the child and fail closed.

This is documented as a deliberate deviation from the conceptual list in the M7 brief.

### Reconciling a PID-less `launching` record

A `launching` record carries no PID, so PID liveness cannot decide it. Reconciliation is:

- `boot_id` differs from the current Linux boot id → the process cannot exist → clear the record;
- same boot id → scan `/proc` for any live process of this user whose canonical `/proc/<pid>/exe`
  resolves inside `runtime/versions/`, **or** whose `argv[0]` equals `expected_apprun_path` (an
  `AppRun` may be a script, in which case `exe` is the interpreter). Any match → still active,
  keep blocking. No match → prove dead → clear the record.
- Any error while scanning → uncertain → keep blocking, do not delete.

The scan is bounded (one `readlink` and one small read per `/proc/<pid>`), over-detects rather than
under-detects, and therefore always fails in the safe direction.

### Invariants

- A successfully running managed child is always represented by a durable `running` record, so
  update, activation, rollback, repair, and cleanup all continue to fail with `GameActive`.
- A record is never deleted while identity is uncertain.
- SQLite play-session state never overrides OS/process identity.
- The runtime mutation lock is held for the whole launch sequence (validation through the `running`
  record write), so an activation cannot interleave with verification-to-spawn.

## Crash / restart reconciliation

`LaunchApplicationService::reconcile_on_startup()` runs after `RuntimeManager::startup_reconcile()`:

| Durable record after runtime reconciliation | Action |
| --- | --- |
| Absent | Every open (`running`) play session is closed as `interrupted`. UI shows no running game. |
| `running`, identity proven live | The session stays `running`; the service publishes running-game state and starts an **adoption monitor** that polls the record's process identity and closes the session as `interrupted` (and clears the record) once the process is proven dead. RetroFrontier cannot `wait()` on a process it did not fork, so no exit code is available. |
| Present but identity uncertain (unsupported schema, mismatched executable, unreadable `/proc`) | Nothing is deleted, no session is closed, runtime mutation stays blocked, and the launch service reports a blocked state so a new launch returns `gameAlreadyRunning`. |

## Launch configuration

Every launch runs the absolute authenticated `AppRun` from the verified active installation:

```text
<AppRun> --config <app-data>/runtime-user/config/retroarch.cfg \
         -L <version>/cores/<component>/<core>.so \
         <content root>/<primary relative path>
```

`AppRun` is taken from `RuntimeManifest::app_run_path()` inside the verified installation; it is
never replaced by an inferred `usr/bin/retroarch`. `--config`, `-L`, and a positional content path
are the documented RetroArch CLI contract. No `PATH` lookup happens: the program is an absolute
path and the child `PATH` is a fixed minimal value.

### RetroFrontier-owned directories

Under the Tauri OS application-data directory (never hard-coded home paths, never inside a
replaceable `runtime/versions/<id>` tree):

```text
<app-data>/
├── runtime-user/
│   ├── config/retroarch.cfg        RetroArch base config (regenerated per launch)
│   ├── system/                     system_directory (composed, see below)
│   ├── core-info/                  libretro_info_path (+ core info cache)
│   ├── core-options/core-options.cfg  core_options_path
│   ├── assets/                     assets_directory
│   ├── shaders/                    video_shader_dir
│   ├── playlists/                  playlist_directory
│   ├── history/                    content_history_path and friends
│   ├── remaps/                     input_remapping_directory
│   ├── autoconfig/                 joypad_autoconfig_dir
│   ├── cache/                      cache_directory
│   ├── thumbnails/                 thumbnails_directory
│   ├── overlays/                   overlay_directory, osk_overlay_directory
│   ├── database/                   content_database_path, cheat_database_path
│   ├── filters/{video,audio}/      video_filter_dir, audio_filter_dir
│   ├── recordings/{output,config}/ recording_* directories
│   ├── menu/{browser,config}/      rgui_browser_directory, rgui_config_directory
│   ├── core-assets/                core_assets_directory
│   └── xdg/{config,data,cache,state}/  child XDG_* base directories
├── saves/                          savefile_directory
├── states/                         savestate_directory
├── screenshots/                    screenshot_directory
└── logs/retroarch/                 log_dir
```

`libretro_directory` points at the managed cores directory **inside the verified immutable version
tree**; RetroArch only reads it. Everything writable is outside version trees, so a runtime update
cannot destroy user data and user data cannot contaminate an authenticated tree.

The following are explicitly set so nothing leaks from a host RetroArch installation:
`config_save_on_exit = false`, `savefiles_in_content_dir`, `savestates_in_content_dir`,
`systemfiles_in_content_dir`, `screenshots_in_content_dir` all `false`, `sort_savefiles_enable` and
`sort_savestates_enable` `true` (per-core subdirectories inside RetroFrontier's own saves/states
roots), `log_to_file = true` with `log_dir`, and `cheevos_enable = false`.

### Base configuration strategy

There is exactly **one** generated configuration file. It contains only RetroFrontier-controlled
values, is deterministic for a given app-data root and installation, and is rewritten atomically
(unique same-directory temporary file, write, flush, `rename`, parent `fsync`, mode `0600`) at the
start of every launch. Because the core comes from `-L` and the content from `argv`, there is
nothing per-game to write, so RetroFrontier does **not** create per-game configuration files and
does not use `--appendconfig`. A crash therefore cannot leave an ambiguous half-written trust-
sensitive file: the previous complete config or the new complete config is present, and either way
it is regenerated before the next launch.

### Composed system directory

Beetle PSX reads BIOS from RetroArch's `system_directory`; dolphin-libretro requires its support
data at `<system_directory>/dolphin-emu/Sys`. Pointing `system_directory` at the user's
`Documents/RetroFrontier/BIOS` would force managed runtime content into user data, and pointing it
at a runtime version directory would force user BIOS into an immutable authenticated tree. Both are
forbidden.

RetroFrontier therefore **composes** `runtime-user/system/`, which it owns:

- For each accepted BIOS file that `BiosService` validated for the launching system, create a
  symbolic link `runtime-user/system/<documented filename>` → the user's BIOS file. User BIOS files
  are never modified, moved, renamed, copied, or deleted; only a link inside RetroFrontier's own
  directory is created.
- When the resolved core declares a managed support asset (Dolphin's `Sys`), create
  `runtime-user/system/dolphin-emu/Sys` → the **verified managed** support-asset path from
  `verified_launch_runtime()`. An arbitrary user `Sys` path is never trusted or searched.
- RetroFrontier owns and resets only the links it created; it never removes anything it does not
  recognise, and it refuses to replace a non-symlink entry (that is a `configPreparationFailed`).

### Child environment policy

The child environment is *constructed*, never blindly inherited and never blindly cleared.

**Preserved from the host when present** (desktop/session facts RetroArch genuinely needs, as
demonstrated by the Linux qualification):

- display/session: `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, `XDG_SESSION_DESKTOP`,
  `XDG_CURRENT_DESKTOP`, `XDG_RUNTIME_DIR`, `XDG_SEAT`, `XDG_VTNR`, `XAUTHORITY`;
- IPC: `DBUS_SESSION_BUS_ADDRESS`;
- audio: `PULSE_SERVER`, `PULSE_RUNTIME_PATH`, `PULSE_COOKIE`, `PIPEWIRE_RUNTIME_DIR`;
- graphics selection: `DRI_PRIME`, `__GLX_VENDOR_LIBRARY_NAME`, `__NV_PRIME_RENDER_OFFLOAD`,
  `__VK_LAYER_NV_optimus`, `MESA_LOADER_DRIVER_OVERRIDE`;
- identity/locale: `HOME`, `USER`, `LOGNAME`, `TZ`, `LANG`, `LANGUAGE`, `LC_ALL`, and the
  individual `LC_*` categories.

**Explicitly set by RetroFrontier** (host value discarded):
`PATH=/usr/bin:/bin`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME`
(all under `runtime-user/xdg/`).

**Everything else is absent by construction.** Because the environment is an allowlist,
`LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`, any `RETROARCH*`/`LIBRETRO*` variable, and a hostile
`XDG_CONFIG_HOME` cannot influence the launch. Dropping `LD_LIBRARY_PATH` is safe for the qualified
artifact, whose ELF runpath is `$ORIGIN/../lib`, and is recorded as a documented decision.

The working directory is set to the RetroFrontier-owned `runtime-user/` directory, so a relative
path can never resolve into user content; the qualification showed the launch is
working-directory independent.

## Linux host prerequisites

Host prerequisites are validated separately from managed-runtime integrity. A missing host
dependency never marks the runtime corrupt and never triggers repair.

**Blocking** (returns `hostPrerequisiteMissing` with a typed `hostPrerequisite` reason):

- `displaySessionUnavailable` — neither a usable Wayland display (`WAYLAND_DISPLAY` together with
  `XDG_RUNTIME_DIR`) nor an X11 `DISPLAY` is present.

**Non-blocking diagnostics** returned alongside a successful `started` response, so the user sees a
visible explanation instead of a silently degraded or falsely "damaged runtime" state:

- `graphicsDeviceUnavailable` — `/dev/dri` is absent or unreadable;
- `audioServiceUnavailable` — no `PULSE_SERVER` and no `$XDG_RUNTIME_DIR/pulse/native` socket;
- `inputDevicesUnavailable` — `/dev/input` is absent or unreadable.

This follows the qualification's conclusion that RetroArch can run with degraded audio and that a
missing audio service is a diagnostic, not corruption.

## Launch lifecycle

Ordering actually implemented (deviations from the brief's conceptual list are marked ▲ and
justified above):

1. Take the in-process launch mutex; reject a second concurrent request with `gameAlreadyRunning`.
2. If an in-process active game is recorded, return `gameAlreadyRunning`.
3. Acquire the OS runtime mutation lock (▲ moved before verification so activation cannot race
   verification-to-spawn). Failure → `runtimeNotReady`.
4. `ensure_no_active_game()` against the durable record → `gameAlreadyRunning` when blocked.
5. Load the Game → `gameNotFound` / `gameUnavailable`.
6. Resolve the Content Unit and its absolute content target.
7. Resolve system policy and the valid override/default core.
8. `verified_launch_runtime()` → `runtimeNotReady` when not `Ready`/`RollbackAvailable`.
9. Verify the selected core is installed, approved, and system-compatible.
10. Validate BIOS through `BiosService`.
11. Validate Linux host prerequisites and collect diagnostics.
12. Build the complete `LaunchContext` (paths, config, argv, environment).
13. Write the generated configuration and compose the system directory →
    `configPreparationFailed`.
14. Persist the Play Session as `running` → `sessionPersistenceFailed`.
15. ▲ Write the durable `launching` process record (no PID) and fsync. Failure → close the session
    as `failedToStart`, return `processIdentityFailed`.
16. Spawn the managed `AppRun`. Failure → clear the record, close the session as `failedToStart`,
    return `spawnFailed`.
17. If the child has already exited, record the exit code, close the session, clear the record, and
    return `processExitedDuringLaunch`.
18. Capture PID, `/proc/<pid>/stat` start-time ticks, boot id, and the observed
    `/proc/<pid>/exe`; atomically replace the record with `running`. Any failure → terminate the
    child, wait for it, clear the record, close the session as `failedToStart`, and return
    `processIdentityFailed`.
19. Release the runtime mutation lock; publish running state; emit `game-launch-state-changed`.
20. Monitor asynchronously. On exit: record the exit status, classify the outcome, close the Play
    Session, clear the process record, publish the stable non-running state, and emit the event.
21. Release the launch mutex.

RetroArch exiting never terminates RetroFrontier; the child is waited on in a blocking task on the
Tokio blocking pool, so the Tauri UI thread is never blocked and no zombie remains.

## Concurrent launches

One managed RetroArch process per RetroFrontier user/instance in V1. Three independent mechanisms
cooperate:

1. an in-process `tokio::sync::Mutex` serialising the launch sequence;
2. in-process active-game state, returning `gameAlreadyRunning` immediately;
3. the durable process record plus the OS runtime mutation lock, which also protect against a
   second or crashed application process.

## Frontend scope

Deliberately small, on top of the existing M6 Game Detail screen and its visual design:

- `PLAY` action in the existing cover-actions column beside `ADD TO FAVORITES`;
- `LAUNCHING…` (busy) and `RUNNING` (disabled, with an explanatory line) states;
- a normalized launch-failure panel driven by `LaunchErrorCode`, reusing `InlineError` copy
  patterns;
- a content-selection list when `contentSelectionRequired` is returned, re-invoking `launchGame`
  with the chosen `contentUnitId`;
- automatic return to the normal detail state when the exit event arrives;
- `getLaunchState()` on mount so a restart with a live adopted game shows `RUNNING`.

No controller focus graph, no launcher shell, no new route, no readiness redesign. The existing
`getReadinessRows` presentation is unchanged; readiness now legitimately reports a resolved core
policy for the four reference systems because the backend policy changed, not because the UI did.

## Testing strategy

Test-first (red → green) for each behaviour, using only synthetic and legal fixtures.

1. **Domain / core policy** — the four resolved policies, the seven unresolved systems, catalog
   validation of the new `CoreDefinition`s, unapproved cores rejected, overrides unable to escape
   approved policy, per-target rejection.
2. **BIOS** — per-file identity matching, MD5 algorithm support, wrong-name/right-content rejection,
   `notCoveredByCatalog` preserved where identities are still unknown, no change to the serialized
   status contract.
3. **LaunchApplicationService** — game not found, unavailable game, single-unit auto-selection,
   multi-unit selection required, foreign unit, unavailable/incomplete content, default core,
   allowed override, invalid override, runtime missing, runtime broken, approved core absent, BIOS
   missing, BIOS invalid, host prerequisite failure, game already running.
4. **RetroArchService / config** — absolute managed executable and core paths, correct content
   target per unit kind, no `PATH` lookup, no host RetroArch config leakage, every controlled
   directory present in the generated config, hostile `XDG_CONFIG_HOME`/`LD_PRELOAD`/`RETROARCH_*`
   removed, required session variables preserved, Dolphin `Sys` only from verified managed data.
5. **Process lifecycle** — a synthetic executable `AppRun` inside a synthetic managed installation
   drives: successful spawn, `launching → running` transition, clean exit, non-zero exit, early
   exit, spawn failure, process-identity failure, process-record write failure, stale PID, PID
   reuse, wrong executable identity, stale boot id, restart with a live child, stale record
   reconciliation, corrupt record, concurrent launches, runtime mutation blocked during a game and
   allowed after a verified exit. The existing process-identity tests are extended, never weakened.
6. **Persistence** — forward and down migrations, foreign keys, restrictive deletes, override
   persistence, play-session lifecycle, crash/restart reconciliation, and proof that a scan does not
   delete overrides or session history.
7. **Frontend** — Play action, launching state, running state, normalized error rendering,
   content-selection behaviour, return to a stable UI after exit, and M6 readiness regression
   coverage.

CI needs no ROM, no BIOS, no network, and no downloaded runtime. Real four-system qualification with
user-owned legal content is documented as manual work at the end of the milestone.

## Documentation updates

- `docs/CORE_MATRIX.md` — the four resolved rows with licence/source/component identity, the seven
  unresolved rows unchanged, PlayStation BIOS identities with their source, and the deferred SNES
  coprocessor-firmware note.
- `docs/RUNTIME_MANAGER.md` — record schema v3, the `launching` phase and its reconciliation, the
  new verified launch-runtime boundary, and launch/mutation lock interaction.
- `docs/RETROARCH_LAUNCH.md` (new) — the implementation contract: paths, config keys, environment
  policy, host prerequisites, lifecycle, error codes, and the manual qualification procedure.
- `ARCHITECTURE.md`, `DOMAIN.md`, `BACKLOG.md`, `docs/DEVELOPMENT.md` — made concrete where M7
  replaces previously conceptual behaviour.

## Deliberate non-goals and known limitations

- Windows and macOS launch adapters.
- Per-region PlayStation BIOS enforcement.
- Per-title SNES coprocessor firmware detection.
- GameCube IPL support.
- Override-management UI and non-core per-game overrides.
- Save-state management (M9) and controller mapping/focus (M8).
- A real production Runtime Release source; M7 is verified against synthetic authenticated
  fixtures, so no end-to-end emulation claim is made by the automated suite.
