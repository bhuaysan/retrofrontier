# M8 — Launch interaction lifecycle: final fix report

This report records one narrowly scoped pass on `feat/m8-controller-focus`, after the second
corrective pass had closed HIGH-FINAL-1 … 5. It fixes a single remaining defect:

> Transient frontend launch state was application-global and was not explicitly owned by the Game
> Detail route/game that created it.

Companion documents: [`docs/CONTROLLER_AND_FOCUS.md`](CONTROLLER_AND_FOCUS.md) (the behavioural
contract, updated by this pass), [`docs/M8_IMPLEMENTATION_REPORT.md`](M8_IMPLEMENTATION_REPORT.md),
[`docs/M8_CORRECTIVE_REPORT.md`](M8_CORRECTIVE_REPORT.md),
[`docs/M8_FINAL_CORRECTIVE_REPORT.md`](M8_FINAL_CORRECTIVE_REPORT.md), and
[ADR-014](adr/ADR-014-input-acquisition-boundary.md).

Nothing from the previous passes was rewritten, and no Rust source file was touched.

## A. Starting state

|                            |                                                                                          |
| -------------------------- | ---------------------------------------------------------------------------------------- |
| Branch                     | `feat/m8-controller-focus`                                                                |
| Starting local `HEAD`      | `d19b7a917f521fb53beac3c35f908ffd5ca6ef58`                                                |
| Starting `origin/feat/m8-controller-focus` | `d19b7a917f521fb53beac3c35f908ffd5ca6ef58` — identical                    |
| M7.5 base (`main` / `origin/main`) | `77f5194c76c360bd6eb14e8546a7a4e0998be1aa` — both, unchanged                      |
| Final `HEAD` of the lifecycle pass | `31bd77541280c5f623f78bdaa740cee562096983` — `docs(launch): finalize M8 interaction ownership record`, the third of the three commits below. Later commits on this branch are outside the pass this report describes. |
| Pushed                     | The lifecycle pass recorded by this report was initially local-only at report creation. It was subsequently pushed unchanged as the fast-forward `d19b7a917f521fb53beac3c35f908ffd5ca6ef58..31bd77541280c5f623f78bdaa740cee562096983`. This row records that historical event and does not attempt to describe the repository's future/current HEAD. |
| PR / merge                 | None. No pull request was opened; nothing was merged to `main`.                            |

Verified before any edit:

```text
branch: feat/m8-controller-focus
HEAD:                             d19b7a917f521fb53beac3c35f908ffd5ca6ef58
origin/feat/m8-controller-focus:  d19b7a917f521fb53beac3c35f908ffd5ca6ef58
main / origin/main:               77f5194c76c360bd6eb14e8546a7a4e0998be1aa
```

### Working-tree state at start

29 pre-existing untracked review artifacts (`M3_REVIEW.md` … `M6_FINAL_REVIEW_2.md` plus
`docs/M5_IMPLEMENTATION_REPORT.md`). No other modifications. All 29 are still untracked at the end.

### Commits created

```text
40bc775 fix(launch): bind transient launch state to its game
aef574e fix(focus): distinguish launch cancel from route unmount
```

plus a third commit carrying this report and the documentation updates.

## B. HIGH-DELTA-1 reproduction

All reproductions are integration tests in `src/app/AppShell.test.tsx`, describe block
`AppShell M8 launch interaction ownership`. They drive the **real** shell and the **real**
`useGameLaunch()` state path; nothing about the ownership solution is faked in the test.

The abandonment path used is the pointer click on Game Detail's **BACK TO LIBRARY** link (and, for
§ B.4, the sidebar rows). These are native navigation paths, not the semantic Cancel/Dismiss action,
and they stay available while a temporary launch scope is open precisely because M8 deliberately does
not browser-trap Tab or the pointer inside a focus scope.

Two fixture bugs were found and fixed while building the harness before any production change:
`getLibraryGameDetail` and `getGameMetadata` take a **request object**, so a mock keyed on a bare
number silently returned Game A's fixture for Game B.

### B.1 Stale content options — CONFIRMED

Test: `does not render another game’s content options after the route is abandoned`.

```text
/games/1 → PLAY → backend answers contentSelectionRequired
"Choose a version" surface opens for Game A
click BACK TO LIBRARY (pointer, no semantic CANCEL) → /library
click Game B's card → /games/2
```

