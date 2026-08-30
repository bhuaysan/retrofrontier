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

- **bsnes-mercury Balanced was explicitly selected as the M7 SNES core.** Upstream treats `bsnes`
  and `bsnes-mercury` as separate core families, so this is a selection, not a naming clarification:
  RetroFrontier chose `bsnes_mercury_balanced_libretro` (from `libretro/bsnes-mercury`) because it
  is the currently qualified Balanced-profile artifact for this M7 decision. No equivalence with the
  separate `bsnes` core family is claimed. Whether the other family better serves V1 is open and is
  not part of the M7 scope.
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
  record is decided by a bounded `/proc` scan matching an executable inside `runtime/versions/` or
  any command-line element naming the authenticated AppRun, which covers a script AppRun and
  over-detects rather than under-detects. (This originally matched `argv[0]` only; see the
  corrective pass below for why that was unsafe.)
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

## Corrective pass (post-review)

A focused review of the M7 branch raised two HIGH process-safety findings, one bounded contract
correction, and one documentation clarification. All four are fixed on this branch; the durable
serialized record shape did not change, so the process-record schema stays at version 3, and
unsupported older/newer schemas stay blocking rather than being deleted.

| Commit | Subject |
| --- | --- |
| `8177dae` | fix(launch): detect a script AppRun during PID-less launch recovery |
| `71576aa` | fix(launch): never clear the process record while child death is unproven |
| `301d974` | fix(launch): validate a persisted core override against the whole contract |
| (this one) | docs: record the M7 corrective pass |

Preserved unchanged: PID + start-time + boot-ID identity for `running` records, authenticated
installation/AppRun containment, the RuntimeManager mutation-lock behaviour, ADR-011's pre-spawn
durable record ordering, and SQLite play sessions as history rather than process authority.

### Verification after the corrective pass

| Command | Result |
| --- | --- |
| `pnpm typecheck` | clean |
| `pnpm lint` | clean |
| `pnpm format:check` | clean |
| `pnpm test` | 353 passed across 23 files |
| `pnpm build` | built |
| `cargo fmt -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test` | **402 passed, 0 failed, 1 ignored** (was 393; +9) |
| `cargo test --lib adapters::runtime_process` | 13 passed (was 9) |
| `cargo test --lib application::launch` | 23 passed (was 18) |
| `cargo test --lib adapters::game_process` | 5 passed |
| `cargo test --lib repositories::launch` | 6 passed |
| `cargo test --lib adapters::database` | 8 passed (migrations; no persistence change was needed) |
| `git diff --check` | clean |

### HIGH-1 — PID-less `Launching` recovery was unsafe for a script AppRun

*Root cause.* `LinuxManagedProcessInspector::managed_process_exists` recognized a live managed child
only through `/proc/<pid>/exe` resolving inside `runtime/versions/`, or through `argv[0]` equalling
the authenticated AppRun. Neither holds for an AppRun implemented as a `#!` script. Linux executes
the interpreter instead, with a command line of the shape
`interpreter [optional-arg] script-path original-argv[1..]`; the script invocation's `argv[0]` is
not preserved. A shebang AppRun that survived a RetroFrontier crash therefore had an executable
outside the managed tree *and* an `argv[0]` that was the interpreter, the scan returned "absent",
`game-process.json` was cleared, and runtime mutation could run underneath a live emulator — a
fail-closed violation of M7 and ADR-011.

*Fix.* The scan now matches the AppRun against *every* command-line element, not `argv[0]`, so the
script pathname is found where the kernel actually puts it — as an interpreter argument. Nothing is
special-cased to `/bin/sh`. Containment is preserved and tightened: the AppRun is a match key only
while it belongs to the managed versions tree, either as it resolves now or — when its installation
was moved or removed underneath a still-running process — as the absolute path
`write_process_record` had already validated against that tree. A record naming a host path can
therefore never make an arbitrary process look managed. Command lines are read with a size cap, and
per-argument canonicalization is limited to arguments that could name the same file. Running-phase
identity (PID + start ticks + canonical `/proc/<pid>/exe`) is untouched.

### HIGH-2 — the durable record was cleared while child death was uncertain

*Root cause.* Two paths deleted `game-process.json` without proof of death.

- The fresh-child monitor treated an error from `child.wait()` as `Interrupted` and then cleared the
  record. A failed wait proves nothing about the child, so a live emulator could be forgotten and
  runtime mutation unblocked underneath it. `SpawnedGame::wait` also consumed the handle, so the
  child could not even be re-checked.
- After spawn, when process identity could not be established or durably written, the code called
  `child.terminate()` and cleared the pre-spawn `Launching` record regardless of whether termination
  had succeeded — and closed the play session as `failed_to_start` while the child might still be
  running.

*Fix.* `clear_process_record` is now reached from `LaunchApplicationService` only through
`clear_record_after_proven_death`, whose contract states the invariant: the caller already holds
proof that no managed child survives — either no process was ever created, or its exit was
positively observed and reaped. Every remaining call site satisfies it (spawn failed before a child
existed; the settle window reaped an exit; the monitor reaped an exit; a positively reaped
`terminate`). `SpawnedGame::wait` borrows instead of consuming, and `terminate` documents that only
`Ok` is a positive observation.

