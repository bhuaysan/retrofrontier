# RetroArch Launch (M7)

This document describes the implemented M7 launch boundary. It is implementation documentation; the
approved design is `docs/superpowers/specs/2026-08-30-m7-retroarch-launch-design.md`.

## Scope

Linux x86_64 only. macOS is the second platform and Windows remains a V1 target; neither has a
launch adapter yet. M7 does not implement controller/focus (M8), save-state management (M9), or
packaging (M10).

## Boundary

```text
React (useGameLaunch / GameDetailPage)
  -> launch_game / get_launch_state
  -> LaunchApplicationService      (application/launch.rs)
       -> LibraryRepository, LaunchRepository, SystemCatalog, BiosService
       -> RuntimeManager (verified launch runtime, launch lock, process record)
       -> RetroArchService          (services/retroarch.rs)
            -> LaunchPaths, RetroArchConfig, child environment, host inspector
            -> GameProcessLauncher  (adapters/game_process.rs)
```

`RuntimeManager` keeps every existing responsibility and gained only two entry points:
`verified_launch_runtime()` (absolute authenticated AppRun, per-component core paths, support-asset
paths, from the same single active-installation verification that produces runtime status) and
`lock_for_launch()` (the existing OS-backed runtime mutation lock).

React never supplies or receives an executable, core, BIOS, save, system, or content filesystem
path. `launch_game` accepts a `GameId` and an optional `ContentUnitId`.

## Approved core policy

Resolved for four systems only; the other seven V1 systems stay `Unresolved` and return
`corePolicyUnresolved`. See `docs/CORE_MATRIX.md` for identities, licences, and sources.

| System | Core | Managed component |
| --- | --- | --- |
| NES | Nestopia UE | `nestopia` |
| SNES | bsnes-mercury Balanced | `bsnes-mercury-balanced` |
| PlayStation | Beetle PSX | `beetle-psx` |
| Nintendo GameCube | Dolphin | `dolphin` (plus the `dolphin-sys` support component) |

RetroFrontier explicitly selected **bsnes-mercury Balanced**
(`bsnes_mercury_balanced_libretro`, from `libretro/bsnes-mercury`) as the M7 SNES core, because it is
the currently qualified Balanced-profile artifact for this M7 decision. Upstream treats `bsnes` and
`bsnes-mercury` as separate core families, so this is a selection, not the literal upstream name of
the separate `bsnes` family, and no equivalence between the two is claimed.

Resolution order is a valid per-game override, then the approved system default. An override is
valid only when the catalog approves that core for that system and the definition declares the
running platform target; it never falls through to the default. The chosen core must additionally
exist as an authenticated installed component whose release-declared systems include the launching
system. `PATH`, a host RetroArch installation, a user core directory, and arbitrary core paths are
never consulted.

Storing an override runs that same resolution, so no override that fails the contract can be
persisted in the first place. The launch still revalidates: passing validation once does not make a
stored override trustworthy later, because the verified runtime can change underneath it.

## Content selection

A unit is launchable when it is `available`, its ordinal-zero membership is a `standalone`,
`descriptor`, or `playlist` file that is itself `available` and equal to the unit's
`primary_relative_path`, and its content root is enabled and not unavailable/unsafe. One launchable
unit is selected automatically; several produce a `contentSelectionRequired` response listing all of
them. Row, insertion, and id order are never used as a tie-break. A unit of another game, or one
that is not launchable, is refused with `contentUnavailable`.

The launch target is the content root joined with `primary_relative_path`, canonicalised and
re-checked for containment, so a `.cue`, `.m3u`, `.chd`, `.nes`, `.sfc`, `.iso`, or `.rvz` primary
is launched and a member track or disc image never is.

## BIOS

`BiosService` validates before anything is spawned. PlayStation is `Required` and carries the three
MD5 identities the approved core documents; the core's bundled OpenBIOS fallback is deliberately not
relied upon and its BIOS-override option is not enabled. Failures are reported as `biosMissing`,
`biosInvalid`, or `biosNotCoveredByCatalog`. RetroFrontier never downloads, modifies, moves,
renames, or deletes a BIOS file.