Observed on `d19b7a9`: `expect(element).not.toBeInTheDocument()` — **the "Choose a version" group was
found on Game B's route**. A direct DOM dump of `/games/2` listed
`Kirby Disc 1 · SINGLE FILE · 1 FILE`, `Kirby Disc 2 · …` and `CANCEL` alongside Game B's own
controls. Confirming an option there would have called `launch(2, 101)`; Rust rejects the mismatch, so
this was never an authority failure — the frontend state and UI were simply wrong.

### B.2 Stale launch failure — CONFIRMED

Test: `does not render or focus another game’s launch failure after the route is abandoned`.

```text
/games/1 → PLAY → backend answers failed
"Launch failed" scope opens, DISMISS takes focus
click BACK TO LIBRARY → /library → open Game B
```

Observed on `d19b7a9`: `expect(element).not.toBeInTheDocument()` — **the `<aside>` "Launch failed"
surface was found on Game B**. The test also asserts what the finding predicted about focus and the
footer: no DISMISS control exists, Game B's own route-entry focus is untouched, and no stale `DISMISS`
Back hint is offered.

### B.3 Cross-game pending launch — CONFIRMED

Test: `refuses a second launch request while one is still unresolved`.

```text
/games/1 → PLAY with a launch request that never settles
click BACK TO LIBRARY → /library → open Game B
```

Observed on `d19b7a9`: `expect(element).toBeDisabled()` failed — **Game B's PLAY was enabled** while
Game A's request was still unresolved, so a second frontend `launch()` could be issued. The test then
settles Game A's request as `failed` and asserts the abandoned result does not resurrect Game A's
failure surface on Game B, and that Game B becomes launchable again once nothing is pending.

Test: `discards a content-selection answer that arrives after the route was abandoned` — same setup,
settled as `contentSelectionRequired`. Observed on `d19b7a9`: the option list appeared on Game B.

### B.4 Stale content-scope focus restoration — CONFIRMED, and independent

Test: `leaves no stale detail:play request when a content scope closes with the route`.

```text
/games/1 → content selection open → focus "Kirby Disc 1"
click sidebar "Settings"    → /settings     (pointer; issues no focus request)
click sidebar "All systems" → /library      (clears the Library return target)
open Game B                 → /games/2      (well inside the 1.2 s safety interval)
```

The sidebar route is used deliberately: neither destination issues a focus request of its own and
`navigateFromShell` clears the Library's return target, so the closing content scope's generic
restoration is the *only* thing that can create a pending request. Fake timers then advance a full
2000 ms to prove no delayed steal can occur.

This finding is **independent** of B.1–B.3, and the evidence shows it:

| Stage | Result |
| --- | --- |
| On `d19b7a9` | Focus landed on Game A's `Kirby Disc 1` button — the stale *surface* dominated |
| After `40bc775` (ownership fix only) | **Still failing**, now for the isolated reason: focus landed on Game B's `Play Second Game Local` |
| After `aef574e` | Passing; Game B's heading keeps focus across the whole interval |

The middle row is the proof that the ownership fix alone does not close it: leaving the route still
unmounts the scope, its cleanup still fires, and `requestFocus('detail:play', { resolveOnRegister })`
still becomes a pending request that Game B's PLAY then satisfies.

### B.5 Over-correction guards (passed before and after)

Two tests were written to fail if the fix went too far, and pass on `d19b7a9`:

- `keeps the owning route’s transient surface while the user stays on it` — a rerender of the same
  route must not be mistaken for leaving it.
- `adopts an authoritative running session started by an abandoned request` — see § E.

## C. Root cause

`useGameLaunch()` held `pendingGameId`, `contentOptions`, `failure`, `running`, and `blocked` as one
flat, application-global bag. Only `pendingGameId` carried a game id, and only while the request was
unresolved: a `contentSelectionRequired` answer stores the options and **clears** `pendingGameId`, and
a normalized `failure` never carried a game id at all.

`AppShell` passed that whole model to `GameDetailPage`, which rendered `contentOptions` and `failure`
unconditionally. So the question "may this route render this transient surface?" was answered by
"whichever Game Detail route is currently mounted", which is not an answer — it is an accident.

