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
  -> list_save_states / load_save_state / delete_save_state      (identities only, plus the
                                                                    confirmed active-controller id
                                                                    on load — see Ingame hotkeys)
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
- **A baseline can only prove anything while its session is the last thing that touched the tree.**
  A retained baseline is retried at the next startup, by which time another session may have written
  its own states — absent from this baseline too, so the delta could no longer say whose they are.
  Attributing them would register another game's state under this session's provenance and supersede
  the row that legitimately owns it. There is no way to disambiguate after the fact, so a baseline a
  later session has written past is **dropped without attributing or removing anything**.
- **Partial independent success.** Each candidate is an independent proof, so one bad candidate does
  not discard the others.
- **A RetroArch crash is not a reason to discard the delta.** A crash is a *certain* end, so valid
  stable states written before it are still registered.
- **An uncertain or blocked process state is never attributed**, and the baseline is kept so the
  eventual retry still has its "before".
- **Absence is only actionable from a complete enumeration.** An unreadable subdirectory, a symbolic
  link anywhere in the tree, a non-UTF-8 name, or a tree beyond the entry and depth bounds all make
  the snapshot *incomplete*, and an incomplete snapshot never drives a `missing` transition. The
  snapshot walk itself is descriptor-relative — every directory is opened with
  `O_DIRECTORY | O_NOFOLLOW` and walked by the resulting handle, never by re-resolving a pathname a
  second time — so a symlink swapped into the tree between listing and reading it cannot redirect
  what the snapshot observes.
- **A baseline that stays indeterminate is retained indefinitely**, retried at every subsequent
  startup, until it either reconciles or a later session's baseline supersedes it (above). There is
  no attempt-count cutoff: a baseline is never discarded merely because reconciliation has failed
  some number of times, since doing so would destructively give up on save states that later
  resolve themselves once the underlying condition (a slow write, a momentarily unreadable
  subdirectory) clears.
- **Only this session's own content basename is ever attributed.** A candidate whose filename
  basename does not match the exact content unit the baseline recorded is left unattributed and
  untouched, however valid-looking its slot suffix is — see *Same path, different games* below.

### Same path, different games

RetroArch's state path is `<library name>/<content basename>.stateN`, so two different library games
whose ROMs share a basename — the same dump added from two content roots, or two files both called
`Tetris.nes` — collide on one path under one core. Updating in place therefore requires the same
**game and content unit** as well as the same binary; otherwise it supersedes and inserts, exactly
as a different binary does. Matching on the binary alone would move the first game's row onto the
second game's bytes while keeping the first game's ids, so its detail page would list a state that
is really the other game's and loading it would boot the wrong ROM.

Attribution goes one step further: a delta candidate is only ever attributed when it **is the exact
state target** a controlled launch of this session's core, content, and slot resolves — the whole
path, not a shape. That path is computable because RetroFrontier owns both halves of RetroArch's
own derivation (see *The RetroArch 1.22.2 layout* below): `savestate_directory` is
`<appData>/states/<CoreId>`, and `sort_savestates_enable` is off so RetroArch inserts nothing of its
own. A perfectly valid-looking managed slot that lands in the same delta window under a different
basename, a different slot, or a *different directory* is left unattributed and untouched; nothing
about timing or slot shape alone is ever enough.

The same equality is re-proved twice more before a process can exist. `prepare_load` re-derives it
from the state's recorded core and content as a fast-fail, and `launch_locked` re-derives it once
more from the core that actually resolved under the runtime mutation lock — the only core whose
`CoreId` really reaches the generated configuration — and refuses to spawn anything unless the
result equals the physical path whose bytes were verified. A row established some other way than
ordinary attribution (a direct database write, a future migration, a bug elsewhere) therefore cannot
be loaded as if it belonged to content, a core, or a namespace it does not.