Where death is unproven the record stays, launch state becomes `blocked`, the session stays open,
and the new `watch_until_absent` polls until either the child is positively reaped or
`ManagedProcessInspector` independently proves absence and clears the record itself. It then closes
the session — `interrupted` for a monitored game, `failed_to_start` for a launch that never
completed — and makes launching available again. It replaced `watch_adopted_process` and now also
covers the blocked branch of restart reconciliation, so a surviving record recovers on its own
instead of blocking for the rest of the application run. Runtime mutation and any further launch
stay blocked for the entire uncertainty interval.

### MEDIUM-1 — persisted core overrides did not enforce the approved contract

*Root cause.* `set_core_override` checked only `SystemCatalog::approves_core_for_system`. The
approved contract also requires the core to map to an authenticated managed runtime component, to be
currently verified as installed, and to be approved for that system by the authenticated release.
The launch path revalidated all of it, so this was never a security bypass, but invalid or stale
override state could be persisted contrary to the contract.

*Fix.* Persistence runs the same `RetroArchService::resolve_core` boundary the launch uses, rather
than duplicating the core-selection algorithm, so the two cannot drift apart and no arbitrary core
path is introduced. Launch-time revalidation is unchanged and still authoritative.

### Tests added

`adapters::runtime_process`

- a shebang AppRun whose interpreter is outside the managed versions tree: a schema-3 PID-less
  `Launching` record is written before spawn, the script is spawned and confirmed to be running
  under a host interpreter (asserting both that `/proc/<pid>/exe` is outside the tree and that
  `argv[0]` is not the AppRun), `ensure_no_active_game()` reports `GameActive`, the durable record
  survives, and only after the child is terminated and reaped is the record cleared;
- the same, with the installation renamed away underneath the live child;
- an ordinary symlinked AppRun resolving to an ELF payload inside the installation;
- an AppRun path outside the managed tree is never a match key.

The pre-existing copied-`/bin/sh` test is retained: it covers the ordinary in-tree ELF case, which
its own `/proc/<pid>/exe` satisfies.

`application::launch`

- a monitor that can observe neither `wait()` nor `try_wait()`: the record survives, mutation stays
  blocked, a second launch is refused, the session stays open, and only proven absence clears the
  record, closes the session as `interrupted`, and makes launching available again;
- a child that cannot be terminated while the Running-record transition fails: the pre-spawn
  `Launching` record survives unchanged and PID-less, mutation stays blocked, a second launch is
  refused, the session stays open, and proven absence then reconciles record and session and
  restores launching;
- the core-override persistence contract — approved and verified-installed persists; approved but
  uninstalled, a component that does not approve the system, an unresolvable component, an
  unapproved core, an unknown core, and an unresolved system all refuse;
- launch-time revalidation still refuses a stored override whose core disappeared.

Both uncertainty tests use test-only fault injection on `SpawnedGame`, so nothing depends on forcing
the OS `waitpid`/`kill` calls to fail by chance.

### Pre-existing test flake, diagnosed and deliberately left

Running the launch module on its own (`cargo test --lib application::launch`) intermittently fails
one of the *pre-existing* launch tests with `runtimeNotReady`: 5 of 20 runs, always
`a_foreign_or_unlaunchable_content_unit_is_never_started` or
`a_missing_or_unavailable_game_is_refused_before_anything_is_started`, never one of the tests added
here. It reproduces identically on the pre-corrective HEAD `4cb60dc`, so it is not introduced by
this pass. The full `cargo test` run did not reproduce it in 8 consecutive runs (402 passed each
time), because the scheduling is different when the whole suite shares the thread pool.

Cause: `flock` is inherited across `fork()`. While one test holds its runtime-mutation lock, a
*concurrent* test's process spawn briefly holds a duplicate of that lock between `fork` and `exec`,
so the first test's next `try_lock` returns `WouldBlock` and the launch reports `runtimeNotReady`.

It was left as is at the time, on the grounds that removing it would mean either serializing the
tests behind a new dependency or adding a retry to a safety primitive's `try_lock`. Neither turned
out to be necessary — see **Final stabilization** below, which resolves it and corrects the second
half of the diagnosis above.

## Final stabilization

### Symptom

`cargo test` fails intermittently — 2 of 20 full parallel runs, and 5 of 20 when the launch module
is run alone — always in a launch test that calls `launch_game` twice, and always the same way: the
*second* call returns `runtimeNotReady` instead of the domain outcome the test asserts.

```
test application::launch::tests::a_missing_or_unavailable_game_is_refused_before_anything_is_started ... FAILED
assertion `left == right` failed
  left: RuntimeNotReady
 right: GameUnavailable
```

### Confirmed root cause

`fs4` implements the lock as `flock(2)`, which is owned by the **open file description**, not by a
descriptor and not by a process. Closing one descriptor releases the lock only if no copy of that
open file description survives.