Global was the right scope for some of that state and the wrong scope for the rest, and the two had
been conflated:

- `running`, `blocked`, and "a launch request is unresolved" are facts about the **application**. A
  game keeps running while the user browses elsewhere, and the input-ownership predicate needs all
  three.
- A pending surface, an option list, and a failure message are **one screen's transient
  presentation**. They have an owner, and that owner was never written down.

The fix does not move anything into a component; it gives the transient half an explicit identity.

## D. Final launch-interaction ownership model

```ts
interface LaunchInteraction {
  gameId: number;                                     // who started it
  phase: 'pending' | 'contentSelection' | 'failure';  // what it is presenting
}
```

Presentation ownership only. It is never consulted about process state, and it holds no copy of
`running`.

```text
                    PLAY on Game A's Detail route
                              │
                              │  refused outright if any request is unresolved
                              ▼
       interaction = { gameId: A, phase: 'pending' } ; pendingGameId = A
                              │
      ┌───────────────────────┼──────────────────────────────┬─────────────────────┐
      │                       │                              │                     │
      ▼ started               ▼ contentSelectionRequired     ▼ failed              ▼ route left
 running = session       phase = 'contentSelection'     phase = 'failure'     interaction = null
 pendingGameId = null    pendingGameId = null           pendingGameId = null  options/failure = null
 interaction = null            │                              │               pendingGameId UNCHANGED
      │                        │                              │                     │
      │        ┌───────────────┴──────────┐            ┌──────┴──────┐              │
      │        ▼ confirm a version        ▼ CANCEL     ▼ DISMISS     ▼ route left    │
      │   same interaction,          interaction   interaction   interaction         │
      │   same gameId A,               = null        = null        = null            │
      │   same focus origin              │             │             │               │
      │   pendingGameId = A ─────────────┘             │             │               │
      │                                                                              │
      ▼ backend reports running -> null (authoritative, once per sessionId)          │
 one requestAppWindowFocus(); pending return becomes observable ◄────────────────────┘
      │                                                     (see § E for the response
      ▼ window focused                                       policy after abandonment)
 DOM focus restored to the captured origin, or to the current route's target
```

| Event | Interaction | `pendingGameId` | Transient UI | Notes |
| --- | --- | --- | --- | --- |
| New launch | `{ A, pending }` | `A` | pending surface on A | Refused if any request is unresolved |
| `contentSelectionRequired` | `{ A, contentSelection }` | `null` | option list on A | Still A's interaction — ownership is **not** discarded because the pending id cleared |
| Option continuation | `{ A, pending }` | `A` | pending surface on A | Same interaction, same game, **same focus origin**; the HIGH-FINAL-2 fix is untouched — the content-option node is never recaptured as the return origin |
| Cancel | `null` | `null` | cleared | Explicit focus restore to `detail:play` |
| Failure before running | `{ A, failure }` | `null` | failure scope on A | Owned by the initiating game while displayed |
| Dismiss | `null` | `null` | cleared | Explicit focus restore to `detail:play` |
| **Route abandoned** | `null` | **unchanged** | cleared | Presentation only; see § E |
| `started` | `null` | `null` | cleared | Backend session adopted; return lifecycle takes over |
| running | `null` | `null` | — | Backend authority, unchanged |
| `blocked` | unchanged | unchanged | — | Nothing is concluded while the state is uncertain |
| Process exit | — | — | — | One `setFocus()`, then DOM restore; unchanged from the previous pass |

Two things enforce ownership:

1. **The shell abandons** the interaction when the current route is no longer the owning game's. One
   effect keyed on `(interaction.gameId, currentGameId)` covers every navigation path — pointer, Tab,
   browser back, semantic back, sidebar, wordmark, mobile nav — instead of one guard per control.
2. **Game Detail receives a route-scoped view.** `contentOptions` and `failure` are masked unless the
   interaction belongs to the current game, so a screen structurally cannot render transient state it
   does not own — not even for the single render between the route change and the effect.
   `pendingGameId`, `running`, and `blocked` stay global on purpose.

## E. Abandoned pending request behaviour

An IPC request cannot be cancelled by deleting frontend state, so an abandoned request is still
allowed to resolve. What is abandoned is its **presentation**.