This is what makes "verify file A, load file B" impossible rather than merely unlikely.
RetroFrontier hands RetroArch a *slot*, never a path, so the only defence against RetroArch
resolving a different file is that the path RetroArch will resolve is fully determined and equal to
the verified one. A basename-only proof was not that: `ForeignNamespace/Tetris.state1` and
`Nestopia/Tetris.state1` share a basename and a slot, and only one of them is ever opened.

### Same slot, different cores

Slot numbers are scoped metadata, not identity. These coexist logically:

```text
Game / ContentUnit / core binary A / slot 1
Game / ContentUnit / core binary B / slot 1
```

When core binary B overwrites the physical path a state from core binary A occupied, the old object
becomes `superseded` and a **new** object is created with its own immutable provenance and digest.
The old object's provenance is never rewritten. When the *same game, content unit, and binary*
overwrites its own slot, the object keeps its identity and moves onto the newly proved content — that change is *explained* by a
controlled launch, which is what distinguishes it from the unexplained change that invalidates a
registered identity.

## Core provenance and loadability

A Save State is bound to the **exact core binary** that produced it, identified by the authenticated
SHA-256 of that component's executable taken from the release manifest's installed-file inventory —
the same map `verify_tree` re-hashes the installed tree against. It is never recomputed from
whatever `.so` happens to sit at the core path: hashing an arbitrary file proves what that file is,
never that it is trusted. (The component's own `sha256` is the *archive* digest, a different value.)

That inventory is the authenticated one either way. ADR-012's detached representation lets the
inventory live in a separate immutable target the manifest binds by length and SHA-256, and the
resolved entries reach this projection through the same `VerifiedRuntimeManifest` boundary as an
inline manifest's. Where the entries were published therefore has no effect on core provenance: the
digest is still authenticated, still decisive, and a matching `.so` outside any authenticated
inventory still never becomes trusted. See `docs/RUNTIME_MANAGER.md`.

A human-readable core version and upstream revision are recorded alongside it so a state stays
describable after its originating Runtime Release is gone. Neither is a load identity.

When loading:

1. The exact recorded core binary is required.
2. The release that happened to *supply* that historical binary is **not** recorded provenance and
   is not required at load time: any currently installed, authenticated, allowed installation
   carrying the identical binary satisfies the load. `originating_runtime_release_id` instead
   records the runtime release that actually **launches** this session — the same value an ordinary
   launch would record — never the historical release the core binary happened to come from. The
   two can differ (a retained older release still carrying a core a newer release also ships), and
   conflating them would misreport which release a session actually ran under. It also **moves with
   the bytes**: when the same core binary overwrites its own slot, the refresh records the session
   *and* the Runtime Release that produced the new physical version. The core identity — core id,
   component, binary digest, display version, source revision — stays immutable, because that is
   precisely what makes the change a refresh rather than a supersession; the producing runtime is
   not immutable, because a save-state load runs an old core binary on the current managed
   RetroArch. Leaving it behind produced a row that contradicted itself.
3. If the exact binary is unavailable, the state is not loadable.
4. There is no silent fallback to the game's currently configured core.
5. There is no "try another core anyway" escape hatch.
6. A revoked, blocked, below-security-floor, or otherwise untrusted component is **never**
   reactivated to load a state. Save-state recovery never overrides Runtime security policy.

The historical core is a **one-shot launch override**. Loading a Save State never reads and never
writes the game's persisted per-game core preference.

**Authorization timing.** `prepare_load` performs a cheap, early lookup of the historical core
purely so an obviously-doomed load fails fast rather than going all the way through the launch
pipeline — but that lookup is explicitly **not** the authorization, and nothing about its result is
carried forward. The decisive lookup is redone from scratch — trust state re-read fresh from disk,
never reused from what the early check observed — inside `launch_locked`, under the same runtime
mutation lock ADR-011 uses to serialize launch against activation. A revocation or a security-floor
change recorded between the early check and that lock therefore still refuses the load: a historical
core cannot be revoked in the window between "looks loadable" and "is actually authorized to spawn."

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
load_save_state(save_state_id, active_gamepad_id?)
```

React supplies an identity, and optionally the frontend's own confirmed active-controller identity
(see *Ingame hotkeys* below) — nothing else. No state path, thumbnail path, core path, runtime
path, digest, requested slot, or requested `CoreId`. `deny_unknown_fields` means such a field is
*rejected* rather than ignored.

Before loading, the backend re-proves: no managed session is active; the state exists and is
`available`; its file exists; its validated relative path stays inside the managed states root; it
is a regular non-symlink file; its size and digest still match; the game and content unit are valid
and available; the state's own registered path belongs to that exact content unit's basename (see
*Same path, different games*); and the exact historical core binary is available, authenticated, and
currently allowed (subject to the re-authorization inside the runtime mutation lock described above).

**The active-session check comes first, and that ordering is load-bearing.** Verification marks a
mismatched state `missing`, and a running RetroArch is entitled to be mid-write on exactly that
file — the session that ends reconciles it properly. Verifying first would let a live emulator's
ordinary in-progress save turn a good Save State into `missing`.

A load and a concurrent Save-State delete are mutually exclusive: both enter the very same
in-process exclusion `LaunchApplicationService` uses to serialize an ordinary launch against a
delete, so a load can never race a delete that is mid-decision about destroying the file the load
would need. Whichever side enters that section first excludes the other for its entire
authorization-to-action window; the loser is refused immediately rather than interleaved.

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
| `savestate_directory` | `<appData>/states/<CoreId>` | RetroFrontier-owned, outside every runtime version tree, and the launching core's own namespace |
| `savestates_in_content_dir` | `false` | Nothing is written beside user ROMs |
| `savestate_auto_save` | `false` | Every managed state comes from a deliberate save |
| `savestate_auto_load` | `false` | Loading is a controlled RetroFrontier action |
| `savestate_thumbnail_enable` | `true` | Produces a *provable* thumbnail candidate |
| `state_slot` | `1`, or the loaded state's slot | The active slot on start |
| `sort_savestates_enable` | `false` | **Security setting.** RetroArch must insert nothing of its own — see the layout note below |

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

**The qualified profile files existing and agreeing is not, by itself, proof they apply.** They are
part of the immutable managed database and are present and agree regardless of what is actually
connected, so deriving hotkeys from that agreement alone would silently bind "Save State" to
DualSense button numbers on a launch whose actual pad is something else entirely. Resolution
additionally requires the frontend's own confirmed identity of the controller RetroFrontier
currently accepts — the `Gamepad.id` (via the browser Gamepad API; ADR-014) of the pad that actually
*owns* RetroFrontier input, read at the moment a launch or a save-state load is issued and passed as
`active_gamepad_id`. RetroFrontier's native code never reads a controller device directly; this
frontend-confirmed identity is the only proof it ever has.

**That identity comes from one ownership decision, never a second selection.**
`useControllerInput` keeps a persistent ownership index, so a pad that already owns input keeps it
when another is plugged in at a lower index; it publishes that exact pad through
`src/input/activeController.ts`, and `activeControllerIdentity()` only reads it. Selecting again at
launch time would discard retained ownership and re-pick the lowest connected index — with an Xbox
pad owning input at index 1 and a DualSense appearing later at index 0, the UI would be driven by
one pad while the backend received the other's identity and wrote its raw button numbers.

**Qualification is an exact match against what the authenticated database itself declares** — the
qualified profiles' own `input_device` and `input_device_alt<N>` values, compared trimmed and
ASCII-case-insensitively. It is deliberately not a token test: `"Generic DualSense-style Adapter"`,
`"MyDualSenseClone"`, and any unmeasured variant all carry the `dualsense` substring, and accepting
them would bind the qualified pad's raw button numbers to a device nobody has measured. Those
declarations are read as text and never pass the joypad-bind filter, so they can never reach the
generated configuration.

If `active_gamepad_id` does not exactly name a declared qualified device, or a qualified profile is
absent, is a symlink, is oversized, omits a role, declares one twice, carries a value that is not a
joypad bind, declares no device at all, or disagrees with the other qualified profile, **no hotkey
is written at all**. RetroArch's hotkey binds are one global set, so picking one of two disagreeing
profiles would silently mis-bind the other pad. An unresolved set never fails a launch: losing the
save hotkey is a smaller failure than losing the game. The qualified set is exactly what the shipped
database names — the USB DualSense, its own declared Bluetooth alias, and the DualSense Edge the
database also carries — and nothing wider (see *Qualification status*).

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
frontend renders a neutral placeholder. This holds on every refresh, not only on first creation:
when the same core binary overwrites its own slot and *this* session's delta does not itself prove a
thumbnail for the new bytes, the exposed thumbnail becomes `None` rather than continuing to show the
*previous* version's proved image. A stale image would misrepresent content it was never proved to
belong to; losing the thumbnail is the honest outcome, not a bug to route around.

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
SHA-256, and that no managed session is active. Any failure fails closed. A delete also enters the
same in-process exclusion a launch does (see *Controlled load* above), so it cannot interleave with
a concurrent launch attempt either: whichever side enters first holds the section for its whole
authorization-to-action window.

The architectural invariant is:

> RetroFrontier deletes exactly the previously verified regular file under its owned Save-State
> root, or deletes nothing — against pathname replacement, symbolic-link traversal, hard links, a
> wrong inode, a wrong digest, and ordinary TOCTOU substitution.

Stated precisely, because the qualifier is load-bearing: those are the actors this design defeats.
It does **not** claim to defeat a hostile *same-user* writer that already holds an open writable
descriptor onto the exact inode — that remains a documented POSIX limitation, narrowed rather than
closed, and it is spelled out under *Accepted residual risks*.

`canonicalize` then `remove_file` cannot provide even the stated invariant — it leaves a window in
which the resolved name is replaced, and the removal would then delete whatever occupies the
*pathname*. So:

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
  re-verifies `(dev, ino, size)` there, re-hashes its content one final time, and only then unlinks.
  A replacement racing the delete can only end up owning the old name, which is no longer touched;
  on any mismatch the original name is restored and nothing is deleted.

A quarantine name cannot parse as a state or a thumbnail, so a crash between the rename and the
unlink leaves an inert file that is never attributed, never listed, and never loaded. But a bare
quarantine-shaped name is not, by itself, proof RetroFrontier is the one that put it there: before
the rename, a small durable **ownership-proof journal entry** is written first, under a random
128-bit id. `sweep_delete_quarantine` at startup only ever finishes or removes a quarantine-shaped
file whose journal entry it can find *and* which satisfies that recorded proof in full — a file that
merely happens to carry the same name pattern, planted or coincidental, is left alone rather than
assumed to be RetroFrontier's own and swept regardless.

**The record names a physical object, not bytes.** Its shape is
`rfdj1:<device>:<inode>:<size>:<sha256>` — an explicit version marker, a fixed field count, strict
parsing, a bounded length, and no value that is ever used as a path. Recording only size and digest
would describe *content*, and content is reproducible by anyone; the entry has to survive process
restarts, so it must identify the object rather than describe what is inside it. Otherwise a stale
entry left at a quarantine name by a refused race is satisfied by whatever later occupies that name
with matching bytes, and a subsequent startup deletes a file RetroFrontier never quarantined. The
journaled device and inode are therefore a **requirement** at every stage, never something learned
from whatever currently sits at a pathname, and a record that does not parse is treated as no proof
at all rather than as a weaker one:

> A journal entry created for physical file A never authorizes the deletion of physical file B, even
> when B has the same name, the same size, and byte-identical content.

**The startup sweep destroys nothing by pathname either.** Verifying a descriptor does not license
unlinking a *name*, and Linux has no unlink-by-descriptor: a racing same-user process could rename
the verified object away and drop an unrelated file at that name in between. So the sweep repeats
the delete path's own discipline. After verifying the object it moves that directory entry to a
**fresh** RetroFrontier-owned second-stage quarantine name with `RENAME_NOREPLACE`, journaled before
the move, and re-proves the journaled `(dev, ino, size)` and digest *at the new name* before
unlinking. A pathname substituted in that window is carried to the second stage instead, fails the
re-proof, is renamed back to where it was found, and is never unlinked.

**Evidence is kept until it would become stale authority.** The ownership chain is:

```text
J1 names object A at the first-stage name Q1
  J2 is written, naming the same physical A, before anything moves
  Q1 → Q2  (NOREPLACE)
  Q2 is re-proved against the journaled identity
  every record naming A — J1, J2, and any redundant record an earlier
  interrupted generation left — is retired together, durably
                                  ← the terminal transition
  Q2 is unlinked
```

Up to the re-proof there is deliberately no step at which the live object has no durable record, so a
crash anywhere in that region leaves the next startup either the first stage or the second, each with
its own proof, and never neither. Every non-terminal outcome — a transient I/O failure, an identity
or content mismatch, an indeterminate verification, a refused race, a second-stage transfer that
cannot complete — keeps the entry that still names a real object, because discarding it would strand
that object permanently: no later startup could prove it safe to finish. A mismatching quarantine
file is never deleted, the entry is never rewritten onto whatever now occupies the pathname, and
repeating the sweep keeps refusing rather than forgetting.

**The last step reverses that priority, deliberately.** A record must not outlive the physical
object it authenticates: `(device, inode)` identifies an object only while the object exists, and
once its last link is unlinked the inode number becomes eligible for reuse — so a record that
survived its own object would be a capability that some future, unrelated file could satisfy. The
authorizing records are therefore retired **before** the unlink, not after it, and if they cannot
all be retired the unlink is not attempted at all.

**The rule is about the object, not about one stage's record.** `J1` names the same physical object
as `J2` for as long as the second stage is being proved, so retiring only the current stage's record
is not enough: a `J1` whose removal failed would outlive the inode it authenticates while `J2`
retired cleanly and the object was destroyed. The terminal condition is therefore journal-wide:

> Before the final link of a quarantined physical object is removed, no durable delete-journal
> record may remain anywhere that authorizes that same physical object identity.

Equivalently, immediately before the final `unlinkat`, the count of valid journal records whose
`(device, inode, size, sha256)` equals the object's own must be zero. That is enforced by
enumerating the journal directory descriptor-relatively, parsing every bounded regular entry
strictly, removing every record equal to the object's ownership, committing the removals, and then
re-reading the journal to confirm none is left. Duplicate and redundant records from earlier
interrupted generations are handled by that identity match alone, with no predecessor chain to
track. A malformed entry matches nothing, is no proof of anything, and is never rewritten or
repaired. The rule, stated as recovery behaviour:

```text
before the final re-proof:
  the journal is retained, so interrupted work stays recoverable

after the final re-proof:
  every record authorizing the object is retired first, then the object is unlinked

if any of that cannot be completed and proven:
  nothing is unlinked and the object is kept

if the unlink then fails or the process dies in between:
  the quarantine file is left inert and is never swept automatically again
```

| Crash point | State | Meaning |
| --- | --- | --- |
| Before the final re-proof | `Q2` + `J1` + `J2` | Recoverable; the next startup retries |
| After the re-proof, before retirement | `Q2` + `J1` + `J2` | Recoverable; the next startup retries |
| During retirement, partial or unprovable | `Q2` + whatever records remain | Nothing is unlinked; the object is kept |
| After retirement, before the unlink | `Q2` only | Inert orphan; never swept again |
| After the unlink | Neither | Done |

RetroFrontier would rather leak one tiny owned orphan than keep a record that could later authorize
a different physical object. That orphan is **not** an unresolved bug — it is the fail-closed outcome
the ordering exists to produce. Its ownership is never reconstructed from the file's name, size,
digest, currently observed inode, or any database row, because doing so would rebuild exactly the
stale authority being removed. It stays inert in every other sense too: a quarantine name cannot
parse as a state or a thumbnail, so nothing attributes, lists, or loads it.

A partial retirement lands in the same family of outcomes rather than a new one: the object is kept,
whatever is left of its evidence is left exactly as it is, and nothing is manufactured to replace
what has gone. If the record naming the object's current name survived, a later startup retries; if
it was among those already removed, the object becomes an inert orphan of the kind above.

**Durability, stated precisely.** Retirement unlinks the entries and then `fsync`s the journal
*directory*, which is what POSIX offers for committing a directory-entry removal, so on a filesystem
and storage stack that honour `fsync` the retirement is durable before the object's own unlink is
even attempted. RetroFrontier claims no more than that: on a stack that ignores `fsync` or reorders
across it, the guarantee degrades to process-crash ordering — which this ordering is structurally
correct for regardless, and which is the window that mattered.

A retirement failure says only that retirement could not be *proven* complete. It does not claim the
records are still present — a removal that was issued before the commit failed may well have taken
effect — so the guarantee that matters is the other half: the object was not unlinked, and no record
is ever recreated for it.

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
honestly. The listing also **skips its file re-check entirely** in that state, for the same reason
the mutations check for an active session first: a running RetroArch is entitled to be mid-write on
a registered state, and concluding `missing` from a half-written file would cost that state its
identity and its history.

## Errors

`notFound`, `unavailable`, `coreUnavailable`, `temporarilyBlocked`, `integrityMismatch`,
`unsafeFilesystemTarget`, `indeterminate`, `reconciliationFailed`, `launchFailed`, `deleteFailed`.

`indeterminate` is deliberately distinct from `unsafeFilesystemTarget`. The latter is a *proof* —
the file is gone, the target is not the managed regular file it must be, or (HIGH-2) the target's
own basename does not belong to the exact content unit the row claims. The former means the
observation itself failed: the process is out of descriptors, a read errored, the tree was
momentarily unreadable. **Only a proof may close a lifecycle**, because `missing` is never
reopened; an inconclusive observation leaves the row exactly as it is.

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

RetroArch composes the state base from `savestate_directory` and the content file's own basename,
then appends the slot number. What sits between those two depends on one setting:

- with `sort_savestates_enable`, RetroArch inserts the **core-reported `sysinfo->library_name`** as
  a subdirectory. That name is not guaranteed to equal a RetroFrontier `CoreId`, a managed component
  ID, a core filename, or a libretro filename stem — and on the real qualified runtime it does not:
  it produced `states/Nestopia/`, `states/bsnes-mercury/`, and `states/dolphin-emu/` for the cores
  RetroFrontier calls `nestopia`, `bsnes-mercury-balanced`, and `dolphin`;
- **without it, RetroArch inserts nothing**, and the base is exactly
  `<savestate_directory>/<content basename>.state`.

RetroFrontier therefore turns sorting **off** and points `savestate_directory` at
`<appData>/states/<CoreId>` — a segment it *writes* rather than reads back. Both halves of the
derivation are then RetroFrontier's own, so `save_state_fs::state_target` can compute the one path a
controlled launch will resolve for a given (core, content, slot), and attribution and authorization
are equality tests against it. Per-core separation is not lost; it is just defined by RetroFrontier
rather than by the core.

**Nothing reverse-maps a directory to a core.** The parse result still carries no core field, and a
directory that merely resembles a `CoreId` proves nothing — only equality with the computed target
does. Core provenance still comes from the controlled launch.

`CoreId` is already restricted to ASCII alphanumerics, `-`, `_`, and `.` with an alphanumeric first
character, so the namespace segment is always exactly one safe path component: it can never be `.`
or `..`, never escape the states root, and never collide with the `.rf-delete-*` quarantine names or
the `.rf-delete-journal` directory.

This composition is pinned against a real RetroArch 1.22.x binary rather than assumed. With
`sort_savestates_enable = false`, `savestate_directory = D` and content `Synthetic Probe.nes`, it
logs `Redirecting save state to "D/Synthetic Probe.state"`, and `--entryslot 3` then resolves
`D/Synthetic Probe.state3`. The same run with sorting enabled resolves
`D/Nestopia/Synthetic Probe.state3` instead.

`sort_savefiles_enable` is deliberately left on. SaveData is opaque to M9, lives under its own
`savefile_directory`, and no RetroFrontier proof depends on a path into it.

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
- **A restore that cannot reclaim its own name leaves the file quarantined.** If something takes the
  original name in the window after the quarantine rename, the restore refuses to overwrite it —
  destroying a file RetroFrontier never verified would break the same invariant the quarantine
  exists to keep. The verified file then stays under its inert quarantine name, with its ownership-
  proof journal entry still recording the physical object RetroFrontier itself verified, until the
  next startup sweep re-proves and removes it. That retained entry names one specific inode, so it
  can never be satisfied by anything else that comes to occupy the pathname.
- **A failure or crash after the final records are retired leaves an inert quarantine orphan.** This
  is the deliberate fail-closed outcome of retiring the authorizing records before the unlink, not a
  defect: the alternative is a record that outlives its object and could later authorize a reused
  inode. The orphan is one bounded file under a name that cannot parse as a state or a thumbnail, it
  is never attributed, listed, or loaded, and it is never automatically deleted or re-adopted. A
  retirement that could not be completed or proven has the same shape: the object stays, whatever
  records remain are left untouched, and none is ever recreated.
- **A same-inode concurrent writer narrows, rather than fully closes, the deletion window.** POSIX
  and Linux offer no dependable, portable, non-mandatory-locking way to exclude a writer that already
  holds an open, writable descriptor to the exact same inode RetroFrontier is deleting — advisory
  locks bind only cooperating processes, and there is no cross-platform mandatory-locking primitive
  to reach for instead. Re-hashing the quarantined file's content immediately before the final
  `unlinkat` narrows the exploitable window from "the whole delete" to the instant between that
  re-hash and the unlink itself — it does not close it. A same-user hostile process capable of
  winning that instant-wide race already has unrestricted filesystem access regardless, so this is
  documented as a narrowed limitation rather than a claimed guarantee.
- **Safe deletion is Unix-only.** The no-follow, directory-handle-relative implementation is
  `#[cfg(unix)]`. On a non-Unix target the stub returns `unsafeFilesystemTarget` unconditionally, so
  Windows and macOS fail *closed* rather than weakening the invariant. Implementing it there is
  packaging work.
- **Managed save-state hotkeys cover the qualified controller path only, and only when it is the
  confirmed active controller.** RetroArch's hotkey binds are one global set, so exactly one
  device's numbers can be written per launch. A pad outside the qualified profiles — or a qualified
  pad that is not the frontend's own confirmed active controller for that launch — gets no
  RetroFrontier hotkeys in M9 and can still save through RetroArch's own menu. Broader
  per-controller coverage is B10 work.
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
10. Confirm states land under `states/<CoreId>/` — the RetroFrontier-composed namespace — and not
    under a core-reported `library_name` directory. A pre-existing `states/Nestopia/` tree from
    before this binding is expected to be ignored rather than adopted: it is not the target any
    controlled launch resolves, so it is neither attributed nor loaded, and it is never deleted
    either.
11. With two pads attached, make the higher-index one take navigation ownership first, then attach
    the qualified DualSense at a lower index, and confirm the save hotkeys are *not* silently bound
    to the DualSense while the other pad drives the UI.
12. Repeat on the distribution matrix in `docs/spikes/LINUX_RUNTIME_QUALIFICATION.md`.
