# Save States (M9)

This document describes the implemented M9 boundary. It is implementation documentation; the ADRs
and the implemented architecture remain authoritative, and `docs/design/screens/B6`/`B7` are visual
handoff only — their `save_states(id, game_id, slot, created_at, thumbnail_path)` sketch, their AUTO
slot, and their "backend observes RetroArch's save directory" watcher are **not** what M9 does.

## Scope

Linux x86_64 only, on the managed RetroArch 1.22.2 runtime.

M9 makes RetroArch **save states** first-class RetroFrontier objects. Normal save data — SRAM,
memory cards, and every other persistent emulator/game file — is deliberately untouched by it.

## The two rules everything else serves

> RetroFrontier never derives trusted Save-State provenance from a filename. A managed Save State
> exists because a controlled launch proves its provenance and RetroFrontier subsequently verifies
> the exact physical state content.

> A `SaveStateId` never directly authorizes a path. It identifies the expected domain object; the
> backend must prove the exact current filesystem target again before acting.

## The SaveData boundary

Normal save data stays RetroFrontier-*located* and otherwise opaque. It lives under `saves/`, which
is outside every replaceable runtime version tree, so a runtime update, repair, or rollback cannot
remove it. M9 never enumerates it, never assigns it slots, never exposes individual files, never
deletes it, and never applies save-state compatibility logic to it. The save-state adapter is only
ever handed the `states/` root, so `saves/` is unreachable from it by construction.

## Boundary

```text
React (useSaveStates / SaveStatesSection on Game Detail)
  -> list_save_states / load_save_state / delete_save_state      (identities only)
  -> SaveStateApplicationService          (application/save_state.rs)
       -> SaveStateRepository             (repositories/save_state.rs)  provenance, lifecycle, baselines
       -> save_state_fs                   (services/save_state_fs.rs)   RetroArch layout, snapshots,
       |                                                                stability, hashing, no-follow
       |                                                                verification and deletion
       -> RuntimeManager                  exact authenticated core binary and its trust
       -> LibraryRepository               game and content-unit validity
       -> LaunchApplicationService        the one managed launch pipeline
rfmedia://localhost/save-state-thumbnail/<SaveStateId>  -> SaveStateThumbnailDelivery
```

`LaunchApplicationService` gained one launch *plan* parameter and one `SaveStateLifecycle`
collaborator. It did not gain a second process launcher, and `RuntimeManager` did not gain a second
trust model.

## Provenance, not discovery

A Save State becomes a managed object only through a **controlled launch delta**:

1. Before RetroArch is spawned, the state tree is enumerated and that snapshot is persisted durably
   as the session's baseline, together with the session's already-authenticated provenance.
2. RetroArch runs. RetroFrontier has no API that tells it to save; the player does, with the
   hotkeys below.
3. After the process end is **certainly observed**, the tree is enumerated again and compared
   against the baseline.
4. Only stable, new or changed, supported numbered state files may be attributed to that session.

There is no general heuristic importer, and nothing is ever attributed from a filename resemblance,
a slot suffix, a directory name, a content basename, or timestamp proximity.

### Attribution is fail-closed

A Save State is associated with a game only when RetroFrontier can prove the game, the exact content
unit, the core provenance, and the exact state file identity. Anything less is not attributed.

Pre-M9, legacy, and orphan states that cannot be proved are **left untouched on disk**, never
imported, never displayed, never offered for load, and never offered for delete. M9 adds no manual
assignment UI and no recovery or orphans screen.

## Filesystem versus SQLite authority

| | Authoritative for |
| --- | --- |
| Filesystem | physical file existence and bytes |
| SQLite | RetroFrontier provenance, lifecycle history, identity, and the *registered* file identity |

Neither alone makes a Save State usable. A row with no matching physical file is not loadable; a
file with no proved RetroFrontier provenance is not a managed Save State.

## Supported slots

Manual slots **1–999** only.

Slot 0 — RetroArch's unnumbered `<base>.state` — and the automatic slot `<base>.state.auto` are
neither imported nor managed. The domain cannot even express them: `SaveStateSlot` takes a number
and refuses everything outside the range, and there is no AUTO variant anywhere.

A normal game launch starts with active slot **1**. The previously active slot is deliberately not
persisted as a RetroFrontier preference. A save-state launch starts on the loaded state's own slot.