`pendingGameId` and `running` are deliberately untouched by abandonment. Clearing `pendingGameId`
early would re-grant application input ownership while RetroArch may already exist — precisely the
interval `ownsApplicationInput()` exists to cover.

Every response is judged on two independent questions:

- `owns()` — is this still the current request of a mounted hook? If not, the response is irrelevant
  to every piece of frontend state.
- `presents()` — does the transient interaction still belong to the game that asked? Route
  abandonment makes this false while `owns()` stays true.

| Response | `pendingGameId` | Presentation | Process state |
| --- | --- | --- | --- |
| `started` | cleared | interaction closed | **`running` adopted unconditionally** |
| `contentSelectionRequired` | cleared | discarded if not owned; painted if owned | — |
| `failed` | cleared | discarded if not owned; painted if owned | — |
| transport rejection | cleared | discarded if not owned; synthesised `internalLaunchFailure` if owned | — |
| launch-state event | cleared when `running` becomes `null` | — | **always authoritative** |

`pendingGameId` clears on **every** resolution regardless of `presents()`, because the request really
did resolve and the ownership predicate depends on that fact.

`started` is adopted regardless of `presents()`: the user did ask for that process, the backend
created it, and a route change must never make a real running process disappear. Test
`adopts an authoritative running session started by an abandoned request` asserts exactly that — after
leaving Game A and opening Game B, a late `started` response produces
`RETROARCH HAS CONTROLLER INPUT` in the footer, disables Game B's Play with
`ANOTHER GAME IS RUNNING`, and brings **no** transient Game A surface with it.

### Request-generation policy

`requestGeneration` still ignores responses from a superseded request or an unmounted hook. Its role
is now much narrower, and deliberately so:

- Because a second launch is **refused** while one is unresolved, the counter can no longer advance
  while a request is in flight. The displacement the finding described — a second request making the
  first response irrelevant — is therefore structurally impossible, not merely unlikely.
- The only legitimate advance is the content-option continuation, which happens **after** the first
  request has resolved.
- No frontend generation may contradict backend process state: the `started` branch and the
  launch-state event are never gated on presentation ownership.

This policy is written into the code comments on `launch()` as well as here.

## F. Focus behaviour

Both launch scopes now declare `restore: 'none'` and restore explicitly per user action, because the
generic `restoreTo`/`restoreFallback` cleanup cannot tell *why* the surface closed and fires for both
a user action and a route unmount.

| Event | Focus |
| --- | --- |
| Content selection **cancelled** | `detail:play`, requested **before** the surface closes while Play is still mounted and enabled, so it resolves at once; `detail:back` is the fallback |
| A **version confirmed** | `detail:back` — deliberately *not* Play, which the launch this click issues disables in the same commit. Back is the only Game Detail control that stays enabled through a pending launch. This is the same place focus previously ended up, now reached on purpose instead of through a disabled-target fallback |
| Launch failure **dismissed** | `detail:play`, then `detail:back` as the fallback (unchanged from the previous pass) |
| **Route unmount**, either scope | Nothing. No stale `detail:play` request, so nothing can steal focus from a Game Detail reopened inside the 1.2 s safety interval |

The existing pending/disabled-target behaviour is preserved:
`does not leave focus on nothing when Play is disabled by the launch it started` still asserts that
BACK TO LIBRARY holds focus after a version is confirmed, and
`cancels the selection with back and restores the Play action` still asserts Play is restored on
Cancel.

## G. Cross-game launch behaviour

**Game B cannot see Game A's transient state**, for two reasons that hold independently: the shell
abandons the interaction on route change, and Game Detail's route-scoped view masks
`contentOptions`/`failure` it does not own. The second makes the guarantee structural rather than
timing-dependent.

**Game B cannot start a second request while one is unresolved.** The invariant lives in
`useGameLaunch.launch()`, which returns without issuing when `pendingGameId !== null`. `LaunchAction`
additionally disables Play whenever *any* launch is pending and states the reason —
`ANOTHER GAME IS LAUNCHING` — rather than looking idle. Keying availability on
`pendingGameId === gameId` alone was the defect. The running-game block, the blocked-state block, and
backend authority are all unchanged.

## H. Regression verification

