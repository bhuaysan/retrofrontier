# M8 — Controller and Focus: implementation report

## A. Repository state

|                                         |                                                                                                                                                                                        |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Starting `main` / `origin/main`         | `77f5194c76c360bd6eb14e8546a7a4e0998be1aa` (expected M7.5 squash merge; the repository had not advanced)                                                                               |
| Branch                                  | `feat/m8-controller-focus`                                                                                                                                                             |
| End of the original implementation pass | `221d2da571da831657f0e746c97516bf6f615120` (`docs(input): document M8 controller and focus architecture`) — the only six commits ever pushed                                           |
| Final HEAD                              | `a7efd6d` (`fix(input): close the M8 launch ownership gap`) plus the documentation commit that carries this revision                                                                   |
| Pushed / PR / merged                    | The six original M8 commits were pushed. **The nine corrective commits are local only:** nothing was pushed after the corrective pass, no pull request was opened, nothing was merged. |

> This report was revised after the M8 adversarial review. It describes the **corrected**
> implementation and the verification of that implementation, not the state at `221d2da`. The
> finding-by-finding disposition, including the two findings that were only partially reproduced,
> is in [`docs/M8_CORRECTIVE_REPORT.md`](M8_CORRECTIVE_REPORT.md).

### Commits created

```text
feat(input): add semantic input actions with keyboard and gamepad adapters
feat(focus): add deterministic focus registry and spatial navigation
feat(focus): gate controller input on window and launch ownership
feat(launch): allow the window focus request needed after a managed game exits
feat(ui): integrate controller navigation, focus scopes, and controller footer
docs(input): document M8 controller and focus architecture
```

### Corrective commits (local, after the adversarial review)

```text
23e6ebd fix(input): preserve native editing keyboard behaviour for Escape
8a6ab06 fix(input): harden gamepad mapping and diagonal ties
0b2eb73 fix(focus): centralise the focusability test
303e0b4 fix(focus): make focus requests settlement-safe, scope-bounded and honest
5b5ef5c fix(ui): keep controller footer hints reactive
c75482b fix(focus): make the launch return origin explicit and route-aware
0673bf5 fix(input): fail closed when desktop window focus is unknown
82b8559 test(focus): cover scope activation boundaries on the real screens
a7efd6d fix(input): close the M8 launch ownership gap
```

### Working-tree state

Clean apart from pre-existing untracked review artifacts.

The 29 pre-existing untracked artifacts (`M3_REVIEW.md` … `M6_FINAL_REVIEW_2.md`,
`docs/M5_IMPLEMENTATION_REPORT.md`) are **preserved and still untracked**. They were briefly staged
by an over-broad `git add -A` during this work; that commit was rebuilt on the local, unshared branch
so the artifacts are untracked again, and their content is byte-identical to what was there before.
No other history was rewritten.

### Files added

```text
src/input/actions.ts                       src/input/keyboardAdapter.ts
src/input/gamepadAdapter.ts                src/input/*.test.ts

src/focus/focusNodes.ts                    src/focus/focusRegistry.ts
src/focus/spatialNavigation.ts             src/focus/footerHints.ts
src/focus/focusContext.ts                  src/focus/FocusProvider.tsx
src/focus/*.test.ts(x)

src/hooks/useKeyboardInput.ts              src/hooks/useControllerInput.ts
src/hooks/useAppWindowFocus.ts             src/hooks/useLaunchFocusReturn.ts
src/hooks/*.test.tsx

src/platform/appWindow.ts
src/components/ui/ControllerFooter.tsx

added by the corrective pass:
src/input/inputOwnership.ts                src/input/inputOwnership.test.ts
src/focus/focusability.ts
src/test/geometry.ts                       (layout stub for geometry-dependent tests)
src/features/library/GameDetailFocus.test.tsx
src/features/settings/SettingsFocus.test.tsx
docs/CONTROLLER_AND_FOCUS.md
docs/adr/ADR-014-input-acquisition-boundary.md
```

### Files materially changed

```text
src/app/AppShell.tsx            provider, input hooks, ownership, route back, footer
src/features/library/LibraryPage.tsx    semantic Library return (replaces the DOM query + setTimeout)
src/features/library/GameCard.tsx       detail link registered; context reaches the selection control
src/features/library/GameDetailPage.tsx actions registered; launch content-selection scope
src/features/settings/SettingsPage.tsx  containment + back for the two confirmations
src/components/ui/PixelRow.tsx          optional focus identity for sidebar rows
src/hooks/useLibraryQuery.ts            `resultVersion` settle signal
src/styles/index.css                    controller companions for the A6 focus rules; footer hints
src-tauri/capabilities/default.json     + core:window:allow-set-focus
ARCHITECTURE.md, BACKLOG.md, docs/DEVELOPMENT.md, docs/adr/README.md
```