## Lifecycle

| Status | Meaning |
| --- | --- |
| `available` | The registered content is present and still matches. Only these appear on Game Detail. |
| `missing` | The registered physical content is gone, or no longer matches its registered identity. |
| `superseded` | A controlled session proved the same physical slot now carries content from *different* immutable core-binary provenance. |
| `deleted` | RetroFrontier itself safely deleted the registered state after explicit user confirmation. |

A closed lifecycle value is never reopened: every transition is conditioned on the row still being
`available`, so two reconciliations, or a reconciliation racing a delete, cannot overwrite each
other's verdict. A Save State's historical core-binary provenance is **never** rewritten into a
different binary — no `UPDATE` statement in the repository mentions those columns at all.

## File content identity

Every managed Save State is bound to its exact registered bytes by SHA-256, size, and validated
relative path. If the file changes outside an attributable controlled launch:

- its previous provenance is no longer trusted;
- it stops appearing as an available Save State;
- it cannot be loaded;
- it cannot be deleted through the old `SaveStateId`;
- and the new, untrusted file is left exactly as it is.

The stored digest is never silently refreshed from an unexplained change. A digest mismatch means
the *registered identity* is no longer valid — it is **not** a claim that the new bytes are corrupt,
and M9 has no vocabulary for that claim.

## Durable launch baselines

The baseline is persisted **before** the durable process record and before the spawn. ADR-011
already writes the record before `exec` so a crash cannot leave an invisible managed process; the
baseline goes one step earlier for the same kind of reason — a state written by a session whose
"before" was never recorded could never be attributed afterwards. A baseline that cannot be created
durably fails the launch with `saveStateBaselineFailed`, before anything is spawned.

Baseline entries record **size, modification time, and inode** rather than a digest. The approved
reconciliation order hashes *after* the process ended, and pre-hashing an entire state tree before
every launch would add unbounded launch latency for no provenance gain. See *Accepted residual
risks*.

This is what makes a crash mid-session recoverable:

```text
baseline persisted → RetroArch spawned → RetroFrontier exits or crashes
→ RetroArch stays alive → RetroFrontier restarts
→ the existing launch reconciliation adopts the known process and session
→ the process later exits certainly → M9 reconciles from the persisted baseline
```

If the session, the process, and the baseline cannot be connected with certainty, **no attribution
and no destructive reconciliation happen at all**.

## Reconciliation

Reconciliation is a consequence of the existing managed RetroArch lifecycle plus startup
reconciliation. There is no new global filesystem watcher.

After a certainly observed process end: load the baseline, snapshot the tree, determine the delta,
consider only supported slots, reject unsupported and ambiguous candidates, verify stability, hash,
verify any thumbnail relationship, persist provenance, mark previous states missing or superseded
where provable, and only then remove the baseline.

- **Retryable and idempotent.** The same completed session and physical identity cannot produce a
  duplicate row: a unique `(play_session_id, state_relative_path, state_sha256)` index makes a
  replay a no-op, `updated_at` included.
- **Partial independent success.** Each candidate is an independent proof, so one bad candidate does
  not discard the others.
- **A RetroArch crash is not a reason to discard the delta.** A crash is a *certain* end, so valid
  stable states written before it are still registered.
- **An uncertain or blocked process state is never attributed**, and the baseline is kept so the
  eventual retry still has its "before".
- **Absence is only actionable from a complete enumeration.** An unreadable subdirectory, a symbolic
  link anywhere in the tree, a non-UTF-8 name, or a tree beyond the entry and depth bounds all make
  the snapshot *incomplete*, and an incomplete snapshot never drives a `missing` transition.
- A baseline that stays indeterminate is dropped after bounded retries rather than leaking forever.

### Same slot, different cores

Slot numbers are scoped metadata, not identity. These coexist logically:

```text
Game / ContentUnit / core binary A / slot 1
Game / ContentUnit / core binary B / slot 1
```

When core binary B overwrites the physical path a state from core binary A occupied, the old object
becomes `superseded` and a **new** object is created with its own immutable provenance and digest.
The old object's provenance is never rewritten. When the *same* binary overwrites its own slot, the
object keeps its identity and moves onto the newly proved content — that change is *explained* by a
controlled launch, which is what distinguishes it from the unexplained change that invalidates a
registered identity.