| Test | On `d19b7a9` | After `40bc775` | After `aef574e` |
| --- | --- | --- | --- |
| does not render another game’s content options after the route is abandoned | FAIL | pass | pass |
| does not render or focus another game’s launch failure after the route is abandoned | FAIL | pass | pass |
| refuses a second launch request while one is still unresolved | FAIL | pass | pass |
| discards a content-selection answer that arrives after the route was abandoned | FAIL | pass | pass |
| leaves no stale detail:play request when a content scope closes with the route | FAIL | **FAIL** | pass |
| restores Play when the content selection is cancelled semantically | pass | pass | pass |
| keeps the owning route’s transient surface while the user stays on it | pass | pass | pass |
| adopts an authoritative running session started by an abandoned request | pass | pass | pass |

### Prior HIGH-FINAL fixes, rerun explicitly

| Fix | Filter | Result |
| --- | --- | --- |
| HIGH-FINAL-1 already-focused exit | `restores DOM focus when the window was already focused as the process ended` | 1 passed |
| HIGH-FINAL-2 multi-content origin | `useLaunchFocusReturn launch interaction lifetime` | 6 passed |
| HIGH-FINAL-3 window focus bootstrap | `useAppWindowFocus bootstrap ordering` | 8 passed |
| HIGH-FINAL-4 RAF ownership | `useControllerInput ownership revocation ordering` | 3 passed |
| HIGH-FINAL-5 failure scope | `Game Detail launch failure scope` | 9 passed |
| Content-selection scope | `Game Detail launch content selection scope` | 4 passed |

## I. Full verification

| Command | Result |
| --- | --- |
| `pnpm typecheck` | pass |
| `pnpm lint` | pass (0 problems) |
| `pnpm format:check` | pass |
| `pnpm test` | **36 files, 570 tests, all passing** |
| `pnpm build` | pass |
| `cargo fmt -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo test` | **411 passed, 0 failed, 1 ignored** |
| `cargo test --all-features` | **431 passed, 0 failed, 7 ignored** |
| `cargo build --release` | pass |
| `git diff --check` | clean |

Frontend test count: 562 at `d19b7a9` → **570** (+8). Rust counts are unchanged, as expected: **no
`.rs` file was modified by this pass.**

### Repeated runs for timing and focus flakiness

`src/app/AppShell.test.tsx` plus `src/features/library`, `src/hooks`, `src/focus`, `src/input`,
`src/features/settings` — 4 consecutive runs, **535/535 each time**, no variation. The
`leaves no stale detail:play request …` test uses fake timers with `shouldAdvanceTime`, so its 1.2 s
assertion is deterministic rather than wall-clock dependent.

### Changed files

```text
src/app/AppShell.tsx
src/app/AppShell.test.tsx
src/hooks/useGameLaunch.ts
src/features/library/GameDetailPage.tsx
src/features/library/GameDetailPage.test.tsx
src/features/library/GameDetailFocus.test.tsx
docs/CONTROLLER_AND_FOCUS.md
docs/M8_IMPLEMENTATION_REPORT.md
docs/M8_FINAL_CORRECTIVE_REPORT.md
docs/M8_LAUNCH_LIFECYCLE_FINAL_REPORT.md
```

No `.rs` change, no capability change, no dependency change, no OS-specific focus hack, no
`xdotool`/`wmctrl`/compositor scripting. Rust remains authoritative for launch validation, content
ownership validation, runtime/core/BIOS, process state, and running-session identity.

## J. Repository hygiene

```text
$ git ls-files docs/M5_IMPLEMENTATION_REPORT.md
                                     (empty — untracked)
```

The file is present locally and untracked. All 29 historical review artifacts remain untracked; none
was staged at any point during this pass, and no broad `git add -A` was used — every commit staged
explicit paths. No ROM, BIOS, RetroArch runtime, core, AppImage, database, credential, token, private
key, log, or build artifact is tracked. `git diff --numstat` reports no binary entries.

## K. Manual qualification

The operator checklist in [`docs/M8_FINAL_CORRECTIVE_REPORT.md`](M8_FINAL_CORRECTIVE_REPORT.md) § K
is carried forward **unchanged and still NOT PERFORMED** — every interactive item there remains
`NOT PERFORMED — HUMAN INTERACTION REQUIRED`. Claude Code cannot press a physical DualSense button,
cannot observe the application window or the compositor's activation decision, and cannot read
WebKitGTK's `Gamepad.mapping`. Hardware presence is not accepted as evidence.