## B. Architecture

Full contract: [`docs/CONTROLLER_AND_FOCUS.md`](CONTROLLER_AND_FOCUS.md). Decision record:
[ADR-014](adr/ADR-014-input-acquisition-boundary.md). Summary:

**Semantic action model.** `InputAction` is
`moveUp | moveDown | moveLeft | moveRight | confirm | back | context`. Physical mappings exist in
exactly two modules — `keyboardAdapter.ts` and `GAMEPAD_BUTTON_INDEX` in `gamepadAdapter.ts`. No
component, hook, or focus module references a key name or a button index.

**Keyboard adapter.** A pure event→action function plus a window listener in the bubble phase, after
React's handlers. It declines an already-consumed event, modifier chords, `Tab`, anything inside a
text-editing control, and `Enter`/`Space` on natively activatable elements.

**Controller adapter.** A pure deterministic state machine over polled Gamepad API frames, driven by
`requestAnimationFrame`. **Standard Gamepad mapping only** — a pad reported with any other mapping is
never adopted, because its button indices and axis order are undefined; the footer says
`CONTROLLER NOT SUPPORTED` instead. Hysteresis band, dominant axis with a deterministic tie-break,
D-pad precedence, UI-paced repeat, edge-triggered activation buttons, deterministic controller
selection, and release-and-adopt on every ownership change.

**Why the browser Gamepad API.** It satisfies every M8 requirement with no new platform surface — no
per-OS device backend, no `/dev/input` permissions or udev rules, no hotplug plumbing, no extra IPC
stream — and its inability to read input while the page is not live matches the ownership model M8
must enforce anyway. A native adapter buys reach that M8 is required not to use.

**Replacement boundary.** The boundary is the module that produces `InputAction` values. Replacing
the adapter touches that module and its hook only: no focus module, no navigation code, and no
component changes. ADR-014 lists the concrete conditions that would justify the swap.

**Focus coordinator / registry.** A registry maps stable semantic identities to live DOM elements
and to a live getter for each node's action metadata. The coordinator owns dispatch, focus requests,
the scope stack, the back stack, and the input-mode attribute. Its API object has stable identity,
so a focus change never re-runs consumers' effects or re-enters a scope.

**Spatial navigation.** Pure, geometry-derived. Candidates are read from the DOM at dispatch time,
never cached. Left/right prefer the current visual row, up/down the current column, both fall back to
the nearest candidate ahead with a cross-axis penalty, edges stop rather than wrap, ties resolve by
document order. No fixed column count anywhere.

**Focus scopes.** A container ref makes a transient surface the root of candidate collection while it
is mounted and gives `back` its dismiss behaviour. Entry and exit focus are configurable per scope so
an existing, already-verified screen keeps its own behaviour.

**Ownership predicate.** One exported pure function, `ownsApplicationInput()` in
`src/input/inputOwnership.ts`, is the single authority for whether RetroFrontier may act on input:

```text
windowFocused ∧ ¬blocked ∧ running === null ∧ pendingGameId === null
```

The `pendingGameId` term is what makes the predicate honest during a launch: between the click on
Play and the backend reporting `running`, a managed process is being started and RetroFrontier must
already have let go. Keyboard dispatch, controller dispatch, and the footer all read the same value,
so the three can never disagree.

**Desktop window focus fails closed.** Inside the desktop runtime, ownership requires both a native
focus read that returned `true` _and_ a live focus subscription. If either is unavailable the answer
is "not focused", because a one-off `true` with no subscription could never become false again and
would grant ownership for the rest of the session. Outside the desktop runtime (`isDesktopRuntime()`
is false — a plain `vite dev` browser session) there is no native window to interrogate, the boundary
is not called at all, and the answer is `true` so development stays usable.

**Focusability.** One module, `src/focus/focusability.ts`, answers the two distinct questions —
_may a focus request target this element_ and _may directional movement land on it_ — and
`focusMoved()` proves after the fact that focus really arrived, so a restoration can never report a
success the DOM did not perform.

## C. Controller behaviour