## Core provenance and loadability

A Save State is bound to the **exact core binary** that produced it, identified by the authenticated
SHA-256 of that component's executable taken from the release manifest's installed-file inventory —
the same map `verify_tree` re-hashes the installed tree against. It is never recomputed from
whatever `.so` happens to sit at the core path: hashing an arbitrary file proves what that file is,
never that it is trusted. (The component's own `sha256` is the *archive* digest, a different value.)

A human-readable core version and upstream revision are recorded alongside it so a state stays
describable after its originating Runtime Release is gone. Neither is a load identity.

When loading:

1. The exact recorded core binary is required.
2. The original Runtime Release ID stays recorded provenance.
3. The original release is **not** required: another currently installed, authenticated, allowed
   installation carrying the identical binary satisfies the load.
4. If the exact binary is unavailable, the state is not loadable.
5. There is no silent fallback to the game's currently configured core.
6. There is no "try another core anyway" escape hatch.
7. A revoked, blocked, below-security-floor, or otherwise untrusted component is **never**
   reactivated to load a state. Save-state recovery never overrides Runtime security policy.

The historical core is a **one-shot launch override**. Loading a Save State never reads and never
writes the game's persisted per-game core preference.

### Loadability is not compatibility

M9 never claims `compatible`. It calculates only whether a controlled load attempt is currently
*permitted*:

| Value | User-facing |
| --- | --- |
| `ready` | Ready to load |
| `coreUnavailable` | Required core unavailable |
| `temporarilyBlocked` | Temporarily unavailable |

Even with the same binary, a state is not guaranteed to deserialize. One failed load never marks a
state corrupt and never alters its digest or provenance. Only concrete evidence — a missing file, a
digest mismatch, invalid trust, an unavailable required binary — changes eligibility.

`loadable` and `deletable` are independent: a state whose historical core is gone is still safely
deletable, because deleting needs the file, not the emulator. Both are **UI snapshots only**; every
invariant is re-proved when the action is actually invoked, so stale frontend capability state can
never authorize anything.

### Runtime retention

Save States do not pin Runtime Releases. The existing retention and security design is unchanged, so
routine cleanup may remove the only authenticated copy of a required core binary. When it does, the
state is preserved, stays visible while its own file is valid, and only its Load action becomes
unavailable. Vulnerable or superseded runtimes are never held open by a save state.

## Content-unit provenance

Every managed Save State is bound to the exact `game_id + content_unit_id` it was created under.
This is mandatory for multi-disc games: a Disc 1 state is never offered as a Disc 2 state, the
recorded unit is *used* rather than selected, and no cross-disc compatibility is inferred. A
save-state load therefore never offers a content choice.

## Controlled load

```text
load_save_state(save_state_id)
```

React supplies an identity and nothing else — no state path, thumbnail path, core path, runtime
path, digest, requested slot, or requested `CoreId`. `deny_unknown_fields` means such a field is
*rejected* rather than ignored.

Before loading, the backend re-proves: no managed session is active; the state exists and is
`available`; its file exists; its validated relative path stays inside the managed states root; it
is a regular non-symlink file; its size and digest still match; the game and content unit are valid
and available; and the exact historical core binary is available, authenticated, and currently
allowed.

**The active-session check comes first, and that ordering is load-bearing.** Verification marks a
mismatched state `missing`, and a running RetroArch is entitled to be mid-write on exactly that
file — the session that ends reconciles it properly. Verifying first would let a live emulator's
ordinary in-progress save turn a good Save State into `missing`.

The load then goes through the **existing managed launch pipeline**. M9 builds no second launcher.

## Save-State launch

```text
AppRun --config <RF-controlled config> -L <exact historical core> --entryslot <stored slot> <content>
```

`--entryslot NUMBER` is documented by the managed RetroArch 1.22.2 binary itself
(`-e, --entryslot=NUMBER  Slot from which to load an entry state.`). The generated configuration
*also* states `state_slot`, because that file is RetroFrontier's single control path over
RetroArch's behaviour and it is rewritten before every launch; saying it costs nothing and makes
"the active slot is the loaded state's own" deterministic rather than inferred. An ordinary launch's
arguments are byte-identical to M7's.

A save-state launch is a **new managed play session** in every respect — the same runtime mutation
lock, process exclusivity, content-target validation, BIOS validation, managed controller profiles,
configuration generation, durable play session, durable process record, restart adoption, and
process monitoring — and it receives its own pre-launch baseline, so states written during it
reconcile normally.

If RetroArch fails to load a state or crashes, the existing launch failure path is used. The Save
State is not marked corrupt, its digest and provenance are untouched, and a later retry is allowed.

## Managed configuration

| Key | Value | Why |
| --- | --- | --- |
| `savestate_directory` | `<appData>/states` | RetroFrontier-owned, outside every runtime version tree |
| `savestates_in_content_dir` | `false` | Nothing is written beside user ROMs |
| `savestate_auto_save` | `false` | Every managed state comes from a deliberate save |
| `savestate_auto_load` | `false` | Loading is a controlled RetroFrontier action |
| `savestate_thumbnail_enable` | `true` | Produces a *provable* thumbnail candidate |
| `state_slot` | `1`, or the loaded state's slot | The active slot on start |
| `sort_savestates_enable` | `true` | Unchanged from M7 — see the layout note below |

## Ingame hotkeys

RetroFrontier has no API that tells a running RetroArch to save. RetroArch stays the component that
writes the state; RetroFrontier configures the hotkeys that ask it to:

| Combination | Effect |
| --- | --- |
| Select + R1 | Save State |
| Select + D-Pad Right | Next state slot |
| Select + D-Pad Left | Previous state slot |

**There is deliberately no RetroFrontier-provided ingame Load State hotkey.** Controlled loading
happens through Game Detail, where the exact historical core binary, the exact content unit, and the
exact file identity are all re-proved first. A hotkey could prove none of that. A test asserts no
generated configuration key, under any input, binds one.

The physical values are derived from the **authenticated managed joypad-autoconfig database** — the
same immutable Runtime Release support component `joypad_autoconfig_dir` points at. There is no
universal gamepad button table to fall back on, and inventing one would bind "Save State" to
whatever that index happens to be on the player's pad. The real qualified DualSense profile declares
Select as `8`, R1 as `5`, and its D-Pad as `h0left`/`h0right`; hat notation is carried through
verbatim. No host RetroArch location is ever consulted and nothing is downloaded.

If a qualified profile is absent, is a symlink, is oversized, omits a role, declares one twice,
carries a value that is not a joypad bind, or disagrees with the other qualified profile, **no
hotkey is written at all**. RetroArch's hotkey binds are one global set, so picking one of two
disagreeing profiles would silently mis-bind the other pad. An unresolved set never fails a launch:
losing the save hotkey is a smaller failure than losing the game.

While RetroArch runs, RetroFrontier does not consume the controller. The M8 ownership boundary is
unchanged, and M9 introduces no focus path around it.

## State thumbnails

A thumbnail is associated **only** when the session's own delta proves it: the file RetroArch wrote
beside the state it just saved, stable, and verifying as a regular file inside the managed root. A
pre-existing image, a thumbnail belonging to another state, and anything under `screenshots/` can
never qualify — the adapter is only ever given the states root, so a general screenshot is
unreachable from it by construction. Temporal proximity is never used.

Thumbnail identity is stored independently of the state's: its own validated relative path, SHA-256,
and size. A valid state with no provable thumbnail stays valid, exposes no thumbnail, and the
frontend renders a neutral placeholder.

The WebView receives an opaque `rfmedia` reference keyed by `SaveStateId`, exactly as a cached cover
does, and the bytes are served only after the registered size and digest are re-proved.

**The bytes come back from the descriptor that verified them.** "Verify, then read the path again"
is two operations: the second resolves the name afresh and follows symbolic links, so a same-user
attacker could swap in a link to another file of the same length between them and have *its* bytes
served into the WebView. Reading holds the same line deletion does — one `O_NOFOLLOW` descriptor,
never re-resolved — and the read is bounded by the registered length.

## Delete safety

```text
delete_save_state(save_state_id)
```

No path or slot input from React. UI confirmation is a courtesy to the user, **not** the security
boundary. Immediately before deleting, the backend re-proves the row, its expected relative path,
managed-root containment, regular file type, absence of a symlink escape, expected size, expected
SHA-256, and that no managed session is active. Any failure fails closed.

The architectural invariant is:

> RetroFrontier deletes exactly the previously verified regular file under its owned Save-State
> root, or deletes nothing.

`canonicalize` then `remove_file` cannot provide that — it leaves a window in which the resolved
name is replaced, and the removal would then delete whatever occupies the *pathname*. So:

- the states root is walked by **directory handle** with `O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC`, so
  a symlinked final component *or* intermediate directory is `ELOOP` rather than a resolution;
- identity — type, link count, size, `(dev, ino)` — is read from the descriptor RetroFrontier itself
  opened, and the digest is streamed from it;
- `O_NONBLOCK` is a **liveness** requirement, not a detail: opening a FIFO read-only blocks until a
  writer arrives, so a same-user attacker could otherwise freeze verification or deletion by leaving
  one named pipe where a state used to be;
- a hard-linked file is refused, because its content is reachable under a name RetroFrontier does
  not own;
- deletion then **renames the verified inode to a private same-directory quarantine name**,
  re-verifies `(dev, ino, size)` *there*, and only then unlinks. A replacement racing the delete can
  only end up owning the old name, which is no longer touched; on any mismatch the original name is
  restored and nothing is deleted.

A quarantine name cannot parse as a state or a thumbnail, so a crash between the rename and the
unlink leaves an inert file that is never attributed, never listed, and never loaded.
`sweep_delete_quarantine` removes such leftovers at startup and touches nothing else.

Deletion is the irreversible primary action, so ordering is: verify → delete the state file → delete
the verified thumbnail if safe → persist `deleted`. If persistence fails afterwards, the row briefly
claims `available` while its file is gone; the next listing or reconciliation re-verifies and
converges on the physical truth. Nothing ever attempts to "roll back" by recreating guessed bytes.

A thumbnail that can no longer be safely verified is **left untouched** and the problem is logged;
safe deletion of the state is not sacrificed for thumbnail cleanup. The retained thumbnail identity
then records that RetroFrontier did not remove it.

## External changes

A previously managed state proven physically absent becomes `missing` and leaves the normal UI.
Nothing searches for a similarly named replacement. If a new file later appears at the same path, it
is **not** automatically the same Save State: it needs its own provable attribution.

## Game Detail

The Save States section lists only `available`, proved states, ordered `updated_at DESC` by the
backend — the frontend does not re-sort. Each card shows a verified thumbnail or a neutral
placeholder, `SLOT N`, the save time, compact core identity, a content-unit label when it
disambiguates, and the current load availability. Digests are never UI copy.

Focus identities are `save-state:<SaveStateId>`, stable across reordering and never derived from
array position. `A` loads only when the state is loadable and a disabled Load invokes no fallback of
any kind; `X` opens the options scope containing Load and Delete, where Load may be disabled while
Delete stays enabled; `B` goes back. The delete confirmation is a dedicated scope whose **initial
focus is Cancel**; cancelling restores focus to the originating state, and a successful delete moves
focus deterministically to the state that took the removed position, else the previous state, else
the section heading — never to a removed DOM node.

The empty state teaches the ingame workflow rather than showing an unexplained blank region, naming
Select + R1 to save and Select + ←/→ to change slot. It names no load hotkey, because there is none.

While any managed game is launching, running, or blocked, M9 performs no Save-State load or delete,
and the listing reports both `temporarilyBlocked` and `deletable: false` so the UI can say so
honestly.

## Errors

`notFound`, `unavailable`, `coreUnavailable`, `temporarilyBlocked`, `integrityMismatch`,
`unsafeFilesystemTarget`, `reconciliationFailed`, `launchFailed`, `deleteFailed`.

There is deliberately **no `corrupt`**. A load returns a status-tagged response that keeps a
Save-State verdict (`refused`) distinct from the launch pipeline's own (`launchFailed`), so the UI
never has to parse text to tell "this state is gone" from "the launch failed".

## The RetroArch 1.22.2 layout, and why it is quarantined

These are **adapter facts**, not domain invariants. They live only in
`src-tauri/src/services/save_state_fs.rs`, pinned by its `retroarch_1_22_2_contract` test module so
a future Runtime upgrade must break them deliberately.

| Slot | File |
| --- | --- |
| 0 | `<base>.state` — not managed |
| N in 1–999 | `<base>.stateN` |
| AUTO | `<base>.state.auto` — not managed |
| thumbnail | `<state path>.png` |

With `sort_savestates_enable`, RetroArch inserts the **core-reported `sysinfo->library_name`** as a
subdirectory. That name is not guaranteed to equal a RetroFrontier `CoreId`, a managed component ID,
a core filename, or a libretro filename stem — and on the real qualified runtime it does not: it
produced `states/Nestopia/`, `states/bsnes-mercury/`, and `states/dolphin-emu/` for the cores
RetroFrontier calls `nestopia`, `bsnes-mercury-balanced`, and `dolphin`. **Nothing reverse-maps a
directory to a core.** The parse result carries no core field at all; core provenance comes from the
controlled launch.

## Accepted residual risks

- **A size-, mtime- and inode-preserving external rewrite is invisible to the delta.** Baseline
  entries store cheap physical identity rather than a digest, because the approved ordering hashes
  after the process ends and pre-hashing a whole tree before every launch would add unbounded launch
  latency. This fails *closed*: such a file is simply never attributed to the session. It cannot
  cause a false attribution, only a missed one.
- **The listing's capability snapshot is size-only.** Hashing every state to render one screen would
  read the whole state tree. A same-size tamper therefore survives the listing and is caught by the
  full digest verification that every load and delete performs. The snapshot is explicitly never an
  authorization.
- **Safe deletion is Unix-only.** The no-follow, directory-handle-relative implementation is
  `#[cfg(unix)]`. On a non-Unix target the stub returns `unsafeFilesystemTarget` unconditionally, so
  Windows and macOS fail *closed* rather than weakening the invariant. Implementing it there is
  packaging work.
- **Managed save-state hotkeys cover the qualified controller path only.** RetroArch's hotkey binds
  are one global set, so exactly one device's numbers can be written per launch. A pad outside the
  qualified profiles gets no RetroFrontier hotkeys in M9 and can still save through RetroArch's own
  menu. Broader per-controller coverage is B10 work.
- **A state tree with an unreadable subdirectory blocks launching.** A baseline cannot be captured
  from a tree that cannot be honestly described, and M9 would rather refuse a launch than lose the
  player's save states. `states/` is RetroFrontier-owned and `0700`, so this is a genuinely
  anomalous condition.

## Deliberately not in M9

Normal SaveData browsing or deletion, SaveData compatibility logic, manual legacy/orphan-state
assignment, heuristic import of arbitrary states, AUTO or slot-0 support, save-state version history
or backups, cloud saves, state synchronization, unlimited old runtime retention, a "try another core
anyway" escape hatch, automatic compatibility claims, an ingame RetroFrontier Load State hotkey, a
new global filesystem watcher, direct React filesystem paths, and an independent RetroArch process
launcher.

## Qualification status

The automated suite uses synthetic content, synthetic cores, and a synthetic `AppRun`; it proves the
provenance and safety architecture, not emulation. The release roundtrip additionally proves that
the *real* shipped profile database resolves the managed save-state hotkeys.

The following remains a manual checklist on any host not yet qualified, using only content and BIOS
the tester legally owns:

1. Launch a game, save with Select + R1, and confirm a state appears on Game Detail after exit with
   the right slot, time, and core.
2. Change slot with Select + ←/→, save again, and confirm a second state appears.
3. Confirm a state thumbnail is shown, and that a state saved with thumbnails unavailable still
   appears with a neutral placeholder.
4. Load a state from Game Detail and confirm the game resumes at that state and that the active slot
   is the loaded state's own.
5. Confirm `saves/` still contains the game's normal save data, untouched, and that it is never
   listed as a save state.
6. Delete a state with confirmation and confirm both the state file and its thumbnail are gone,
   siblings are untouched, and no `.rf-delete-*` file remains.
7. Kill RetroFrontier while a game runs, restart it, close the game, and confirm the states written
   during that session are reconciled.
8. Roll the runtime back or forward so a state's core binary is no longer installed, and confirm the
   state stays visible, reports `Required core unavailable`, and is still deletable.
9. While a game runs, confirm every Save-State load and delete is refused.
10. Repeat on the distribution matrix in `docs/spikes/LINUX_RUNTIME_QUALIFICATION.md`.
