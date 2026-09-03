# M9 Saves and Save States Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking. Follow red-green-refactor: every production behaviour change starts with a failing test that demonstrates the missing behaviour.

**Goal:** Make RetroArch save states first-class RetroFrontier domain objects whose provenance is *proved* by a controlled launch and whose physical identity is re-proved before every read, load, or destructive action. Normal SaveData stays opaque, RetroFrontier-located, and uninterpreted.

**Architecture:**

```text
React (useSaveStates / SaveStatesSection on Game Detail)
  -> list_save_states / load_save_state / delete_save_state          (SaveStateId only)
  -> SaveStateApplicationService            (application/save_state.rs)
       -> SaveStateRepository               (repositories/save_state.rs)   provenance + lifecycle + baselines
       -> SaveStateFilesystem               (services/save_state_fs.rs)    RetroArch 1.22.2 layout, snapshots,
       |                                                                   stability, hashing, no-follow verify/delete
       -> RuntimeManager                    (exact authenticated core binary + trust)
       -> LibraryRepository                 (game / content unit validity)
       -> LaunchApplicationService          (the one managed launch pipeline)
rfmedia://localhost/save-state-thumbnail/<SaveStateId>  -> SaveStateThumbnailDelivery
```

`LaunchApplicationService` gains one launch *plan* parameter and one `SaveStateLifecycle` collaborator; it does not gain a second process launcher. `RuntimeManager` gains exact authenticated core-binary identity in its existing verified projection plus one lookup entry point; it gains no new trust model.

**Tech Stack:** Rust (Tauri 2, sqlx/SQLite, tokio, sha2, `rustix` for no-follow filesystem primitives), React 19 / TypeScript 6 / Vitest, existing design tokens and IPC conventions.

**Approved design:** the M9 architecture in the milestone brief. `docs/adr/` and the implemented architecture remain authoritative; `docs/design/screens/B6`/`B7` are visual handoff only.

---

## Global Constraints

- Work only on `feat/m9-save-states`; base commit `a5c351fb08634ff85180a9780b0c7b0dc5ab750d`. Do not merge, push, or rewrite history. Preserve the ignored root `*REVIEW*.md` / `*REPORT*.md` artifacts and `BIOS/`, `.env` exactly as they are.
- **Provenance is never derived from a filename.** A managed Save State exists because a controlled launch proved its provenance and RetroFrontier then verified the exact physical bytes.
- **A `SaveStateId` never authorizes a path.** Every load and delete re-proves the current filesystem target before acting.
- Normal SaveData (`saves/`) is never enumerated, sliced into slots, exposed as files, deleted, or subjected to compatibility logic.
- Supported slots are `1..=999` only. Slot 0 (`<base>.state`) and AUTO (`<base>.state.auto`) are neither imported nor managed.
- Legacy/orphan/unprovable states stay on disk, untouched, invisible, unloadable, undeletable. No recovery or orphan screen, no manual assignment UI, no heuristic importer.
- Never claim `compatible`. The permitted-load concept is `loadable`, and a failed load never marks a state corrupt.
- Runtime security policy always wins: a revoked, blocked, or below-floor component is never reactivated to load a state, and Save States never pin a runtime release.
- React never supplies or receives a state path, thumbnail path, core path, runtime path, SHA, slot, or CoreId as a *request* input.
- No new global filesystem watcher. Reconciliation is a consequence of the existing managed RetroArch lifecycle plus startup reconciliation.
- Do not start M10 packaging work, do not add SaveData browsing, and do not perform unrelated refactors.
- No ROM, BIOS, runtime payload, database, log, or generated installation may enter Git. All fixtures are synthetic.
- Prefer small reviewable commits; run focused tests continuously and the full matrix at the end.

## Verified repository facts this plan is built on

Established by inspection at the base commit; every one of them is load-bearing.