| Physical              | Action                                           |
| --------------------- | ------------------------------------------------ |
| Button 0 / 1 / 2      | `confirm` / `back` / `context`                   |
| Buttons 12–15         | `moveUp` / `moveDown` / `moveLeft` / `moveRight` |
| Axes 0/1 (left stick) | directional                                      |

| Policy                     | Value    |
| -------------------------- | -------- |
| Enter deadzone             | `0.55`   |
| Exit deadzone (hysteresis) | `0.35`   |
| Initial repeat delay       | `400 ms` |
| Repeat interval            | `110 ms` |
| Axis dominance margin      | `0.15`   |

- **Hysteresis:** the exit threshold sits well below the enter threshold, so jitter inside the band
  neither re-triggers nor releases a direction.
- **Dominant axis, with a deterministic tie-break:** only the larger of `|x|`/`|y|` produces a
  direction. A perfect diagonal is _not_ dropped — dropping it makes a real 45° push do nothing at
  all — it resolves to the horizontal axis, which is the documented priority. Once a direction is
  held, switching axes requires the other axis to exceed it by `axisDominanceMargin`, so a stick
  wandering across the diagonal cannot produce an alternating storm while a genuine axis change still
  gets through. A pressed D-pad always beats the stick.
- **Mapping:** a snapshot is only usable when `connected ∧ mapping === 'standard'`. Selection filters
  to supported pads, and a step against an unsupported snapshot returns released state rather than
  guessing at indices.
- **Repeat:** one action on press, then the delay, then a bounded interval, and at most one
  directional action per polled frame. Direction change and release both reset repeat state.
- **Activation buttons:** edge-triggered; never once per frame while held.
- **Disconnect / replacement / ownership change:** held and repeat state is dropped and the next
  observation adopts what is physically held without emitting, so nothing replays. Adoption happens
  at the instant ownership changes, so a real press immediately afterwards is still delivered.
- **Multiple controllers:** the active pad keeps ownership while connected **and still Standard
  mapped**; otherwise the lowest connected _supported_ index wins. Only one pad is ever read, so no
  duplicate actions are possible.
- **Unsupported pad attached:** no pad is adopted, `data-controller="unsupported"` is set, and the
  footer states `CONTROLLER NOT SUPPORTED` rather than leaving the user to guess why nothing moves.

## D. Focus behaviour

- **Library.** Cards, the sidebar system filters, the sidebar menu, the filter bar, pagination, and
  the header controls are all reachable. A card's focus target remains the existing native detail
  link; `context` reaches the separate B1 selection button, which is not collapsed into the link.
- **Game Detail.** Back to Library, Play, Favorite, the metadata action, Forget provider choice,
  metadata candidates, launch content options, and cancel/dismiss actions all carry identities and
  honest labels. No action was invented. Play keeps its identity while disabled — it is the return
  target — but stops offering `confirm`.
- **Settings.** The existing removal and metadata-account behaviour is preserved exactly:
  confirmation receives focus, cancel returns to the trigger, a removed trigger falls back to the
  roots heading, the folder picker returns focus to its trigger, and form fields edit normally. M8
  adds containment and `back`-to-cancel around those without rewriting the screen.
- **Temporary scopes.** The launch content-selection surface enters focus deterministically, contains
  movement, cancels on `back`, and restores Play (falling back to Back to Library). Both Settings
  confirmations contain movement and cancel on `back`.
- **A scope bounds activation, not only movement.** While a scope is active, `confirm` and `context`
  are refused unless the active element is inside the scope container, and the footer offers no
  action hints for a node outside it. A scope is a modal surface: it would be incoherent for movement
  to be trapped while `confirm` could still fire the control behind the dialog — which is exactly
  what happens when the user clicks outside, or when the browser leaves focus on `document.body`.
  `back` is deliberately still accepted, because dismissing the scope is the way out.
- **Scope restoration is deferred one microtask.** A scope's cleanup runs while React is still
  committing the surrounding update, so the DOM it would restore into is mid-flight. The restoration
  is queued instead, and it stands down if a focus request was made in the meantime or if focus has
  already landed somewhere real. The contract is unchanged — the initiating target is restored — only
  its timing.
- **Route restoration.** Opening a game records its `GameId`; returning issues a focus request for
  that identity with the Library heading as fallback, resolved only after `useLibraryQuery` reports a
  newly committed result. A card still rendered from the previous result cannot take a focus it is
  about to lose.