The environment facts recorded there were re-verified during that pass (Fedora 44, Plasma 6.7.4,
Wayland, DualSense at `/dev/input/js1`, WebKitGTK 2.52.5, libmanette 0.2.13, Tauri 2.11.5) and are
unchanged by this pass, which touched no platform boundary.

### New section 9 — Route-abandon qualification

| # | Step | Verdict |
| --- | --- | --- |
| 9.1 | Open Game A's Detail; press PLAY on a multi-content game so the version list appears | NOT PERFORMED |
| 9.2 | Leave through pointer/native navigation — click BACK TO LIBRARY, the sidebar, or the wordmark — **without** pressing CANCEL | NOT PERFORMED |
| 9.3 | Open Game B; verify **no** Game-A version list is present | NOT PERFORMED |
| 9.4 | Repeat 9.1–9.3 with a reproducible launch failure instead: verify **no** Game-A failure appears on Game B and that nothing steals focus | NOT PERFORMED |
| 9.5 | Game B's route-entry focus behaves normally (heading focused; the first directional press moves from there) | NOT PERFORMED |
| 9.6 | While Game A's launch is still starting, leave to Game B: Game B's PLAY is disabled and reads `ANOTHER GAME IS LAUNCHING`; pressing it does nothing | NOT PERFORMED |
| 9.7 | Let that launch start RetroArch anyway: the session is adopted (footer says `RETROARCH HAS CONTROLLER INPUT`), and on exit the return lifecycle behaves as in § K.5–K.7 of the previous report | NOT PERFORMED |
| 9.8 | Leave Game A with the version list open, then reopen a Game Detail within about a second: the reopened route's own entry focus is not displaced | NOT PERFORMED |
| 9.9 | Cancel the version list semantically instead: focus returns to PLAY | NOT PERFORMED |

## L. Remaining risks

1. **The interactive gate is still unperformed**, including the new § K.9 items. Everything here rests
   on jsdom integration tests through the real shell and real hook. The largest single open risk from
   the previous report is unchanged: WebKitGTK reporting `mapping === "standard"` for the DualSense is
   unverified, and without it M8 controller navigation does not work on this machine at all.
2. **Route abandonment is keyed on the game route, not on the surface.** Leaving Game A's Detail for
   the Library and coming straight back to Game A does *not* restore the abandoned version list; the
   user must press PLAY again. That is deliberate and, I think, the honest behaviour — the backend
   answer was consumed and the interaction is over — but it is a behaviour change worth a product
   glance rather than a silent one.
3. **A pending launch now disables PLAY on every Game Detail.** If a launch request were ever to hang
   without resolving, the entire application would be unable to start a game until the response
   arrived or the app was restarted. The previous behaviour hid this by allowing a second request,
   which was worse; but there is no frontend timeout, deliberately, because inventing one would mean
   guessing about process state.
4. **`abandonInteraction` is driven from an effect**, so the underlying state is cleared one commit
   after the route change. The route-scoped view is what makes the guarantee structural in the
   meantime; if that scoping were ever removed, the effect alone would leave a one-render window.
5. **The launch-return origin is not discarded on route abandonment.** It does not need to be — the
   existing `routeKey` guard means a Game-A origin can never be restored on Game B — but the origin
   does survive until the request resolves, so the mechanism protecting against contamination is the
   route-key comparison rather than the absence of the origin.
6. **`docs/M5_IMPLEMENTATION_REPORT.md` is untracked but not ignored**, unchanged from the previous
   report: a future over-broad `git add -A` can re-track it.
7. **Windows and macOS remain unqualified**, as do controller remapping (B10) and RetroArch's own
   input configuration.

## M. Verdict

The defect was reproduced in four separate forms with integration tests that failed on `d19b7a9`, the
stale focus-request defect was shown to be independent by remaining red after the ownership fix, the
root cause was fixed rather than the symptoms, the five prior HIGH-FINAL fixes were rerun explicitly,
and the automated gate is complete and green with no Rust change. The interactive Linux/DualSense gate
is still not performed and cannot be self-certified from this session.

`M8 LAUNCH LIFECYCLE FIX — READY FOR FINAL REVIEW`

Subject to the manual qualification in § K, which remains an open, operator-owned gate.