`RuntimeMutationLock` releases by dropping its `File`, i.e. by closing its descriptor. Every launch
test harness owns a real lock over its own temporary runtime root, and all of them live in one test
binary. When any parallel test spawns a child, the `fork` copies the whole process descriptor table
into that child — including *other* harnesses' lock descriptors — and those copies survive until the
child reaches `execve`. So:

1. harness A takes its mutation lock in `launch_locked`;
2. harness B, on another thread, spawns a synthetic AppRun child, which copies A's lock descriptor;
3. A finishes and drops its lock — the kernel does **not** release it, because B's not-yet-`exec`ed
   child holds a copy of the same open file description;
4. A's next `launch_game` calls `RuntimeMutationLock::acquire` on its own path, gets `WouldBlock`,
   and `launch_locked` maps that to `LaunchErrorCode::RuntimeNotReady`.

The earlier diagnosis had step 3 backwards: the copy does not contend with a lock A still holds, it
keeps A's lock alive *after* A has released it. That is why only the second launch in a test fails.

Proven directly, independently of Rust:

```
exec 9>x.lock; flock -n 9          # owner acquires
( sleep 3 ) &                      # a forked child inherits fd 9 and never execs it away
exec 9>&-                          # owner closes its descriptor
flock -n x.lock -c true            # -> fails: STILL HELD by the inherited copy
flock -u 9 (before closing)        # -> then it is FREE immediately
```

### Why this is a test-only defect

Production has one application instance, one runtime root, and one mutation-lock path. There is no
second, unrelated lock descriptor in the process for a spawn to strand. Production forks only from
`LinuxGameProcessLauncher::spawn`, under the same lock the launch service already owns, so a copy
can only extend the lifetime of a lock the application itself holds — and a runtime mutation that
loses that race is told the runtime is busy, which is true and fails closed. The defect is entirely
an artifact of many unrelated temporary runtime roots sharing one forking test process.

### Fix

One `#[cfg(test)]` `Drop` on `RuntimeMutationLock` that releases the open file description
explicitly instead of relying on being the last descriptor:

```rust
#[cfg(test)]
impl Drop for RuntimeMutationLock {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.file);
    }
}
```

`flock(LOCK_UN)` acts on the open file description, so it releases the lock however many copies of
the descriptor exist. Plus a `#[cfg(test)] duplicate_descriptor()` accessor used only to set the
condition up in the regression tests.

### Production locking semantics did not change

Both additions are `#[cfg(test)]`, so neither exists in a non-test build; the production release
path is still "drop the `File`". Nothing else changed: `acquire` is still a single non-blocking
`try_lock`, `WouldBlock` is still a hard failure, there are no retries, no sleeps, no blocking lock,
no global mutex, and the ADR-011 ordering — mutation lock taken before verification and held across
the spawn — is untouched. `runtime_lock.rs` production code, `LaunchApplicationService`,
`RuntimeManager`, `RetroArchService` and the process-record schema are byte-identical.

Release-by-close itself cannot be asserted from inside this binary, and an attempt to do so was
removed: a test that locks a bare descriptor and closes it has no way to own "the last descriptor",
because any parallel test may copy it. That attempted test failed on run 13 of the stress gate — a
direct, independent confirmation of the root cause.

### Deterministic regressions added

Neither depends on timing; both create the inherited copy with `try_clone`, which duplicates the
open file description exactly as `fork` does.

- `adapters::runtime_lock::tests::releasing_the_lock_does_not_depend_on_being_the_last_descriptor`
  — acquire, duplicate, drop the owner, re-acquire.
- `application::launch::tests::a_descriptor_inherited_by_a_parallel_test_child_cannot_strand_the_mutation_lock`
  — the same condition through the real launch path, asserting the exact symptom: two consecutive
  `launch_game` calls must return `GameNotFound` and `GameUnavailable`, not `runtimeNotReady`.

Before the fix both fail, the launch one with `left: RuntimeNotReady, right: GameNotFound`. After
the fix both pass.

### Verification

Reproduction before the fix, on the unmodified starting tree `f65515b`:

| Command | Result |
| --- | --- |
| `cargo test --manifest-path Cargo.toml --lib` × 20 | 2 failures — runs 7 and 10 |

Both failures were the second `launch_game` of a two-launch test returning `RuntimeNotReady`:
`a_missing_or_unavailable_game_is_refused_before_anything_is_started` (expected `GameUnavailable`)
and `a_foreign_or_unlaunchable_content_unit_is_never_started` (expected `GameUnavailable`).

After the fix:

| Command | Result |
| --- | --- |
| `cargo test --manifest-path Cargo.toml` × 50, ordinary parallel | **50 / 50 consecutive clean**, 0 failures |
| `cargo test --manifest-path Cargo.toml --lib application::launch` × 30 | 30 / 30 clean |
| `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | clean |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | clean |
| `pnpm typecheck` / `lint` / `format:check` / `build` | clean |
| `pnpm test` | 353 passed in 23 files |

The Rust suite is 405 tests (404 passed, 1 ignored) per run: 403 before this pass plus the two
deterministic regressions. No existing test was changed or removed — the diff is `+84 / -0` across
`runtime_lock.rs` and `launch.rs`, and every added item is `#[cfg(test)]`.

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