- **Fallback.** A game that disappeared, or that no longer belongs to the current search/filter/page,
  falls back to the Library heading. A detached node is never focused, and a resolved request never
  fires again.
- **A settle-gated request that times out takes the fallback, not the target.** A request that waits
  for a settle signal carries a safety timeout. Firing at the target when the timeout expires is the
  one case where the target is _known_ to be unverified — the signal that would have confirmed it
  never arrived — so the expiry path skips the target entirely and resolves to the fallback. Without
  this, the timeout restored focus to precisely the stale card the settle gate exists to exclude.
- **A focus request only succeeds if the DOM agrees.** Every request goes through `focusMoved()`. A
  registered-but-unfocusable target (disabled, `inert`, `aria-hidden`, detached) does not consume the
  request: the fallback is taken immediately, and a `resolveOnRegister` request that fails on
  registration stays alive for the fallback instead of silently ending on the body.

## E. RetroArch handoff

- **Before launch.** Nothing changes. The M7 contract is untouched: React calls
  `launch_game(gameId, contentUnitId?)` and supplies no runtime, core, BIOS, or content path.
- **From the moment the launch is requested.** Ownership is released as soon as a launch is pending,
  not when `running` finally arrives. The gap between the two is real — the backend is starting a
  process during it — and RetroFrontier must not act on input it is about to hand over.
- **While a game runs, or while launch state is blocked.** The controller dispatcher stops delivering
  semantic actions and releases its held state. The poll loop keeps running so the footer can still
  report whether a controller is attached, and the footer says `RETROARCH HAS CONTROLLER INPUT`
  instead of claiming any action. RetroFrontier does not raise its window, does not request focus,
  and uses no `xdotool`, `wmctrl`, or compositor scripting. Keyboard behaviour follows OS window
  focus.
- **On process completion.** `requestAppWindowFocus()` is called exactly once per ended session,
  through the Tauri window API. There is no retry and no repeated foreground stealing. While
  `blocked` is true the last known session is held rather than consumed, so the return still happens
  once the backend can describe the state honestly again.
- **DOM restoration.** Only after the application window actually reports focus. The origin is
  captured **synchronously at the call site that issues the launch** — `AppShell` wraps the launch
  model so `captureLaunchOrigin()` runs immediately before `launch()`. That is the moment of the
  user's intent and the only moment at which the origin is unambiguous; sampling whichever node
  happens to be focused when `running` later arrives can record something the user never launched
  from, because the launch surface has typically closed and moved focus by then.
- **The origin is route-scoped.** It records the route it belonged to, and it is only restored while
  that route is still on screen. A user who navigated elsewhere during the run is not dragged back:
  the current route's own deterministic target is used instead (Play on Game Detail, the Settings
  heading, the Library heading), and no obsolete request is left pending to steal focus later. The
  route is not reset.
- **Permissions.** `core:default` already covers `core:window:allow-is-focused`; it does **not**
  cover `allow-set-focus` in the installed Tauri (`tauri 2.11.5`, `@tauri-apps/api ^2.11.1`) — this
  was verified against `src-tauri/gen/schemas/acl-manifests.json`, not assumed. Exactly one
  permission was added.
- **Linux/Wayland findings.** See section H — the interactive part of this was not performed.

## F. Controller footer

Hints are derived from the focus model, not hard-coded per page: the focused node's declared
`confirm` and `context` labels plus the active scope's or route's `back` label. `deriveFooterHints`
is a pure function over those three optional labels and emits one hint per supported action in a
stable order.

- A node that declares no `context` action shows no `X` hint.
- An unregistered but natively activatable control shows a generic `CONFIRM`, which is true of it.
- The Library root shows no `back` hint, because there is nothing to go back to; Game Detail and
  Settings show `B LIBRARY`.
- While RetroFrontier does not own input, no action hint is shown at all.
- An attached pad the engine cannot map to the Standard layout shows `CONTROLLER NOT SUPPORTED`.

**Hints are reactive.** The footer re-derives when the focused identity changes _and_ when the
focused node's own declared actions change underneath it — a card that becomes selected, a Play
button that becomes disabled, a scope that pushes or pops its `back` label. A focus identity that
stays the same while its meaning changes was previously enough to leave the footer advertising an
action the control no longer offers; a revision counter published through context now makes that
change observable.

The existing shell status (`LOCAL LIBRARY` and the scan state) is unchanged; controller connection
state replaces the static note when a controller is attached.

## G. Automated verification