| Fact | Evidence |
| --- | --- |
| The authenticated release manifest carries a mandatory per-file SHA-256 for every core component's executable | `src-tauri/src/domain/runtime.rs:639` (`InstalledEntry.sha256`), `:930-960` (a component executable must exist in the inventory as an executable file) |
| `RuntimeComponent.sha256` is the *archive* digest, not the binary digest | `src-tauri/src/domain/runtime.rs:601-620` |
| `verify_installation` re-hashes every installed file against that inventory | `src-tauri/src/adapters/runtime_installed.rs:162-250` |
| `VerifiedLaunchRuntime.cores` currently drops the binary digest, display version, and source revision | `src-tauri/src/application/runtime_manager.rs:572-606` |
| Every trusted, verified installation can be enumerated | `RuntimeManager::list_verified_installations`, `src-tauri/src/application/runtime_manager.rs:723-780` |
| Retention keeps two installations by default, so a historical core binary can legitimately disappear | `src-tauri/src/application/runtime_manager.rs:28-50`, `:867-899` |
| `sort_savestates_enable = true` and `savestate_directory = <appData>/states` are already generated | `src-tauri/src/services/retroarch_config.rs:85,132` |
| The real managed runtime already produced `states/Nestopia/`, `states/bsnes-mercury/`, `states/dolphin-emu/` — core-reported `library_name`, **not** RetroFrontier `CoreId`s (`nestopia`, `bsnes-mercury-balanced`, `dolphin`) | local app-data inspection; confirms the §12 warning empirically |
| The managed RetroArch 1.22.2 binary contains `savestate_thumbnail_enable`, `state_slot`, `enable_hotkey`, `state_slot_increase`, `state_slot_decrease`, `entryslot`, `.state`, `.auto`, `(.state.auto)`, `_btn`, and `-e, --entryslot=NUMBER  Slot from which to load an entry state.` | `strings` over `runtime/retroarch/usr/bin/retroarch` in the installed managed runtime |
| The authenticated `joypad-autoconfig` component ships 420 `udev` profiles; the qualified DualSense and DualSense Edge profiles agree exactly: `input_select_btn="8"`, `input_r_btn="5"`, `input_left_btn="h0left"`, `input_right_btn="h0right"` | `runtime/support/joypad-autoconfig/udev/Sony Interactive Entertainment DualSense*.cfg` |
| `RelativePath` already is a validated safe relative-path newtype (no absolute, no `\`, no `.`/`..`, no NUL, no control chars, ≤4096) | `src-tauri/src/domain/runtime.rs:326-382` |
| The app already delivers app-owned images to the WebView by *domain identity* over a custom protocol, never by path | `src-tauri/src/services/media_delivery.rs`, `src-tauri/src/domain/library.rs:408-419`, `src-tauri/src/lib.rs:253-277` |
| `rustix 1.1.4` and `libc 0.2.189` are already resolved transitively | `src-tauri/Cargo.lock:3841`, `:2575` |
| `adapters/database.rs` pins the migration count at 6 and must become 7 | `src-tauri/src/adapters/database.rs:151,168` |
| `context` is Standard Gamepad button 2; button 3 is already `search` | `src/input/gamepadAdapter.ts:11-19`, `docs/CONTROLLER_AND_FOCUS.md` §"Reaching Search" |

### Two recorded deviations, decided before implementation

1. **Options is the semantic `context` action, so its physical button is X/Square (index 2), not Y.** §32 opens with "Follow the existing M8 semantic focus architecture", and in that architecture button 3 is already the Library `search` exit (ADR-014 concentrates the button table in one file on purpose). B7's `Y OPTIONEN` predates M8 and is visual handoff only. The *semantic* intent of §32 — a card-scoped Options action distinct from `confirm` and `back` — is implemented exactly; only the letter in the footer differs, and it is derived, not hard-coded.
2. **Managed save-state hotkeys are written for the qualified managed controller path only.** RetroArch hotkey binds are one global set of raw joypad values, so one profile's numbers can be written per launch. RetroFrontier derives them from the authenticated qualified profiles and refuses to write any hotkey if those profiles disagree or a required role is absent. An unresolved profile omits the hotkeys and logs; it never blocks a launch, because M7/M8 launch behaviour must not regress. Broader per-controller hotkey coverage stays B10 work.

Both are recorded again in the final report and in `docs/SAVE_STATES.md`.

---

### Task 1: Save-State domain

**Files:** `src-tauri/src/domain/save_state.rs` (new), `src-tauri/src/domain/mod.rs`

- [ ] **Step 1 (RED):** Assert `SaveStateSlot::new` accepts `1..=999`; rejects `0` and `1000`; that there is no constructor for AUTO at all; that a `SaveStateSlot` is not usable as an identity (two `SaveState` values with the same slot but different `core_binary_sha256` are distinct and both representable); that `SaveStateStatus` round-trips through `as_db`/`from_db` for `available`/`missing`/`superseded`/`deleted` and rejects an unknown value; that `SaveStateLoadability` and `SaveStateError` serialize to their stable camelCase codes; that `SaveStateView` serializes **no** `sha256`, no absolute path, and no `stateRelativePath`; and that `SaveStateProvenance` exposes no setter for `core_binary_sha256`.
- [ ] **Step 2:** Add `SaveStateId(pub i64)`, `SaveStateSlot`, `SaveStateStatus`, `SaveStateProvenance`, `SaveStateFileIdentity`, `SaveStateThumbnailIdentity`, `SaveStateCapabilities`, `SaveStateLoadability`, `SaveState`, `SaveStateView`, `LaunchStateBaseline`, `LaunchStateBaselineEntry`, and `SaveStateError`.
  - `SaveStateError` variants: `NotFound`, `Unavailable`, `CoreUnavailable`, `TemporarilyBlocked`, `IntegrityMismatch`, `UnsafeFilesystemTarget`, `ReconciliationFailed`, `LaunchFailed`, `DeleteFailed`. Each carries a stable camelCase `code()` and a fixed safe `message()`. **No `Corrupt` variant exists**, and the `IntegrityMismatch` message says the registered identity no longer matches, never that the file is corrupt.
  - Reuse `domain::runtime::{RelativePath, Sha256Digest, SafeIdentifier}`. Do **not** introduce a parallel safe-path or digest type; map `RelativePath::new` failures to `SaveStateError::UnsafeFilesystemTarget` at the boundary. Record that decision in a module comment.
  - `SaveState` fields exactly as the approved model: `id`, `game_id`, `content_unit_id`, `play_session_id`, `core_id`, `core_component_id`, `core_binary_sha256`, `core_display_version`, `core_source_revision`, `originating_runtime_release_id`, `slot`, `state`: `SaveStateFileIdentity`, `thumbnail`: `Option<SaveStateThumbnailIdentity>`, `created_at`, `updated_at`, `status`.
  - Filesystem naming conventions are **not** in this module. No `.state`, `.stateN`, `.png`, or `library_name` string appears in `domain/save_state.rs`; a test asserts the module source contains none of them.

### Task 2: Save-State and baseline persistence

**Files:** `src-tauri/migrations/20260903000000_save_states.{up,down}.sql` (new), `src-tauri/src/repositories/save_state.rs` (new), `src-tauri/src/repositories/mod.rs`, `src-tauri/src/adapters/database.rs`

- [ ] **Step 1 (RED):** Migration tests — forward migration from the M8.5 schema and the down migration; three `RESTRICT` foreign keys on `save_states` (`games`, `content_units`, `play_sessions`) and `RESTRICT` on both baseline tables; the `slot BETWEEN 1 AND 999` check refuses `0` and `1000`; the status check refuses an unknown value; the all-or-nothing thumbnail check refuses a path with no digest and a digest with no path; the 64-character digest checks; `idx_save_states_session_identity` refuses a duplicate `(play_session_id, state_relative_path, state_sha256)`; `idx_save_states_available_path` refuses two `available` rows for one `state_relative_path` while permitting a `superseded` predecessor beside its successor; and `pragma_foreign_key_list` shows no cascade anywhere.
- [ ] **Step 2 (RED):** Repository tests — insert and read back complete provenance byte-for-byte; `available → missing`, `available → superseded`, `available → deleted` transitions; a closed lifecycle value is never silently reopened; `core_binary_sha256` has no update path at all (the repository exposes no method that writes it after insert); baseline persist/load/delete round-trips including entries; `attempts` increments; a baseline survives a fresh repository over the same file (restart recovery); replaying `register_reconciled_state` with the same session and the same physical identity produces **no** second row; and `save_states_for_game` returns only `available` rows ordered `updated_at DESC, id DESC`.
- [ ] **Step 3:** Write the migration.

  ```sql
  CREATE TABLE IF NOT EXISTS save_states (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      game_id INTEGER NOT NULL,
      content_unit_id INTEGER NOT NULL,
      play_session_id INTEGER NOT NULL,
      core_id TEXT NOT NULL,
      core_component_id TEXT NOT NULL,
      core_binary_sha256 TEXT NOT NULL CHECK (length(core_binary_sha256) = 64),
      core_display_version TEXT,
      core_source_revision TEXT,
      originating_runtime_release_id TEXT NOT NULL,
      slot INTEGER NOT NULL CHECK (slot BETWEEN 1 AND 999),
      state_relative_path TEXT NOT NULL,
      state_sha256 TEXT NOT NULL CHECK (length(state_sha256) = 64),
      state_size INTEGER NOT NULL CHECK (state_size >= 0),
      thumbnail_relative_path TEXT,
      thumbnail_sha256 TEXT CHECK (thumbnail_sha256 IS NULL OR length(thumbnail_sha256) = 64),
      thumbnail_size INTEGER CHECK (thumbnail_size IS NULL OR thumbnail_size >= 0),
      status TEXT NOT NULL
          CHECK (status IN ('available', 'missing', 'superseded', 'deleted')),
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL,
      -- A thumbnail is proved as a whole or not at all.
      CHECK ((thumbnail_relative_path IS NULL) = (thumbnail_sha256 IS NULL)),
      CHECK ((thumbnail_relative_path IS NULL) = (thumbnail_size IS NULL)),
      FOREIGN KEY (game_id) REFERENCES games (id) ON DELETE RESTRICT,
      FOREIGN KEY (content_unit_id) REFERENCES content_units (id) ON DELETE RESTRICT,
      FOREIGN KEY (play_session_id) REFERENCES play_sessions (id) ON DELETE RESTRICT
  );
  CREATE UNIQUE INDEX IF NOT EXISTS idx_save_states_session_identity
      ON save_states (play_session_id, state_relative_path, state_sha256);
  CREATE UNIQUE INDEX IF NOT EXISTS idx_save_states_available_path
      ON save_states (state_relative_path) WHERE status = 'available';
  CREATE INDEX IF NOT EXISTS idx_save_states_game_recent
      ON save_states (game_id, updated_at DESC, id DESC);
  ```

  Plus `launch_state_baselines` (primary key `play_session_id`, the same provenance columns, `runtime_installation_id`, `runtime_release_id`, `entry_count`, `attempts`, `captured_at`, all foreign keys `RESTRICT`) and `launch_state_baseline_entries` (`play_session_id`, `relative_path`, `size_bytes`, `mtime_nanos`, `inode`, primary key on the first two, `RESTRICT` to the header). Entries are deleted explicitly before the header inside one transaction, so the repository keeps the project-wide no-cascade convention with no exception.

  Baseline entries deliberately store **size + mtime + inode, not a digest**: the approved reconciliation order computes SHA-256 at step 7, after the process ended, and pre-hashing an entire state tree before every launch would add unbounded launch latency for no provenance gain. The residual risk (a size-, mtime- and inode-preserving external rewrite is invisible to the delta) is accepted, fails *closed* — the file is simply never attributed — and is recorded in the security review.
- [ ] **Step 4:** Implement `SaveStateRepository`: `save_states_for_game`, `save_state`, `register_reconciled_state`, `refresh_reconciled_state`, `mark_missing`, `mark_superseded`, `mark_deleted`, `available_states`, `put_baseline`, `baseline`, `baselines_awaiting_reconciliation`, `increment_baseline_attempts`, `delete_baseline`. Insert/refresh run inside one transaction each. No method accepts or writes `core_binary_sha256` for an existing row.
- [ ] **Step 5:** Update the migration-count assertions in `src-tauri/src/adapters/database.rs` from 6 to 7 and extend the existing forward/down migration test to cover the new tables.

### Task 3: Exact authenticated core-binary provenance

**Files:** `src-tauri/src/application/runtime_manager.rs`, `src-tauri/src/application/runtime.rs`, `docs/RUNTIME_MANAGER.md`

- [ ] **Step 1 (RED):** Assert `verified_launch_runtime()` now exposes, for every authenticated core component, the exact `binary_sha256` and `binary_size_bytes` **taken from the authenticated manifest inventory entry for that component's executable** together with `display_version` and `source_revision`; that a component whose executable has no inventory entry is omitted rather than hashed off disk; that the digest equals the inventory value and is never recomputed from an arbitrary `.so`; and that `installation_id`/`release_id` are still reported.
- [ ] **Step 2 (RED):** Assert `locate_authenticated_core_binary(component_id, sha256)`
  - returns the active installation's binary when it matches;
  - finds the exact binary in **another** currently installed, trusted, fully verified installation when the active one no longer carries it, and reports that installation's own `installation_id`/`release_id`;
  - refuses a matching `component_id` whose binary digest differs (`CoreUnavailable`, never a substitution);
  - refuses an installation the persisted trust state does not permit — revoked release, below the security floor, unknown manifest digest — even when the bytes on disk match;
  - refuses when no installation carries the binary;
  - never resurrects, re-activates, re-downloads, or pins an installation, and never changes `active.json`.
- [ ] **Step 3:** Add `binary_sha256`, `binary_size_bytes`, `display_version`, `source_revision` to `ManagedCoreComponent`; project them in `verified_launch_runtime()` by looking the component executable's path up in `manifest.release.inventory` (the same map `verify_tree` already trusts). Add `AuthenticatedCoreBinary { component_id, core_id_hint: Option<CoreId>, core_path, binary_sha256, binary_size_bytes, display_version, source_revision, installation_id, release_id, systems }` and `RuntimeManager::locate_authenticated_core_binary`, reusing `list_verified_installations_with_state` so trust is recomputed and never cached. Expose it through `RuntimeApplicationService`.
- [ ] **Step 4:** Update the *Verified runtime snapshot* and *Launch boundary* sections of `docs/RUNTIME_MANAGER.md`, and state explicitly under *Retention and rollback* that Save States never pin a runtime release: routine cleanup may remove the only authenticated copy of a historical core, the state is preserved and stays visible while its own file is valid, and only its Load action becomes unavailable.

### Task 4: RetroArch save-state filesystem adapter

**Files:** `src-tauri/src/services/save_state_fs.rs` (new), `src-tauri/src/services/mod.rs`, `src-tauri/Cargo.toml`

All RetroArch 1.22.2 layout knowledge lives here and nowhere else.

- [ ] **Step 1 (RED) — candidate parsing, pinned as adapter contract tests:** Assert `parse_state_candidate` maps `Nestopia/Synthetic.state1` → slot 1, `…​.state999` → slot 999, and `…​.state42` → slot 42; returns *unsupported* for `…​.state` (slot 0), `…​.state.auto` (AUTO), `…​.state0`, `…​.state1000`, `…​.state01`, `…​.state+1`, `…​.state-1`, `…​.stateN` with a non-ASCII digit, `…​.sav`, `…​.srm`, and a bare `Synthetic`; maps `Nestopia/Synthetic.state1.png` → a thumbnail *of* `Nestopia/Synthetic.state1`; and that a name whose per-core directory is `Nestopia` is never reverse-mapped to a `CoreId` — the parse result carries **no** core field at all. Name the test module `retroarch_1_22_2_contract` and document that a future Runtime upgrade must break it deliberately.
- [ ] **Step 2 (RED) — containment and file-type safety:** Using a temp states root, assert `verify_managed_state_file` rejects an absolute stored path, a `..` component, a path whose final component is a symlink (to a file inside the root *and* to a file outside it), a path traversing a **symlinked intermediate directory**, a directory, a FIFO, a hard-linked file (`st_nlink > 1`), a size mismatch, and a digest mismatch; and accepts a plain regular file with the expected size and digest. Assert the returned handle's digest was computed from the opened descriptor.
- [ ] **Step 3 (RED) — snapshot and delta:** Assert `snapshot_state_tree` enumerates only regular files, records `(relative_path, size, mtime_nanos, inode)`, skips symlinks and non-regular entries, reports whether the enumeration was **complete**, and refuses (as incomplete) rather than guessing when a subdirectory cannot be read. Assert `state_tree_delta(baseline, snapshot)` reports a new file, reports a changed file (size, mtime, or inode differing), ignores an unchanged file, and ignores a file that vanished.
- [ ] **Step 4 (RED) — stability:** Assert a `StabilityProbe` observing two identical `(size, mtime, inode)` readings reports stable; that a changing size reports unstable; that a vanished file reports unstable; and that the deterministic test probe makes both outcomes reachable without sleeping. Production uses `PollingStabilityProbe { samples: 3, interval: 120ms }` behind the same trait.
- [ ] **Step 5 (RED) — thumbnail proof:** Assert a thumbnail is associated only when `<state relative path>.png` is itself part of *this* session's delta, is stable, and verifies as a regular file inside the root; that a pre-existing `.png` untouched by the session is **not** associated; that a `.png` for a *different* state is not associated; and that a file under `screenshots/` is never associated by any code path (the adapter is given only the states root and there is no screenshots accessor).
- [ ] **Step 6 (RED) — safe deletion:** Assert `delete_verified_state_file`
  - deletes exactly the verified regular file and returns success;
  - refuses and deletes nothing when the final component became a symlink, a directory, or a different inode between registration and the call;
  - refuses and deletes nothing on a size or digest mismatch;
  - refuses and deletes nothing for a `..` path or an absolute path;
  - **TOCTOU:** given an injectable hook that replaces the path with a different file *after* the verifying open and *before* the destructive step, deletes nothing, restores the original name, and returns `UnsafeFilesystemTarget`;
  - leaves no `.rf-delete-*` quarantine file behind on either the success or the refusal path;
  - and that `sweep_delete_quarantine` removes a leftover quarantine file from a simulated crash and touches nothing that parses as a state or a thumbnail.
- [ ] **Step 7:** Add `rustix = { version = "1.1", features = ["fs"] }` to `src-tauri/Cargo.toml` — already resolved transitively at `1.1.4`, so no new supply-chain surface — and implement the adapter.
  - Path resolution walks the states root by **directory handle**: `open(root, O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC)`, then `openat(dir, component, O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC)` per intermediate component, then `openat(dir, name, O_RDONLY|O_NOFOLLOW|O_CLOEXEC)`. `ELOOP` anywhere is a refusal. Nothing is canonicalized and then re-opened by absolute path.
  - Verification reads `fstat` on the descriptor: `S_ISREG`, `st_nlink == 1`, `st_size == expected`, and the digest is streamed from that descriptor. `(st_dev, st_ino)` is recorded.
  - Deletion satisfies *"deletes exactly the previously verified regular file, or deletes nothing"* with a **same-directory quarantine rename**: `renameat(dir, name, dir, ".rf-delete-<pid>-<counter>")`, then `openat` the quarantine name `O_NOFOLLOW` and require `(st_dev, st_ino, st_size)` to equal the recorded values. Equal ⇒ the verified inode now sits at a name nothing else can target, so `unlinkat(dir, quarantine, 0)` deletes exactly it. Unequal ⇒ rename back and refuse; nothing was deleted. A quarantine name never parses as a state or a thumbnail, so a crash between rename and unlink leaves an inert file that `sweep_delete_quarantine` removes.
  - The whole no-follow module is `#[cfg(unix)]`. The `#[cfg(not(unix))]` stub returns `UnsafeFilesystemTarget` unconditionally and carries a comment naming the platform limitation, so Windows and macOS fail closed rather than weakening the check. Record the limitation in `docs/SAVE_STATES.md`.

### Task 5: Managed save-state configuration and controller hotkeys

**Files:** `src-tauri/src/services/retroarch_input.rs` (new), `src-tauri/src/services/retroarch_config.rs`, `src-tauri/src/services/retroarch.rs`, `src-tauri/src/services/mod.rs`

- [ ] **Step 1 (RED) — hotkey derivation from authenticated profiles:** Using a synthetic authenticated profile tree, assert `resolve_managed_save_state_hotkeys`
  - reads the qualified managed controller profiles from the verified immutable `joypad-autoconfig` component and nothing else — no host location, no `~/.config/retroarch`, no `/usr/share/libretro`;
  - derives `enable_hotkey` from `input_select_btn`, `save_state` from `input_r_btn`, `slot_increase` from `input_right_btn`, `slot_decrease` from `input_left_btn`, preserving the profile's own value form including hat notation (`h0right`);
  - returns **no** hotkeys when the qualified profiles disagree on any of those four roles, when a role is absent, when a profile file is a symlink, or when the component is absent — and never substitutes a numeric constant;
  - uses no universal button-index table: a test rewrites the synthetic profile to different numbers and asserts the derived values follow the profile;
  - and that an unresolved hotkey set does not fail a launch.
- [ ] **Step 2 (RED) — generated configuration:** Assert the generated `retroarch.cfg`
  - keeps `savestate_directory` pointing at the RetroFrontier-owned `states/` root, `savestates_in_content_dir = false`, `savestate_auto_save = false`, `savestate_auto_load = false`;
  - adds `savestate_thumbnail_enable = true`;
  - sets `state_slot = "1"` for a normal launch and `state_slot = "<stored slot>"` for a save-state launch;
  - writes `input_enable_hotkey_btn`, `input_save_state_btn`, `input_state_slot_increase_btn`, `input_state_slot_decrease_btn` with exactly the derived profile values when a hotkey set resolved;
  - writes **no** `input_load_state_btn` and no other `input_*load_state*` key under any input, because there is deliberately no RetroFrontier ingame Load hotkey;
  - omits all four hotkey keys entirely when no hotkey set resolved, rather than writing a guessed value;
  - and keeps every existing M7/M8 assertion in `retroarch_config.rs` passing unchanged.
- [ ] **Step 3 (RED) — launch argument contract:** Assert a normal launch's arguments remain exactly `--config <cfg> -L <core> <content>` and that a save-state launch is exactly `--config <cfg> -L <core> --entryslot <slot> <content>`, with the slot rendered as a bare decimal in `1..=999` and the content target still last.
- [ ] **Step 4:** Replace the positional `RetroArchConfig::build(paths, core_directory, controller_profiles_root)` with `RetroArchConfig::build(&RetroArchConfigRequest { paths, core_directory, controller_profiles_root, state_slot, save_state_hotkeys })`, extend `LaunchPreparation` with `entry_slot: Option<SaveStateSlot>`, and implement `retroarch_input.rs`. The application-owned list of qualified managed controller device profiles lives in `retroarch_input.rs` as a documented constant referencing the M8 hardware qualification; it is a list of *device profile filenames inside the authenticated database*, never a button table.
- [ ] **Step 5:** `state_slot` is written **in addition to** `--entryslot` deliberately. `--entryslot` is the documented mechanism and the argument contract is unchanged; the configuration value makes "the active slot after a save-state launch is the loaded state's own slot" hold deterministically from RetroFrontier's own single control path, which the generated file already is. Note this in `docs/SAVE_STATES.md` and add it to the manual qualification checklist.

### Task 6: Launch pipeline — one pipeline, two plans

**Files:** `src-tauri/src/application/launch.rs`, `src-tauri/src/services/retroarch.rs`

- [ ] **Step 1 (RED):** Assert
  - a normal launch resolves its core through the existing override-then-default policy and passes `state_slot = 1` with no `--entryslot`;
  - a save-state launch uses the **exact historical core binary** from its plan, launches the **exact recorded ContentUnit**, and passes the stored slot;
  - a save-state launch never reads and never writes `game_launch_overrides` (asserted by driving one with a *different* stored override present and checking both the resolved core and that the stored override is byte-identical afterwards);
  - a save-state launch whose plan core is not catalog-approved for the game's system, or not release-approved for it, is refused rather than falling through to the game's current core;
  - there is no "try the current core anyway" path: no code path in `launch.rs` turns a `CoreUnavailable` save-state launch into a normal launch;
  - a save-state launch is a **new managed play session** with its own durable process record and its own pre-launch baseline;
  - and every existing M7 launch-lifecycle test still passes.
- [ ] **Step 2 (RED) — baselines are pre-spawn and fail closed:** Assert the baseline is durably persisted **before** the process record is written and before `spawn` is called (a spawn-recording launcher plus a repository read prove the ordering); that a baseline failure aborts the launch *before* any spawn, closes the session as `failed_to_start`, clears the record, and returns a normalized failure; and that a baseline exceeding the entry cap aborts the launch the same way.
- [ ] **Step 3 (RED) — reconciliation is driven by the certainly observed end:** Assert a positively reaped clean exit, a non-zero exit (`crashed`), and a `failed_to_start` all trigger reconciliation; that an *uncertain* end — a failed `wait`, a failed `terminate`, a blocked record — triggers **no** reconciliation while the session stays open, and that reconciliation then happens once `watch_until_absent` proves absence and closes the session; and that `reconcile_on_startup` reconciles a persisted baseline whose session is already closed while leaving an open session's baseline alone.
- [ ] **Step 4:** Refactor `launch_locked` to take a `LaunchPlan`:

  ```rust
  enum LaunchPlan {
      Normal,
      SaveState { core: AuthenticatedCoreBinary, content_unit_id: ContentUnitId, slot: SaveStateSlot },
  }
  ```

  `LaunchPlan::Normal` keeps the current behaviour exactly. `LaunchPlan::SaveState` replaces only core resolution and content-unit selection; the runtime mutation lock, process exclusivity, content-target validation, BIOS validation, managed controller profiles, config generation, durable play-session creation, durable process record, restart adoption, and process monitoring are the *same* code. Add `pub async fn launch_save_state(&self, plan) -> LaunchResponse`. Add `LaunchErrorCode::SaveStateUnavailable` only if a genuinely new normalized launch code is needed; otherwise reuse the existing codes and keep `LaunchErrorCode::ALL` and its wire test in sync either way.
- [ ] **Step 5:** Add the `SaveStateLifecycle` collaborator:

  ```rust
  pub trait SaveStateLifecycle: Send + Sync {
      fn capture_baseline(&self, request: &BaselineRequest) -> Result<(), SaveStateError>;
      fn discard_baseline(&self, session_id: PlaySessionId);
      fn reconcile_session(&self, session_id: PlaySessionId);
  }
  ```

  `LaunchApplicationService::new` takes `Arc<dyn SaveStateLifecycle>` as a required argument — a launch with no baseline is not a launch M9 permits, so there is no permissive default. `capture_baseline` is called synchronously before the process record; `discard_baseline` on any pre-spawn abort; `reconcile_session` after each certainly observed close, in `monitor`, `watch_until_absent`, and the pre-spawn failure paths. Update the M7 test harness in `launch.rs` to construct a real `SaveStateApplicationService` over its existing temp database and a temp states root.
  The dependency cycle is broken on the *other* side: `SaveStateApplicationService` holds the launch service behind a `OnceLock` set by `attach_launch(...)`, because only `load_save_state` needs it. Construction order in `lib.rs` is save-states → launch → `attach_launch`.

### Task 7: Save-State application service

**Files:** `src-tauri/src/application/save_state.rs` (new), `src-tauri/src/application/mod.rs`

- [ ] **Step 1 (RED) — reconciliation:** Using a synthetic states tree, a real repository, and a deterministic stability probe, assert reconciliation
  1. registers a new stable `.stateN` file with complete provenance from the session's baseline (game, content unit, session, core id, component id, exact core binary digest, display version, source revision, originating release id, slot, path, digest, size);
  2. registers a **changed** file at an already-registered path;
  3. ignores an unchanged file, an unsupported file, `slot 0`, and AUTO;
  4. refuses an unstable candidate while still registering an independently proved sibling (partial independent success);
  5. is idempotent — a full replay creates no duplicate row and changes no `updated_at`;
  6. is retryable — a crash simulated after the snapshot but before persistence leaves the baseline, and the retry completes;
  7. attributes nothing at all when the session is still open;
  8. attributes nothing when the snapshot was incomplete, and performs **no** `missing` transition in that case;
  9. transitions an `available` row whose file vanished to `missing` when the snapshot was complete;
  10. marks an `available` row `superseded` and inserts a **new** row when the same physical path is proved to carry content from a **different** core binary, never rewriting the old row's `core_binary_sha256`;
  11. refreshes in place — same `SaveStateId`, same immutable core-binary provenance, new digest/size/thumbnail, bumped `updated_at` — when the same core binary overwrote its own slot;
  12. associates a thumbnail only when proved, and leaves a state valid with no thumbnail otherwise;
  13. drops the baseline after `MAX_RECONCILIATION_ATTEMPTS` indeterminate attempts, registering nothing and logging `reconciliationFailed`, so a baseline cannot leak forever;
  14. never touches `saves/`.
- [ ] **Step 2 (RED) — list and capabilities:** Assert `list_save_states`
  - returns only `available`, proven states, ordered `updated_at DESC`;
  - performs the cheap re-check (containment, no-follow regular file, size) and transitions a vanished or size-mismatched file to `missing`, excluding it from the result;
  - reports `loadability: ready` when the exact core binary is locatable and no managed session is active; `coreUnavailable` when it is not; `temporarilyBlocked` while a managed game is launching, running, or blocked;
  - reports `deletable: true` for a state whose historical core is unavailable, proving loadability and deletability are independent;
  - exposes a thumbnail reference only for a state with a registered thumbnail, as an opaque `rfmedia` reference and never a path;
  - exposes a content-unit label only when the game has more than one content unit;
  - and exposes no digest and no path in the serialized view.
- [ ] **Step 3 (RED) — controlled load:** Assert `load_save_state(id)` re-verifies, in order, that the state exists, is `available`, its file exists, its validated relative path stays inside the managed states root, it is a regular non-symlink file, its size and digest still match, the game and content unit are valid and available, the exact historical core binary is available *and* currently trusted, and that no managed session is active/pending/blocked — with a distinct focused test per failure returning the matching `SaveStateError` and **launching nothing**. Assert a digest mismatch transitions the row to `missing`, leaves the untrusted file untouched, refuses the load, and refuses a later delete through that same id. Assert the happy path delegates to `LaunchApplicationService::launch_save_state` with the exact recorded content unit, the located historical binary, and the stored slot.
- [ ] **Step 4 (RED) — load failure never damages provenance:** Assert that a `spawnFailed`, `processExitedDuringLaunch`, or `crashed` outcome after a save-state launch leaves the row `available` with an unchanged digest, unchanged provenance, and unchanged status, and that a later retry succeeds once the state is otherwise loadable. Assert no code path marks a state corrupt.
- [ ] **Step 5 (RED) — delete:** Assert `delete_save_state(id)`
  - safely deletes the exact registered file and persists `deleted`;
  - deletes safely even when the historical core is unavailable;
  - refuses on a digest mismatch, a size mismatch, a symlink, a path escape, or a non-regular file, deleting nothing;
  - refuses while a managed game is launching, running, or blocked;
  - deletes a verified thumbnail together with its state;
  - deletes the state and leaves an unverifiable thumbnail untouched, recording the thumbnail-cleanup problem, rather than sacrificing the safe state deletion;
  - and, with persistence made to fail after the physical delete, leaves the row inconsistent for exactly one cycle and converges to `missing` on the next list or reconciliation — asserted end to end.
- [ ] **Step 6:** Implement `SaveStateApplicationService` with `list_save_states`, `load_save_state`, `delete_save_state`, `reconcile_session`, `reconcile_on_startup`, `capture_baseline`, `discard_baseline`, and `attach_launch`; implement `SaveStateLifecycle` for it. Reconciliation runs on a `tokio::spawn`ed task from the lifecycle hook and is awaited directly in tests. Structured `tracing` on every outcome with `play_session_id`, `save_state_id`, `game_id`, `content_unit_id`, `core_id`, `core_component_id`, `runtime_release_id`, `slot`, and the outcome — never state bytes, never an unnecessary absolute path, never a credential.

### Task 8: IPC surface and thumbnail delivery

**Files:** `src-tauri/src/commands/save_state.rs` (new), `src-tauri/src/commands/mod.rs`, `src-tauri/src/services/media_delivery.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/domain/save_state.rs`

- [ ] **Step 1 (RED):** Assert the request types deserialize **only** `{ gameId }` and `{ saveStateId }`; that `LoadSaveStateRequest` and `DeleteSaveStateRequest` have no path, thumbnail, core, runtime, sha, slot, or coreId field, and that `serde(deny_unknown_fields)` rejects one being sent; and that every anticipated problem is a status-tagged response rather than an IPC error.
- [ ] **Step 2 (RED):** Assert `parse_save_state_thumbnail_route` accepts `/save-state-thumbnail/<positive integer>` and rejects an empty id, `0`, a negative id, a percent-encoded value, a traversal-shaped value, a query-like value, and any other route; and that `load_save_state_thumbnail` refuses a state with no registered thumbnail, a state that is not `available`, a thumbnail whose file is absent, and a thumbnail whose size or digest no longer matches, serving bytes only after full re-verification.
- [ ] **Step 3:** Add `list_save_states`, `load_save_state`, `delete_save_state` commands, register them in `lib.rs`, wire `SaveStateApplicationService` into `AppState`, extend the existing `rfmedia` protocol handler with the thumbnail route, and add `save_state_thumbnail_reference(SaveStateId)` beside `cached_cover_reference` with the same target-specific origin handling and its own origin test.
- [ ] **Step 4:** Call `SaveStateApplicationService::reconcile_on_startup()` and `sweep_delete_quarantine()` in `initialize_state`, after `LaunchApplicationService::reconcile_on_startup()`.

### Task 9: Frontend contract and hook

**Files:** `src/platform/ipc.ts`, `src/hooks/useSaveStates.ts` (new) and its test

- [ ] **Step 1 (RED):** Assert the hook loads a game's states, exposes them in backend order without re-sorting, surfaces a normalized `SaveStateError` code without parsing strings, refuses to issue a load or delete while a managed game is launching/running/blocked, reloads after a successful delete, keeps the list unchanged after a failed delete, is race-safe across a game change (a late response for a previous game is discarded), and reloads when the launch state reports a managed game ended.
- [ ] **Step 2:** Mirror `SaveStateView`, `SaveStateCapabilities`, `SaveStateLoadability`, `SaveStateErrorCode`, `SaveStateError`, `DeleteSaveStateResponse` in `ipc.ts` and add `listSaveStates`, `loadSaveState`, `deleteSaveState`. Implement `useSaveStates` following the existing `useGameDetail` channel pattern.

### Task 10: Game Detail Save States section, focus, and confirmation

**Files:** `src/focus/focusNodes.ts`, `src/features/library/SaveStatesSection.tsx` (new), `src/features/library/saveStateCopy.ts` (new), `src/features/library/GameDetailPage.tsx`, `src/app/AppShell.tsx`, `src/styles/index.css`, plus `SaveStatesSection.test.tsx` and `SaveStatesFocus.test.tsx`

- [ ] **Step 1 (RED) — presentation:** Assert the section renders only `available` states, in `updated_at DESC` order as delivered; renders a verified thumbnail and a neutral placeholder when there is none; renders `SLOT N`, the save/update time, and compact core identity; renders a content-unit/disc label only when it disambiguates; renders `READY TO LOAD`, `REQUIRED CORE UNAVAILABLE`, and `TEMPORARILY UNAVAILABLE` for the three loadability values; renders **no** copy containing "compatible" anywhere (asserted against the rendered container text); renders no SHA-shaped string; distinguishes two states that share a slot but differ in core provenance; and renders the empty state with the ingame `SELECT + R1` / `SELECT + ← / →` hotkey guidance when there are none.
- [ ] **Step 2 (RED) — focus and controller:** Assert each card's focus identity is `save-state:<SaveStateId>` and survives a reorder (identity is never array position); that `confirm` loads only when `loadability === 'ready'` and that a disabled Load invokes **no** fallback of any kind; that `context` opens the options scope containing Load and Delete; that Load can be disabled while Delete stays enabled; that `back` closes the options scope and restores focus to the originating card; that the delete-confirmation scope's initial focus is **Cancel**; that `A` confirms the focused choice and `B` cancels; that cancelling restores focus to the originating save state; that a successful delete moves focus deterministically to the state that took the removed position, else the previous state, else the Save States section heading, and never to a removed DOM node; and that no controller action reaches the section while RetroFrontier does not own application input.
- [ ] **Step 3:** Add `focusNodes.saveState`, `focusNodes.saveStateAction`, `focusNodes.saveStatesHeading`, and `focusScopes.saveStateOptions(id)` / `focusScopes.saveStateDelete(id)`. Build the section from existing primitives and tokens following B6/B7's visual intent — 16/9 thumbnail, slot badge, options control, confirmation surface with Cancel first in DOM order so `initialFocus: 'auto'` lands on it — with English copy consistent with the rest of the app. Both scopes use `restore: 'none'` and restore explicitly per user action, exactly as the launch scopes do. Wire the section into `GameDetailPage` below Local Content and pass `useSaveStates` through `AppShell`.
- [ ] **Step 4 (RED) — regression:** Assert the existing Game Detail, launch-scope, and Game Detail focus suites still pass unchanged, and that the section renders nothing that changes the M7 Play interaction.

### Task 11: Security and adversarial review

- [ ] Walk every case in the approved adversarial list and record, per case, the exact test or code path that closes it: untrusted filename; `../`; an accidentally absolute stored path; a symlinked file; a symlinked nested directory; replacement between verification and deletion; stale frontend capability data; a tampered state file; a tampered thumbnail; a stale or missing Runtime installation; the same `CoreId` with the wrong binary; a revoked Runtime/core; a RetroFrontier restart mid-session; a RetroFrontier crash mid-reconciliation; a RetroArch crash; a DB failure after a successful filesystem deletion; a legacy file shaped exactly like a valid RetroArch state; the same content basename in multiple library roots; multi-disc games; multiple core binaries using the same slot; multiple core binaries resolving to the same physical state path.
- [ ] Record accepted residual risks explicitly, with the size/mtime/inode baseline trade-off and the non-Unix fail-closed deletion stub named. Do not weaken an invariant to make a test easier.
- [ ] Run `/security-review` over the branch diff and address anything real.

### Task 12: Documentation

**Files:** `docs/SAVE_STATES.md` (new), `DOMAIN.md`, `ARCHITECTURE.md`, `BACKLOG.md`, `docs/RETROARCH_LAUNCH.md`, `docs/CONTROLLER_AND_FOCUS.md`, `docs/RUNTIME_MANAGER.md`, `docs/DEVELOPMENT.md`, `README.md`

- [ ] Write `docs/SAVE_STATES.md` covering: the opaque SaveData boundary; the provenance-based launch delta model; filesystem versus SQLite authority; exact core-binary identity; no compatibility guarantee; no current-core fallback; Runtime security floor wins; supported slots `1..=999`; AUTO and slot 0 out of scope; the controller Save/slot hotkeys and their derivation; no ingame RetroFrontier Load hotkey; the state-thumbnail proof rule; legacy/orphan states remaining untouched and invisible; restart and reconciliation behaviour; the four lifecycle states; delete safety including the quarantine-rename invariant and the non-Unix limitation; the two recorded deviations; and the manual qualification checklist for the pinned RetroArch 1.22.2 behaviour.
- [ ] Update `DOMAIN.md`: expand *Save Data* and *Save State*, add the M9 persistence paragraph, and add the domain rules for provenance-only attribution and for a `SaveStateId` never authorizing a path.
- [ ] Update `ARCHITECTURE.md`: replace the aspirational *SaveService* section with the implemented boundary, and add `states/` detail to *Application data*.
- [ ] Update `docs/RETROARCH_LAUNCH.md` (the launch plan, `--entryslot`, the new config keys, the baseline in the lifecycle ordering), `docs/CONTROLLER_AND_FOCUS.md` (the save-state focus identities, the two new scopes, the Options action and its derived physical button), `docs/RUNTIME_MANAGER.md` (Task 3 Step 4), `docs/DEVELOPMENT.md` (focused M9 test commands), `README.md` (one status sentence), and close M9 in `BACKLOG.md` with the deliberate exclusions.
- [ ] Do not copy stale B6/B7 technical assumptions — an AUTO slot, a `thumbnail_path` column, a directory watcher — into any architecture document.

### Task 13: Verification and report

- [ ] `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, `pnpm test`, `pnpm build`
- [ ] `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` and `cargo test --manifest-path src-tauri/Cargo.toml --all-features`
- [ ] `git diff --check`; compare `git status --porcelain --ignored` before and after to prove no pre-existing untracked/ignored artifact changed
- [ ] Confirm no secret, ROM, BIOS file, runtime binary, database, log, or generated artifact entered the diff
- [ ] `/code-review` at high effort over the branch, then address valid findings through the same TDD loop
- [ ] Write the structured implementation report; report every deviation explicitly