## Controlled paths

Under the OS application-data directory (never a hard-coded home path, never inside a replaceable
runtime version tree):

```text
runtime-user/{config,system,core-info,core-options,assets,core-assets,shaders,filters/{video,audio},
             playlists,history,remaps,autoconfig,cache,thumbnails,wallpapers,overlays,database,
             recordings/{output,config},menu/{browser,config},xdg/{config,data,cache,state}}
saves/  states/  screenshots/  logs/retroarch/
```

The generated `runtime-user/config/retroarch.cfg` sets every stateful RetroArch directory to one of
these paths. `libretro_directory` is the only value pointing into the verified immutable version
tree, because RetroArch only reads it. `config_save_on_exit` is `false` so RetroArch never rewrites
the generated file, and `savefiles_in_content_dir`, `savestates_in_content_dir`,
`systemfiles_in_content_dir`, and `screenshots_in_content_dir` are all `false` so nothing is written
beside user ROMs.

There is exactly one configuration file. It is deterministic and rewritten atomically (unique
same-directory temporary file, flush, rename, parent fsync, mode `0600`) before every launch, so no
per-game configuration files exist and a crash cannot leave an ambiguous half-written file.

### Composed system directory

`system_directory` is `runtime-user/system/`, which RetroFrontier owns and composes per launch:

- a symbolic link per validated user BIOS file, pointing at the file where the user put it;
- `dolphin-emu/Sys` pointing at the verified managed support component when Dolphin is resolved.

Only links RetroFrontier created are replaced; an unexpected regular file or directory is left alone
and a name collision with a non-link is a `configPreparationFailed`. User BIOS files are never
modified, moved, renamed, or copied, and no user data enters an authenticated runtime tree.

## Child environment

The environment is constructed from an allowlist, so it is neither blind inheritance nor a blind
clear:

- preserved when present — `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, `XDG_SESSION_TYPE`,
  `XDG_SESSION_DESKTOP`, `XDG_CURRENT_DESKTOP`, `XDG_RUNTIME_DIR`, `XDG_SEAT`, `XDG_VTNR`,
  `DBUS_SESSION_BUS_ADDRESS`, `PULSE_SERVER`, `PULSE_RUNTIME_PATH`, `PULSE_COOKIE`,
  `PIPEWIRE_RUNTIME_DIR`, `DRI_PRIME`, `MESA_LOADER_DRIVER_OVERRIDE`, `__GLX_VENDOR_LIBRARY_NAME`,
  `__NV_PRIME_RENDER_OFFLOAD`, `__VK_LAYER_NV_optimus`, `HOME`, `USER`, `LOGNAME`, `TZ`, `LANG`,
  `LANGUAGE`, `LC_ALL`, and the individual `LC_*` categories;
- set by RetroFrontier — `PATH=/usr/bin:/bin` and the four `XDG_*` base directories under
  `runtime-user/xdg/`;
- everything else absent by construction, including `LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`,
  any `RETROARCH*`/`LIBRETRO*` variable, and a hostile `XDG_CONFIG_HOME`.

Dropping `LD_LIBRARY_PATH` is safe for the qualified artifact, whose ELF runpath is
`$ORIGIN/../lib`. The working directory is `runtime-user/`, so a relative path can never resolve
into user content.

## Host prerequisites

Validated separately from managed-runtime integrity; a missing host capability never marks the
runtime corrupt or triggers a repair.

| Prerequisite | Blocking | Detection |
| --- | --- | --- |
| `displaySession` | yes | `WAYLAND_DISPLAY` with `XDG_RUNTIME_DIR`, or `DISPLAY` |
| `graphicsDevice` | no | `/dev/dri` readable |
| `audioService` | no | `PULSE_SERVER`, or `$XDG_RUNTIME_DIR/pulse/native` |
| `inputDevices` | no | `/dev/input` readable |

Non-blocking gaps are returned as `diagnostics` on a successful launch and rendered on Game Detail.

## Launch lifecycle

1. take the in-process launch mutex; a second concurrent request is `gameAlreadyRunning`;
2. refuse immediately if this process already has a running or blocked launch state;
3. acquire the OS runtime mutation lock (ADR-011 serializes launch and mutation under it), so an
   activation cannot interleave with verification-to-spawn;
4. `ensure_no_active_game()` against the durable process record;
5. load the game, resolve the content unit and its absolute target;
6. verify the runtime, resolve the approved core and its managed support assets;
7. validate BIOS, then host prerequisites;
8. write the generated configuration and compose the system directory;
9. persist the play session as `running`;
10. write the conservative `launching` process record — no PID, because the child does not exist yet;
11. spawn the authenticated `AppRun`;
12. build strong process identity and atomically replace the record with `running`; on failure the
    child is terminated and the launch fails closed — see *Uncertain process death* for what
    happens when that termination itself cannot be confirmed;
13. watch a bounded settle window for an immediate exit (`processExitedDuringLaunch`);
14. release the mutation lock, publish running state, and monitor the child on a blocking task;
15. on a positively reaped exit: classify the outcome, close the session, clear the record, publish
    the stable state.

`AppRun --config <generated config> -L <managed core> <content target>` is the exact command form.

### Why the record is written before the spawn

The prompt's conceptual ordering records the process only after spawn. M7 uses ADR-011's ordering
instead, because the window between `exec` and durably persisting a PID is exactly where a
RetroFrontier crash would leave a live managed RetroArch that no safety check knows about. The
post-spawn requirement is kept as well: identity is completed immediately, and a child whose
identity cannot be established or recorded is stopped rather than left running.

### Uncertain process death

The durable record may be deleted only with proof that no managed child survives. Two things count
as proof and nothing else does: no process was ever created, or the child's exit was positively
observed and reaped. A failed `wait()` and a failed `terminate()` prove nothing about a process.

So neither one clears the record. Where death is unproven, RetroFrontier:

- keeps `game-process.json` exactly as written, which keeps runtime mutation and every further
  launch blocked for the whole uncertainty interval;
- moves the launch state to `blocked`, because no honest running session can be described;
- leaves the play session open rather than claiming a closed `failed_to_start` or `completed`
  session underneath a child that may still be alive;
- polls until either the child is positively reaped or `ManagedProcessInspector` independently
  proves absence — the inspector clears the record itself in that case — and only then closes the
  session (`interrupted` for a monitored game, `failed_to_start` for a launch that never completed)
  and makes launching available again.

The same bounded polling adopts a process inherited from a previous application run, and it now also
covers a surviving record that names no honest running session, so a blocked state recovers on its
own once the child is gone instead of persisting for the rest of the run.

## Managed process record

Schema version 3. It adds `launch_id` and `play_session_id`, and its process identity is optional so
the pre-spawn `launching` phase can exist:

| Phase | `pid` / start ticks / observed executable | Liveness decision |
| --- | --- | --- |
| `launching` | must be absent | previous boot ⇒ dead; otherwise a bounded `/proc` scan for an executable inside `runtime/versions/` or *any* command-line element naming the authenticated AppRun |
| `running` | all required | boot id, `/proc/<pid>/stat` start ticks, and canonical `/proc/<pid>/exe` equality |

Only proof of absence removes the record. A PID/start-time match with a different executable stays
uncertain and blocking, an unsupported schema version stays blocking and undeleted, and PID alone is
never identity. The `/proc` scan deliberately over-detects: a false positive keeps mutation blocked,
a false negative would let an update run underneath a live emulator.

The PID-less scan matches the whole command line rather than `argv[0]`, because an AppRun may be a
`#!` script. Linux then runs the interpreter with a command line of the shape
`interpreter [optional-arg] script-path original-argv[1..]`: the original `argv[0]` is not
preserved, `/proc/<pid>/exe` resolves to a host interpreter outside the managed tree, and the AppRun
appears as an interpreter *argument*. The AppRun is a match key only while it belongs to the managed
versions tree — as it resolves now, or, when its installation was moved or removed underneath a live
process, as the absolute path the record already validated against that tree. Running-phase identity
is unchanged and is not weakened by any of this.