All commands run at the final HEAD, after the corrective pass. Counts are from those runs, not from
the original implementation pass.

| Command                                                    | Result                                          |
| ---------------------------------------------------------- | ----------------------------------------------- |
| `pnpm typecheck`                                           | pass, 0 errors                                  |
| `pnpm lint`                                                | pass, 0 errors, 0 warnings                      |
| `pnpm format:check`                                        | pass, all matched files use Prettier style      |
| `pnpm test`                                                | pass — **36 test files, 529 tests, 0 failures** |
| `pnpm build`                                               | pass, `built in 189ms`                          |
| `cargo fmt -- --check`                                     | pass                                            |
| `cargo clippy --all-targets -- -D warnings`                | pass, 0 warnings                                |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass, 0 warnings                                |
| `cargo test`                                               | pass — **411 passed, 0 failed, 1 ignored**      |
| `cargo test --all-features`                                | pass — **431 passed, 0 failed, 7 ignored**      |
| `cargo build --release`                                    | pass                                            |
| `git diff --check`                                         | clean, exit 0                                   |

Frontend test count before M8: 449. At the end of the original implementation pass: 469 across 35
files. After the corrective pass: **529 across 36 files** — 60 tests added, none removed to make an
implementation pass. The M8-specific suites:

| Suite                                                                 | Tests |
| --------------------------------------------------------------------- | ----- |
| `src/app/AppShell.test.tsx` (M8 block within 83)                      | 19    |
| `src/input/gamepadAdapter.test.ts`                                    | 30    |
| `src/focus/FocusProvider.test.tsx`                                    | 28    |
| `src/input/keyboardAdapter.test.ts`                                   | 12    |
| `src/focus/spatialNavigation.test.ts`                                 | 11    |
| `src/features/library/GameDetailFocus.test.tsx`                       | 9     |
| `src/features/settings/SettingsFocus.test.tsx`                        | 9     |
| `src/hooks/useAppWindowFocus.test.tsx`                                | 8     |
| `src/hooks/useKeyboardInput.test.tsx`                                 | 8     |
| `src/hooks/useLaunchFocusReturn.test.tsx`                             | 8     |
| `src/hooks/useControllerInput.test.tsx`                               | 7     |
| `src/input/inputOwnership.test.ts`                                    | 6     |
| `src/focus/footerHints.test.ts`                                       | 3     |
| `src/styles/applicationShell.test.ts` (focus-language block within 7) | 2     |

The focused M8 suites were run three times in isolation at the final HEAD (21 files, 303 tests each
run, identical results) to check for order dependence and timing flakiness. None was observed.

Two existing tests were replaced rather than weakened, each because it encoded a contract that was
wrong; both replacements are stricter, and the reasons are recorded in the test files and in section
B of [`docs/M8_CORRECTIVE_REPORT.md`](M8_CORRECTIVE_REPORT.md):

- `keyboardAdapter.test.ts` — `'still allows back out of a text-editing control'` asserted that
  `Escape` inside a field produced a page-level `back`. It is now
  `'leaves Escape inside a text-editing control to the platform'` across seven element types, plus a
  test that an ordinary non-editing target still produces `back`.
- `gamepadAdapter.test.ts` — `'emits nothing for a perfectly ambiguous diagonal'` asserted that a
  real 45° stick push produced no movement at all. It is now
  `'breaks a perfect diagonal deterministically towards the documented axis priority'`, joined by
  held-axis tie, crossover-storm, and genuine-axis-switch cases.

One existing test, `'restores the initiating target when the scope closes'`, became `async`. Its
assertion is unchanged; only the restoration's timing moved by one microtask, and the file records
why.

Coverage against the required list:

| Required                                                                   | Where                                                                                                             |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| keyboard → semantic action                                                 | `keyboardAdapter.test.ts`, `useKeyboardInput.test.tsx`                                                            |
| text-editing controls not hijacked, `Escape` included                      | `keyboardAdapter.test.ts`, AppShell search-typing, AppShell Library-search and Settings-credential `Escape` tests |
| D-pad mapping                                                              | `gamepadAdapter.test.ts`                                                                                          |
| Standard-mapping-only acceptance, unsupported pad surfaced                 | `gamepadAdapter.test.ts`, `useControllerInput.test.tsx`                                                           |
| deadzone / hysteresis / dominant axis / diagonal tie-break                 | `gamepadAdapter.test.ts`                                                                                          |
| first action, repeat delay, repeat interval                                | `gamepadAdapter.test.ts`                                                                                          |
| release reset, direction-change reset                                      | `gamepadAdapter.test.ts`                                                                                          |
| confirm/back/context edge detection                                        | `gamepadAdapter.test.ts`                                                                                          |
| disconnect reset, focus-loss reset                                         | `gamepadAdapter.test.ts`, `useControllerInput.test.tsx`                                                           |
| deterministic controller selection                                         | `gamepadAdapter.test.ts`, `useControllerInput.test.tsx`                                                           |
| responsive grid geometry, left/right, up/down                              | `spatialNavigation.test.ts`, `FocusProvider.test.tsx`, AppShell                                                   |
| irregular final row                                                        | `spatialNavigation.test.ts`                                                                                       |
| disabled/missing nodes, deterministic edges                                | `spatialNavigation.test.ts`                                                                                       |
| no stale node selection                                                    | live candidate collection; `spatialNavigation.test.ts` withheld-candidate case                                    |
| settle timeout never focuses the stale target                              | `FocusProvider.test.tsx` (`awaitSettle safety timeout`)                                                           |
| a request never reports success the DOM did not perform                    | `FocusProvider.test.tsx` (`target focusability`)                                                                  |
| Library → Detail → Library restores the `GameId`                           | AppShell                                                                                                          |
| disappeared game falls back safely                                         | AppShell                                                                                                          |
| route transition does not repeatedly steal focus                           | `FocusProvider.test.tsx`, `useLaunchFocusReturn.test.tsx`, AppShell                                               |
| pointer focus updates logical focus                                        | `FocusProvider.test.tsx`, AppShell                                                                                |
| Tab remains usable                                                         | `useKeyboardInput.test.tsx`, AppShell                                                                             |
| scope entry / containment / back / restore / fallback                      | `FocusProvider.test.tsx`, `GameDetailFocus.test.tsx`, `SettingsFocus.test.tsx`                                    |
| scope bounds activation, not only movement                                 | `FocusProvider.test.tsx`, `GameDetailFocus.test.tsx`, `SettingsFocus.test.tsx`                                    |
| dispatch disabled while pending / running / blocked / window unfocused     | `inputOwnership.test.ts`, AppShell                                                                                |
| desktop window focus fails closed; browser dev stays usable                | `useAppWindowFocus.test.tsx`                                                                                      |
| held state cleared on ownership change, nothing replayed                   | `gamepadAdapter.test.ts`, `useControllerInput.test.tsx`, AppShell                                                 |
| window focus requested once; DOM focus after; no repeats                   | `useLaunchFocusReturn.test.tsx`, AppShell                                                                         |
| launch origin captured at intent; route-aware return                       | `useLaunchFocusReturn.test.tsx`                                                                                   |
| footer hints update; unsupported actions absent; hints react to node state | `footerHints.test.ts`, AppShell                                                                                   |

## H. Manual qualification

The environment was re-verified during the corrective pass rather than assumed to have survived
from the implementation pass; the values below are from that re-check.

**Environment recorded**

|                |                                                                                         |
| -------------- | --------------------------------------------------------------------------------------- |
| Distribution   | Fedora 44 (`Fedora release 44 (Forty Four)`)                                            |
| Kernel         | `7.1.9-200.fc44.x86_64`                                                                 |
| Desktop        | KDE Plasma 6 (`XDG_CURRENT_DESKTOP=KDE`)                                                |
| Display server | Wayland (`XDG_SESSION_TYPE=wayland`, `WAYLAND_DISPLAY=wayland-0`, `DISPLAY=:0` present) |
| Controller     | Sony DualSense Wireless Controller, present as `/dev/input/js1` and `…-event-joystick`  |
| WebKitGTK      | `webkit2gtk-4.1` 2.52.5                                                                 |
| Tauri          | crate `2.11.5`, `@tauri-apps/api ^2.11.1`                                               |

**Performed**

| Item                                                         | Result                                                                                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Release binary builds and starts with the amended capability | **PASS.** Rebuilt from the corrected sources; `./src-tauri/target/release/retrofrontier` ran for 25 s and was killed by the timeout (exit 124). Log: startup, `managed runtime reconciled state=Ready`, storage initialized, metadata worker started. No permission denial, no capability error, no WebView failure.                        |
| Controller hardware and session present                      | **PASS (presence only).** Fedora 44 / KDE / Wayland session live; DualSense enumerated at `/dev/input/js1`; WebKitGTK 2.52.5 linked against libmanette 0.2.13, which is the component that would expose the pad to the Gamepad API. This establishes only that the qualification is runnable — no controller behaviour is inferred from it. |

**NOT PERFORMED** — every item below requires a person at the screen pressing controller buttons and
observing the result. This session could start the application but could not deliver physical input
to it or observe its window, so no result is inferred for any of them.

| Item                                                                   | Result        |
| ---------------------------------------------------------------------- | ------------- |
| Controller connect/disconnect in the running app                       | NOT PERFORMED |
| D-pad navigation in the Library                                        | NOT PERFORMED |
| Analogue-stick navigation                                              | NOT PERFORMED |
| Repeated movement / held direction feel                                | NOT PERFORMED |
| Responsive grid navigation at several widths                           | NOT PERFORMED |
| Sidebar navigation                                                     | NOT PERFORMED |
| Open Game Detail with the controller                                   | NOT PERFORMED |
| Back restores the same Library game                                    | NOT PERFORMED |
| Mouse changes focus naturally                                          | NOT PERFORMED |
| Tab still works                                                        | NOT PERFORMED |
| Search input editable                                                  | NOT PERFORMED |
| Settings forms editable                                                | NOT PERFORMED |
| Game Detail action navigation, Play, content selection, cancel         | NOT PERFORMED |
| Start a legally available game through the M7 path                     | NOT PERFORMED |
| RetroFrontier stops consuming controller UI input while RetroArch runs | NOT PERFORMED |
| Emulator receives controller ownership                                 | NOT PERFORMED |
| Exit RetroArch; RetroFrontier returns to foreground                    | NOT PERFORMED |
| DOM focus restored to the Play/detail context                          | NOT PERFORMED |
| Focus restored once, not repeatedly                                    | NOT PERFORMED |
| Focus visuals under `data-input-mode="controller"`                     | NOT PERFORMED |
| WebKitGTK reports the DualSense with `mapping === 'standard'`          | NOT PERFORMED |
| `CONTROLLER NOT SUPPORTED` appears for a non-Standard pad              | NOT PERFORMED |

The hardware and session are present, so this qualification is runnable by a human on this machine
without further setup, using `pnpm tauri:dev` or the built release binary. Until it is run, controller
behaviour in the real WebView — including whether WebKitGTK 2.52.5 exposes the DualSense to the
Gamepad API at all, and with which `mapping` string — is **unverified**.

The Gamepad API deliberately reveals nothing until the page receives a real user gesture on the pad,
so this specific question cannot be answered by any automated or headless means: it needs a person to
press a button. It is the single most consequential open item, because a non-Standard mapping would
make the DualSense unusable for navigation on the qualification target. The failure is at least
honest and visible rather than silent — the footer states `CONTROLLER NOT SUPPORTED` and the keyboard
remains fully sufficient — but it would be a qualification failure, not a cosmetic one.

## I. Accessibility and regression

- **Native semantics preserved.** `GameCard`'s detail target is still the same `<a href>`; the
  separate B1 selection button is still a sibling above it, and `context` reaches it rather than
  collapsing the two. Every registered node is an existing native control; nothing was converted to a
  `div` with a handler, and no `role` or `aria-*` attribute was changed.
- **Tab.** The keyboard adapter never handles `Tab` or `Shift+Tab`, and no `tabindex` was added to
  anything that did not already have one. Focus registration is a ref, not a tab-order change.
- **Text editing.** Movement, `confirm`, and `context` are suppressed inside `input` (except
  button-like types), `textarea`, `select`, `contenteditable`, and `role="textbox"`. The library
  search field, the filter selects, and the metadata account username/password fields all keep native
  behaviour.
- **Native activation.** `Enter`/`Space` on a native control produces no semantic `confirm`, so
  nothing is activated twice. Controller `confirm` synthesizes a click only when the node declares no
  handler of its own.
- **Pointer.** Unchanged. `focusin` makes pointer focus the logical focus, so a click followed by a
  D-pad press continues from the clicked control.
- **Escape / back.** `Escape` inside a text-editing control is left entirely to the platform: the
  Library search field, the filter selects, and the Settings credential fields keep their native
  behaviour, and the Library — which has no semantic `back` at all — no longer navigates from a
  keypress inside its own search box. Outside an editing control `Escape` is `back`. The window
  listener still ignores events whose default was already prevented, so the existing element-level
  `Escape` cancellations in Settings act exactly once, and a scope that wants `Escape` from inside a
  field handles it itself. `back` is only offered where a real dismiss or route-back exists.