## Restart reconciliation

`LaunchApplicationService::reconcile_on_startup` runs after `RuntimeManager::startup_reconcile`:

| Record state | Result |
| --- | --- |
| absent | every open session is closed as `interrupted`; no running game |
| `running` with a live identity | the session stays `running`, the process is adopted and polled, and it is closed as `interrupted` once proven gone (no exit code is available for a process RetroFrontier did not fork) |
| `launching`, or `running` without an open session | nothing closed, nothing deleted, launches refused, `blocked` reported; the record and its session are released once the child is proven gone |
| unreadable or an unsupported schema | nothing closed, nothing deleted, launches refused, `blocked` reported; no verdict is possible, so nothing is polled |

SQLite play-session state never overrides the OS/process verdict.

## Persistence

`play_sessions` records game, content unit, core, runtime installation and release, timestamps, exit
code, and a constrained outcome (`running`, `completed`, `failed_to_start`, `crashed`,
`interrupted`). A session is open exactly while it has no end time, and a closed session keeps its
first verdict. No raw stderr or log blob is stored; RetroArch's log goes to `logs/retroarch/`.

`game_launch_overrides` stores one user-owned `CoreId` per game. Both tables use restrictive deletes
and sit apart from scanner-owned and provider-owned tables, so a rescan or a metadata refresh cannot
reset a core choice or delete history. An override-management UI is deferred.

## Normalized launch errors

`gameNotFound`, `gameUnavailable`, `contentSelectionRequired`, `contentUnavailable`,
`runtimeNotReady`, `corePolicyUnresolved`, `coreNotInstalled`, `coreNotApproved`, `biosMissing`,
`biosInvalid`, `biosNotCoveredByCatalog`, `hostPrerequisiteMissing`, `gameAlreadyRunning`,
`configPreparationFailed`, `spawnFailed`, `processIdentityFailed`, `processExitedDuringLaunch`,
`sessionPersistenceFailed`, `internalLaunchFailure`.

Every anticipated problem is a `LaunchResponse` with `status: "failed"`, not an IPC error. The
message is a fixed RetroFrontier sentence, and the typed context carries only identifiers React may
see (`systemId`, `coreId`, `biosRequirementIds`, `runtimeState`, `hostPrerequisite`, `exitCode`,
`contentOptions`). No path, `errno`, or OS error text ever reaches React.

## Manual qualification still required

The automated suite uses a synthetic shell `AppRun`, synthetic cores, and synthetic content; it
proves the launch architecture, not emulation. A real Linux qualification is required before any
public claim, and it needs a production Runtime Release, which ADR-012 still gates.

For each of NES, SNES, PlayStation, and GameCube, using only content and BIOS the tester legally
owns and never adding either to this repository:

1. Install the managed runtime and confirm `get_systems` reports the approved default core as
   available for the system under test.
2. For PlayStation, place one of `scph5500.bin`, `scph5501.bin`, or `scph5502.bin` in
   `Documents/RetroFrontier/BIOS` and confirm readiness reports it as present and valid; confirm a
   deliberately corrupted copy reports `biosInvalid` and refuses to launch.
3. Launch from Game Detail and confirm: the window opens, video and audio work, the controller is
   detected, and `runtime-user/config/retroarch.cfg` is the configuration actually in use.
4. While the game runs, confirm a runtime update/repair/rollback is refused and a second launch
   returns `gameAlreadyRunning`.
5. Save in-game, exit, and confirm the save lands under `saves/` and never beside the ROM.
6. Confirm RetroFrontier regains focus, the session is `completed`, and the UI returns to its normal
   detail state.
7. Kill RetroFrontier while the game runs, restart it, and confirm the game is still reported as
   running and runtime mutation stays blocked; then close the game and confirm the session becomes
   `interrupted` and mutation is allowed again.
8. For GameCube, confirm the managed Dolphin `Sys` link exists under `runtime-user/system/` and that
   no unrelated user Dolphin installation is consulted.
9. Repeat on the distribution matrix in `docs/spikes/LINUX_RUNTIME_QUALIFICATION.md`.