- **Focus visuals.** A6 V5 is unchanged in appearance. The only addition is a companion selector so
  the same declarations apply to controller-driven focus, which `:focus-visible` cannot observe. A
  test asserts every `:focus-visible` selector has a companion and that no companion adds an outline
  or a new token. Light/dark theming and the responsive layout are untouched.

## J. Security and process regression

- **M7/M7.5 authority preserved.** No Rust source file was changed. React still calls one semantic
  `launch_game` command, supplies no runtime/core/BIOS/content path, never inspects OS processes, and
  never infers an exit from a timer. `useGameLaunch` remains the only launch state, and it follows the
  backend-owned `running`/`blocked` values. `RuntimeManager`, `LaunchApplicationService`, the launch
  mutex, the durable process record, and `startup_reconcile` are untouched.
- **Launch/runtime regression tests.** `cargo test` 411 passed / `cargo test --all-features` 431
  passed, including the launch suites (`simultaneous_launch_requests_start_at_most_one_game`,
  `a_restart_with_a_live_child_keeps_the_session_running_and_mutation_blocked`,
  `a_restart_with_no_surviving_process_interrupts_open_sessions`,
  `a_valid_override_is_used_and_an_invalid_one_never_falls_back`). The M7 frontend launch tests in
  `AppShell.test.tsx` and `GameDetailPage.test.tsx` pass unchanged.
- **Capability scope.** Exactly one permission added: `core:window:allow-set-focus`. Verified against
  the installed Tauri's generated ACL manifest that it is not part of `core:default` and that
  `allow-is-focused` already is. No plugin, filesystem, shell, or process permission was added.
- **No shell utilities.** No `xdotool`, `wmctrl`, compositor scripting, or `Command`/child-process use
  was introduced anywhere.
- **Repository audit.** The branch diff against `77f5194` is 48 tracked files: 23 `.ts`, 16 `.tsx`,
  1 `.css`, 1 `.json`, 7 `.md`. No binary blobs (`git diff --numstat` reports no `-` entries),
  no ROMs, BIOS files, RetroArch binaries, cores, AppImages, generated runtime trees, databases,
  credentials, signing material, logs, or build output. A scan of the diff for key material, hard-coded
  credentials, and content extensions found nothing. `git diff --check` is clean. The 29 pre-existing
  untracked review artifacts remain untracked and unmodified.

## K. Deferred work

- **Manual Linux qualification.** Section H. Runnable on this machine; not run here. Whether the
  WebView exposes the DualSense with the Standard mapping is the top open risk.
- **Windows.** Controller enumeration, Standard Gamepad mapping fidelity, and `setFocus()` activation
  semantics are unverified.
- **macOS.** Same, plus WKWebView Gamepad API behaviour and activation policy.
- **Controller remapping (B10).** Explicitly out of scope: no mapping is persisted, and no remapping
  UI was added.
- **M9 / M10.** Save states and packaging are untouched.
- **On-screen keyboard (B2).** Controller navigation can reach the search field, but there is no
  controller-operable keyboard for it; typing requires a real keyboard.
- **TV mode (D5), collections (D2).** Untouched.

### Non-M8 issues observed and deliberately not fixed

- `logs/retroarch/` is still never populated, and `docs/RETROARCH_LAUNCH.md` still claims RetroArch's
  log goes there. Carried over from the M7.5 report; unchanged here.
- `startup_reconcile` still reports `Broken` while a managed game is alive. Carried over from the
  M7.5 report. M8 exposed no correctness bug that required touching it, so it was left alone.
- Whether RetroArch takes keyboard focus when its window maps under KWin/Wayland remains
  uninstrumented — M7.5 flagged this as the first thing M8 should measure, and it needs the manual
  qualification in section H.

## L. Final verdict

**M8 CONTROLLER AND FOCUS — READY FOR REVIEW**

Every required behaviour is implemented and covered by automated tests, all ten review findings have
been dispositioned with evidence (see [`docs/M8_CORRECTIVE_REPORT.md`](M8_CORRECTIVE_REPORT.md)), and
all required automated checks pass. The manual Linux controller and RetroArch qualification in section H is **NOT
PERFORMED** and is the outstanding gate before this can be called qualified on Linux; Windows and
macOS remain unqualified as V1 targets.
