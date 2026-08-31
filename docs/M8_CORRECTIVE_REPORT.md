# M8 — Controller and Focus: corrective pass report

This report records the adversarial corrective pass performed on `feat/m8-controller-focus` after
the M8 review. It is written to be re-checked by a second independent reviewer: every claim below
names the code, the regression test, and the observed result that supports it.

Companion documents: [`docs/CONTROLLER_AND_FOCUS.md`](CONTROLLER_AND_FOCUS.md) (the behavioural
contract), [`docs/M8_IMPLEMENTATION_REPORT.md`](M8_IMPLEMENTATION_REPORT.md) (the implementation,
revised to describe the corrected state), and
[ADR-014](adr/ADR-014-input-acquisition-boundary.md) (the acquisition boundary and its narrowed
support contract).

## A. Starting state

|                          |                                                                                                                 |
| ------------------------ | --------------------------------------------------------------------------------------------------------------- |
| Branch                   | `feat/m8-controller-focus`                                                                                      |
| Starting HEAD            | `221d2da571da831657f0e746c97516bf6f615120` — `docs(input): document M8 controller and focus architecture`       |
| Base for the branch diff | `77f5194c76c360bd6eb14e8546a7a4e0998be1aa` (M7.5)                                                               |
| Final HEAD               | `a7efd6d` — `fix(input): close the M8 launch ownership gap`, plus the documentation commit carrying this report |
| Pushed                   | The six original M8 commits were already pushed. **No corrective commit was pushed.**                           |
| PR / merge               | None. No pull request was opened; nothing was merged to `main`.                                                 |
| Working tree at start    | Clean apart from 29 pre-existing untracked review artifacts, which were left untouched.                         |

The starting state was verified, not assumed: `git rev-parse HEAD` returned `221d2da…` before any
change, and the full branch diff `77f5194..221d2da` was read before the first finding was
investigated.

Nine corrective commits were created, each scoped to one root cause:

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

Two files were added: `src/input/inputOwnership.ts` and `src/focus/focusability.ts`. No M8 module was
rewritten, no architectural decision from the implementation pass was reversed, and no Rust source
file was touched.

## B. Review findings — disposition

Each finding was investigated by reading the code first, then by writing a test that expresses the
correct contract and running it against the starting HEAD. A finding is only marked CONFIRMED where
that test actually failed on `221d2da`.

---

### HIGH-1 — Input ownership is not revoked during `launching`

**Status: CONFIRMED.**

**Root cause.** Ownership was computed inline in `AppShell.tsx` as
`windowFocused && !launch.blocked && launch.running === null`. `useGameLaunch` exposes a third state
the expression ignored: `pendingGameId`, set from the moment `launch_game` is invoked until the
backend answers. In that window a managed process is genuinely being started, `running` is still
`null`, and RetroFrontier therefore still dispatched controller and keyboard actions — including a
second `confirm`, which the launch mutex would reject but which the UI had no business sending.

**Regression tests.**

- `src/input/inputOwnership.test.ts` — six tests over the predicate itself, including
  `'gives up ownership as soon as a launch request becomes pending'` and
  `'regains ownership once a launch request resolved without starting a process'`.
- `src/app/AppShell.test.tsx` — `'stops consuming controller input while a launch request is still
pending'`, `'does not replay a direction held across the launch transition'`, and
  `'lets the controller act on content selection once the launch request resolved'`.

Verifying the intended failure exposed two tests that passed for the wrong reason and had to be
restructured before they were trustworthy:

- The Escape tests initially passed on the old adapter because the desktop window-focus gate had not
  yet resolved when the assertion ran, so nothing was dispatched either way. An
  `inputOwnershipSettled()` helper was added, after which they failed on the old code as intended.
- `'does not replay a direction held across the launch transition'` initially passed on the old
  predicate because the test refocused Play after settling. It was restructured to assert _during_
  the pending window and again after ownership returns, without refocusing; it then failed on the old
  predicate.

**Fix.** `src/input/inputOwnership.ts` — one exported pure function that is now the single authority:

```ts
export function ownsApplicationInput(state: ApplicationInputOwnership): boolean {
  return (
    state.windowFocused && !state.blocked && state.running === null && state.pendingGameId === null
  );
}
```

`AppShell` calls it and passes the single result to the keyboard hook, the controller hook, and the
footer, so the three can no longer disagree. Root cause, not symptom: the defect was that the
predicate was an inline expression nobody owned, so the fix was to give it an owner.

**Result.** All eleven tests pass. The held-direction case also confirms nothing replays when
ownership returns.

---

### HIGH-2 — `awaitSettle` resolves the stale target when the safety timeout fires

**Status: CONFIRMED.**

**Root cause.** `resolvePending()` in `FocusProvider.tsx` was a single path used both by the settle
signal and by the safety timeout. On timeout it attempted the requested target first and only then
the fallback. But the timeout firing is precisely the case in which the target is _known_ to be
unverified — the settle signal that would have confirmed the surface committed never arrived — so the
timeout path restored focus onto exactly the stale card the settle gate exists to exclude.

**Regression tests.** `src/focus/FocusProvider.test.tsx`, describe `FocusProvider awaitSettle safety
timeout`: `'never focuses a stale target when the safety timeout fires before the surface settles'`
and `'still focuses the target when the surface settles with it present'`. The first uses fake timers
and a spy on `HTMLElement.prototype.focus`, asserting not merely where focus ended up but that the
stale card's `focus()` was **never called** and that the only element focused was the Library
heading.

**Fix.** `resolvePending(expired: boolean)`. The settle path (`settleFocusRequest`) passes `false`
and behaves as before; the timer passes `true` and skips the target entirely, going straight to the
fallback.

**Result.** Both tests pass; the first fails on the starting HEAD.

---

### HIGH-3 — A global `Escape` handler hijacks editing controls

**Status: CONFIRMED.**

**Root cause.** `keyboardAdapter.ts` suppressed arrows, `confirm`, and `context` inside text-editing
targets, but `Escape` was mapped to `back` unconditionally. An earlier test,
`'still allows back out of a text-editing control'`, had encoded that as deliberate. It is wrong:
`Escape` in a search field is the field's own key, and the Library has no semantic `back` at all, so
the effect on the Library was a keypress inside the search box triggering page-level navigation.

**Regression tests.**

- `src/input/keyboardAdapter.test.ts` — `'leaves Escape inside a text-editing control to the
platform'` across seven element types, plus `'still produces back from an ordinary non-editing
focus target'` so the fix cannot over-reach.
- `src/app/AppShell.test.tsx` — `'leaves Escape in the Library search to the platform instead of
navigating'`, `'does not navigate away when Escape is pressed in the Settings credential fields'`,
  and `'still produces the route back from an ordinary focused control'`.
- `src/features/settings/SettingsFocus.test.tsx` — `'keeps Escape inside the Settings credential
fields out of page navigation'`.

**Fix.** `case 'Escape': return editing ? null : { action: 'back', preventDefault: true };` A scope
that wants `Escape` from inside a field still handles it at the element and consumes the event, which
the adapter already honours through `defaultPrevented`.

**Existing test replaced, not weakened.** `'still allows back out of a text-editing control'` was
deleted because it asserted the defect. Its replacement is strictly stronger — seven element types
instead of one — and the test file records why the old contract was wrong.

---

### HIGH-4 — The launch return origin is captured too late and is not route-aware

**Status: CONFIRMED.**

**Root cause.** `useLaunchFocusReturn` sampled `document.activeElement`'s focus identity in the effect
that observed `running` becoming non-null. That is not the moment of the user's intent: by then the
launch content-selection scope has typically closed and moved focus, so the recorded "origin" could
be a node the user never launched from. Separately, the recorded origin carried no route identity, so
a user who navigated to the Library while the game ran was dragged back to the Game Detail action on
return, and an obsolete request could remain pending to steal focus later.

**Regression tests.** `src/hooks/useLaunchFocusReturn.test.tsx` (8 tests), specifically
`'records the origin when the UI initiates the launch, not when running arrives'`,
`'does not drag the user back to the route the launch started from'`,
`'leaves no obsolete request that steals focus when the old route returns'`, and
`'falls back within the current route when nothing recorded the launch origin'`. The harness renders
two genuinely different routes, so a route change really changes which nodes exist.

**Fix.** The hook now returns `captureLaunchOrigin()`, called synchronously at the call site that
issues the launch — `AppShell` wraps the launch model:

```ts
launch: (gameId, contentUnitId) => {
  captureLaunchOrigin();
  return gameLaunch.launch(gameId, contentUnitId);
};
```

The origin records `{ nodeId, routeKey }` and is only restored while `routeKey` still matches the
current route; otherwise the current route's own deterministic target is used. `AppShell` supplies
`routeKey` (`game:<id>` or the route name) and `fallbackNodeId` (`detail('play')`,
`settings('heading')`, or the Library heading). `SettingsPage`'s `<h1>` gained a programmatic-only
focus identity so Settings has a real deterministic target.

**Result.** All eight tests pass; the three route-awareness tests and the capture-timing test fail on
the starting HEAD.

---

### HIGH-5 — Scopes constrain movement but not activation

**Status: CONFIRMED.**

**Root cause.** `activeScope()` was consulted when collecting movement candidates but not in the
`confirm`/`context` paths, nor when deriving footer hints. A scope is a modal surface; it is
incoherent for movement to be trapped while `confirm` still fires the control behind the dialog. The
reachable states are real: clicking outside the surface moves DOM focus out of it, and a browser can
leave focus on `document.body`, after which the native-activation fallback would act on whatever was
there.

**Regression tests.**

- `src/focus/FocusProvider.test.tsx`, describe `FocusProvider scope activation boundary` — 5 tests
  covering `confirm`, `context`, the native-activation fallback, re-entry on the next directional
  action, and normal activation resuming after dismissal.
- `src/features/library/GameDetailFocus.test.tsx`, describe `Game Detail scope activation boundary` —
  4 tests on the real launch content-selection surface.
- `src/features/settings/SettingsFocus.test.tsx`, describe `SettingsPage scope activation boundary` —
  4 tests on the real removal and metadata-account confirmations.

The real-screen coverage is deliberate: proving the invariant only in a synthetic provider harness
would not show that the actual dialogs satisfy it.

**Fix.**

```ts
const activationAllowed = useCallback((): boolean => {
  const scope = activeScope();
  if (scope === null) return true;
  const active = document.activeElement;
  return active !== null && scope.element.contains(active);
}, [activeScope]);
```

Consulted in `dispatch` after `back` is handled — `back` must still work, because dismissing the
scope is the way out — and in `getSupportedActions`, so the footer stops advertising actions the
scope will refuse. The next directional action re-enters the scope, so the user is never stuck.

**Note on a related defect found while fixing this.** `getSupportedActions` had been reading a
`focusedActivatable` ref captured at focus time. That ref is a second, staler copy of a fact the DOM
already knows. It was removed in favour of reading `isActivatableElement(document.activeElement)`
live.

---

### HIGH-6 — `focusNode()` can report success when focus did not move

**Status: PARTIALLY CONFIRMED.** The underlying defect is real and is fixed. The reviewer's worked
example does not occur as described, and this is proven rather than asserted.

**What was confirmed.** `focusElement` called `element.focus()` and returned `true` unconditionally.
A disabled, `inert`, `aria-hidden`, or detached element silently ignores `focus()`, so a restoration
consumed its request and left focus on `document.body` while reporting success — and
`resolveOnRegister` requests ended on registration even when the newly registered element refused
focus. `focusRegistry` also carried its own private navigability predicate, so "can this be focused"
had two different answers in two places.

**What was not reproduced.** The review's worked example was: the launch content-selection scope
closes, its cleanup restores focus to Play, but Play has already been disabled by the launch the user
just started, so focus is lost. A probe test was written that records, in order, the scope cleanup
and the sibling `disabled` prop update. It printed:

```text
['cleanup disabled=false', 'focused=true']
```

React runs a deleted subtree's ref cleanups **before** sibling prop updates within the same commit,
so Play is still enabled at the moment the scope's cleanup runs, and the described sequence does not
happen. Reporting this as a straightforward confirmation would have been wrong.

**The genuine adjacent defect.** The restoration nonetheless runs against a DOM that React is still
committing. The correct fix is not to fight the commit but to stop restoring into the middle of it.

**Regression tests.**

- `src/focus/FocusProvider.test.tsx`, describe `FocusProvider target focusability` — 5 tests: a
  disabled target, an `inert` target, an enabled target with the programmatic heading fallback still
  usable, an unregistered target, and `'reports focusNode failure rather than a false success'`.
- `src/features/library/GameDetailFocus.test.tsx` —
  `'does not leave focus on nothing when Play is disabled by the launch it started'`, which drives the
  real choose-a-version → launch → Play-disabled flow and asserts focus lands on `BACK TO LIBRARY`.

**Fix.** `src/focus/focusability.ts` centralises the two distinct questions —
`isProgrammaticallyFocusable` (may a request target it) and `isControllerNavigable` (may movement
land on it, which excludes `tabindex="-1"` fallbacks) — and `focusMoved()` proves after the fact that
focus really arrived, accepting containment for composite controls. `focusRegistry` lost its private
predicate and gained `focusable(id)`. `FocusProvider` uses `focusMoved` for every request, takes the
fallback immediately for a present-but-unfocusable target, and only consumes a `resolveOnRegister`
request when focus actually moved. `useFocusScope`'s cleanup defers restoration by one microtask and
stands down if a focus request was made in the meantime or if focus already landed somewhere real.

**Existing test adjusted, not weakened.** `'restores the initiating target when the scope closes'`
became `async` with a `waitFor`. Its assertion is unchanged — the initiating target is still
restored — and the file records that only the timing moved.

---

### MEDIUM-1 — Window focus fails open when the native state is unknown

**Status: CONFIRMED.**

**Root cause.** `useAppWindowFocus` initialised to `true` and kept `true` when `isAppWindowFocused()`
returned `null` or rejected. Worse, a successful one-off read with a **failed subscription** granted
ownership permanently, because nothing could ever set it back to `false`. RetroFrontier would then
claim controller input while another application owned the screen.

**Regression tests.** `src/hooks/useAppWindowFocus.test.tsx` (8 tests): `'fails closed when the
native focus state cannot be read'`, `'fails closed when the native focus read rejects'`, `'does not
turn a failed focus subscription into a permanent ownership grant'`, `'does not grant ownership when
subscribing rejects'`, `'never claims ownership before the native state has been read'`, and
`'keeps a plain browser dev session usable'`.

**Fix.** `src/platform/appWindow.ts` gained `isDesktopRuntime()` (a guarded `isTauri()`), and
`onAppWindowFocusChanged` now resolves to `null` when it could not subscribe. The hook tracks
`{ focused, subscribed }` and requires **both** to be true inside the desktop runtime. Outside it the
effect is skipped entirely and the answer is `true`, so a plain `vite dev` browser session stays
usable — the test asserts `isAppWindowFocused` is not called at all in that path.

**Collateral.** Adding `isDesktopRuntime` to the platform module broke all 74 `AppShell` tests, whose
module mock did not define it. The mock, its reset list, and `setupDefaults` were updated; this is
mock maintenance, not a contract change.

---

### MEDIUM-2 — Footer hints can go stale

**Status: PARTIALLY CONFIRMED.** The defect is real for one class of transition and is fixed; the
other two transitions the review cited were already correct, for an incidental reason, and this is
reported rather than glossed over.

**Root cause.** `ControllerFooter` re-derived hints from `useFocusedNodeId()`. A node's _identity_ can
stay the same while its declared actions change — a Library card that becomes selected changes its
`context` label from `SELECT` to `DESELECT` without changing identity — and no state changed that the
footer subscribed to, so it kept advertising the old label.

**Honest scope of the reproduction.** Of the four footer tests, only
`'updates the footer action immediately when the focused card changes state'` fails on the starting
HEAD. `'drops the confirm hint when the focused control becomes disabled'` and `'shows a scope back
hint while a temporary surface is open and removes it after'` pass **with or without** the fix,
because those transitions also re-render the shell for other reasons and the footer is re-derived
incidentally. They are kept as guards against regressions in the coupling they depend on, but they
are not evidence for this finding, and it would be misleading to present them as such.

**Fix.** `FocusProvider` publishes an `actionRevision` counter through
`FocusActionRevisionContext`, bumped by `registerBack`, `pushScope`, their releases, and by
`notifyNodeActionsChanged(id)` when the changed node is the focused one. `useFocusNode` calls that
notifier when its own label signature changes. `ControllerFooter` subscribes via
`useFocusActionRevision()`. The dependency is explicit rather than incidental.

---

### MEDIUM-3 — Non-standard gamepad mappings are read as if they were standard

**Status: CONFIRMED.**

**Root cause.** `selectActiveGamepad` and `stepGamepad` checked `connected` but never `mapping`. A pad
the browser exposes with an empty or vendor mapping has undefined button indices and axis order, so
`GAMEPAD_BUTTON_INDEX` would be applied to numbers that mean something else entirely: `confirm` could
land on any face button. Silently guessing is worse than declining, because a mis-mapped `confirm`
activates something the user did not choose.

**Regression tests.** `src/input/gamepadAdapter.test.ts` — `'never selects a pad whose mapping
contract cannot be interpreted'`, `'does not let a non-standard low-index pad block a usable standard
pad'`, `'drops an active pad that stops reporting a standard mapping'`, `'reports connected-but-
unsupported pads honestly'`, and describe `stepGamepad mapping policy`. Plus
`src/hooks/useControllerInput.test.tsx` — `'does not dispatch mapped actions for a non-standard pad'`
and `'lets a standard pad at a higher index win over a non-standard pad at index 0'`.

**Fix.** `isSupportedGamepad()` (`connected ∧ mapping === 'standard'`) filters selection;
`stepGamepad` returns released state for an unsupported snapshot. `hasUnsupportedGamepad()` lets the
UI tell the truth: `useControllerInput` returns `{ connected, unsupported }`, `data-controller`
becomes `unsupported`, and `ControllerFooter` shows `CONTROLLER NOT SUPPORTED`. This narrows a
support contract, so [ADR-014](adr/ADR-014-input-acquisition-boundary.md) was updated to state it
explicitly.

---

### MEDIUM-4 — No deterministic tie-break for a diagonal deflection

**Status: CONFIRMED.**

**Root cause.** `analogueDirection` required a strict `>` on one axis and returned `null` on an exact
tie. A test had encoded that as correct. It is not: a real 45° push then does nothing at all, which
reads as a dead controller. Worse, near a tie a stick wandering across the diagonal alternates
between two directions frame to frame, producing an action storm.

**Regression tests.** `src/input/gamepadAdapter.test.ts` — `'breaks a perfect diagonal
deterministically towards the documented axis priority'`, `'keeps a held horizontal direction when
the deflection becomes an exact tie'`, `'keeps a held vertical direction when the deflection becomes
an exact tie'`, `'does not storm actions when the two axes cross over by a small amount'`, and
`'still switches axis when the other axis genuinely dominates'`.

**Fix.** A perfect tie resolves to the horizontal axis, the documented priority. Once a direction is
held, switching axes requires the other axis to exceed it by `axisDominanceMargin` (`0.15`), so
crossover jitter cannot storm while a genuine axis change still gets through immediately.

**Existing test replaced, not weakened.** `'emits nothing for a perfectly ambiguous diagonal'` was
deleted because it asserted the defect; its replacement plus four new cases is strictly stronger.

---

### Summary

| Finding                                                | Status                                                                         |
| ------------------------------------------------------ | ------------------------------------------------------------------------------ |
| HIGH-1 ownership during `launching`                    | CONFIRMED — fixed                                                              |
| HIGH-2 `awaitSettle` stale target on timeout           | CONFIRMED — fixed                                                              |
| HIGH-3 global `Escape` hijacks editing controls        | CONFIRMED — fixed                                                              |
| HIGH-4 launch return origin too late / not route-aware | CONFIRMED — fixed                                                              |
| HIGH-5 scopes do not bound activation                  | CONFIRMED — fixed                                                              |
| HIGH-6 `focusNode()` false success                     | PARTIALLY CONFIRMED — defect fixed; worked example disproven with a probe test |
| MEDIUM-1 fail-open window focus                        | CONFIRMED — fixed                                                              |
| MEDIUM-2 stale footer hints                            | PARTIALLY CONFIRMED — reproduced for the same-identity case only; fixed        |
| MEDIUM-3 non-standard gamepad mapping                  | CONFIRMED — fixed                                                              |
| MEDIUM-4 no deterministic diagonal tie-break           | CONFIRMED — fixed                                                              |

## C. Input ownership

There is now exactly one ownership derivation, `ownsApplicationInput()`:

```text
windowFocused ∧ ¬blocked ∧ running === null ∧ pendingGameId === null
```

`AppShell` computes it once per render and passes the same boolean to `useKeyboardInput`,
`useControllerInput`, and `ControllerFooter`. The audit of ownership derivations found no other
place that decides whether input may be acted on: the adapters produce actions, the provider
dispatches them, and neither consults launch state independently.

`windowFocused` itself is derived by `useAppWindowFocus`, which fails closed inside the desktop
runtime — it requires a native focus read of `true` **and** a live subscription — and returns `true`
outside it, where there is no native window to interrogate.

Ownership loss releases held controller state and re-adopts what is physically held without
emitting, so nothing replays when ownership returns. That is asserted both at the adapter
(`'does not replay a held input when ownership returns after a focus loss'`) and end-to-end
(`'does not replay a direction held across the launch transition'`).

## D. Focus lifecycle

Every focus-request lifetime was audited:

- **Route restoration (Library return).** Issued with `awaitSettle` against `useLibraryQuery`'s
  `resultVersion`. Resolves on the settle signal, or on the safety timeout — and on the timeout it now
  goes straight to the fallback, never to the unverified target.
- **Launch return.** Issued only after the application window really reports focus, once per ended
  session, against a route-scoped origin with the current route's deterministic fallback.
- **Scope entry.** Deterministic per scope, unchanged from the implementation pass.
- **Scope exit.** Deferred one microtask, and abandoned if another owner already made a request or if
  focus has already landed on a real element. The restored identity is unchanged.
- **`resolveOnRegister`.** Consumed only when `focusMoved()` returned true, so a node that registers
  in an unfocusable state does not swallow the request.

No request outlives its resolution: each is cleared when it resolves, and the "does not keep stealing
focus after a request resolved" and "leaves no obsolete request that steals focus when the old route
returns" tests assert that from both ends.

Every temporary scope was audited — the Game Detail launch content-selection surface and the two
Settings confirmations. All three now bound movement _and_ activation, dismiss on `back`, restore
their initiating target, and offer no footer hints for a node outside them. Dynamic disabled state
was audited with them: Play keeps its identity while disabled (it is the return target) but stops
offering `confirm`, and a disabled element can no longer consume a focus request.

## E. Keyboard behaviour

- Arrows, `Escape`, `ContextMenu`/`Shift+F10`, and `Enter`/`Space` are all suppressed inside
  text-editing targets: `input` (except button-like types), `textarea`, `select`, `contenteditable`,
  and `role="textbox"`.
- `Escape` outside an editing control is `back`. Inside one it is left to the platform. An
  element-level handler that consumes `Escape` still wins, because the adapter honours
  `defaultPrevented`.
- `Tab`/`Shift+Tab` are never handled, and no `tabindex` was added to anything that did not already
  have one.
- `Enter`/`Space` on a natively activatable control produces no semantic `confirm`, so nothing
  activates twice.
- Modifier chords are declined entirely.

The keyboard-interference audit found no remaining key that the application takes from a control that
owns it.

## F. Gamepad behaviour

- **Standard mapping only.** Selection filters to `connected ∧ mapping === 'standard'`; an
  unsupported snapshot steps to released state. An attached-but-unsupported pad sets
  `data-controller="unsupported"` and shows `CONTROLLER NOT SUPPORTED`.
- **Deadzone `0.55` enter / `0.35` exit**, so jitter inside the band neither re-triggers nor releases.
- **Dominant axis with a `0.15` margin**, resolving a perfect diagonal to horizontal and refusing to
  storm on crossover while still allowing a genuine axis switch.
- **Repeat** `400 ms` then `110 ms`, at most one directional action per polled frame; direction change
  and release reset it.
- **Activation buttons** are edge-triggered.
- **Disconnect, replacement, and ownership change** drop held and repeat state and adopt what is held
  without emitting.

## G. Footer behaviour

Hints are derived from the focused node's declared `confirm`/`context` labels plus the active scope's
or route's `back` label, and they now re-derive when the focused identity changes **or** when the
focused node's declared actions change underneath it. While RetroFrontier does not own input no
action hint is shown; while a scope is active, a node outside it offers no hints, matching what
activation will actually do.

## H. Automated verification

All commands run at the final HEAD.

| Command                                                    | Result                              |
| ---------------------------------------------------------- | ----------------------------------- |
| `pnpm typecheck`                                           | pass, 0 errors                      |
| `pnpm lint`                                                | pass, 0 errors, 0 warnings          |
| `pnpm format:check`                                        | pass                                |
| `pnpm test`                                                | **36 files, 529 tests, 0 failures** |
| `pnpm build`                                               | pass                                |
| `cargo fmt -- --check`                                     | pass                                |
| `cargo clippy --all-targets -- -D warnings`                | pass, 0 warnings                    |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass, 0 warnings                    |
| `cargo test`                                               | **411 passed, 0 failed, 1 ignored** |
| `cargo test --all-features`                                | **431 passed, 0 failed, 7 ignored** |
| `cargo build --release`                                    | pass                                |
| `git diff --check`                                         | clean, exit 0                       |

Frontend suite: 469 tests across 35 files at `221d2da` → **529 across 36 files** at the final HEAD.
Sixty tests were added. Two tests were replaced because they encoded incorrect contracts (HIGH-3 and
MEDIUM-4, both documented above and in the test files); one was made `async` with its assertion
unchanged. No test was weakened, skipped, or deleted to make an implementation pass.

Per-file counts for the M8 suites are in section G of
[`docs/M8_IMPLEMENTATION_REPORT.md`](M8_IMPLEMENTATION_REPORT.md). The focused M8 suites were run
three times in isolation at the final HEAD — 21 files, 303 tests, identical each run — to check for
order dependence and timing flakiness. None was observed.

## I. Manual qualification

The environment was **verified present**, not assumed: Fedora 44, kernel `7.1.9-200.fc44.x86_64`, KDE
Plasma 6, Wayland session, a Sony DualSense enumerated at `/dev/input/js1`, and WebKitGTK 2.52.5
linked against libmanette 0.2.13.

**Performed**

| Item                                                          | Result                                                                                                                                                                                                                           |
| ------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Release binary rebuilt from the corrected sources and started | **PASS.** Ran 25 s, killed by the timeout (exit 124). Clean log: startup, `managed runtime reconciled state=Ready`, storage initialized, metadata worker started. No permission denial, no capability error, no WebView failure. |

**NOT PERFORMED** — every interactive item. This session can start the application but cannot press a
controller button or observe the window, and the Gamepad API deliberately reveals nothing until the
page receives a real user gesture on the pad. No result is inferred from hardware presence.

| Item                                                                        | Result        |
| --------------------------------------------------------------------------- | ------------- |
| WebKitGTK exposes the DualSense with `mapping === 'standard'`               | NOT PERFORMED |
| Controller connect/disconnect in the running app                            | NOT PERFORMED |
| D-pad and analogue navigation, repeat feel, responsive grid                 | NOT PERFORMED |
| Sidebar navigation; open Game Detail; back restores the same game           | NOT PERFORMED |
| Mouse, Tab, search typing, Settings form editing alongside the controller   | NOT PERFORMED |
| `Escape` in the Library search does not navigate                            | NOT PERFORMED |
| Game Detail actions, Play, content selection, cancel                        | NOT PERFORMED |
| Launch a legally available game through the M7 path                         | NOT PERFORMED |
| RetroFrontier stops consuming controller UI input while RetroArch runs      | NOT PERFORMED |
| Emulator receives controller ownership                                      | NOT PERFORMED |
| Exit RetroArch; window returns; DOM focus restored once, to the right place | NOT PERFORMED |
| Focus visuals under `data-input-mode="controller"`                          | NOT PERFORMED |
| `CONTROLLER NOT SUPPORTED` appears for a non-Standard pad                   | NOT PERFORMED |

## J. Security and repository audit

- **No prohibited file became tracked.** The branch diff `77f5194..HEAD` is 48 files: 23 `.ts`,
  16 `.tsx`, 1 `.css`, 1 `.json`, 7 `.md`. `git diff --numstat` reports no binary entries. No ROM,
  BIOS, RetroArch binary, core, AppImage, generated runtime tree, database, credential, token,
  private key, private or local log, or build output is present.
- **No Rust source file was modified** in the corrective pass. `RuntimeManager`,
  `LaunchApplicationService`, the launch mutex, the durable process record, and `startup_reconcile`
  are untouched, and the M7/M7.5 backend process authority is intact.
- **Capability scope unchanged.** Still exactly one added permission, `core:window:allow-set-focus`.
  Nothing was added during the corrective pass.
- **No shell utilities.** No `xdotool`, `wmctrl`, compositor scripting, or child-process use was
  introduced.
- **Pre-existing untracked artifacts preserved.** All 29 remain untracked and byte-identical:
  `M3_REVIEW.md` … `M6_FINAL_REVIEW_2.md` and `docs/M5_IMPLEMENTATION_REPORT.md`.
- `git diff --check` is clean.

## K. Remaining risks

1. **The Standard-mapping question is unanswered on the qualification target.** If WebKitGTK 2.52.5
   exposes the DualSense with a non-Standard mapping, the pad is unusable for navigation on the one
   platform M8 is meant to qualify. The failure is honest and visible (`CONTROLLER NOT SUPPORTED`,
   keyboard fully sufficient, RetroArch unaffected because it reads the device directly) but it would
   be a qualification failure. This is the top risk and it needs a person with the controller.
2. **All interactive Linux qualification remains NOT PERFORMED**, including the RetroArch handoff and
   return that M7.5 asked M8 to measure.
3. **Scope activation containment is enforced through `document.activeElement` containment.** A future
   scope that intentionally renders into a portal outside its container element would be refused.
   No current scope does this, and the tests cover the three that exist.
4. **The scope-exit restoration is deferred by one microtask.** This is the correct fix for restoring
   into a mid-commit DOM, but it means restoration is no longer synchronous with cleanup; a future
   caller that assumes synchrony would be surprised. The behaviour is documented in
   `docs/CONTROLLER_AND_FOCUS.md` and asserted by test.
5. **Two footer tests pass with and without the MEDIUM-2 fix** because those transitions re-render the
   shell incidentally. They guard the coupling but do not prove the fix; only the same-identity test
   does.
6. **Windows and macOS remain entirely unqualified** for controller enumeration, mapping fidelity, and
   window activation semantics.

## L. Final verdict

**M8 CORRECTIVE PASS — READY FOR REVIEW**

All ten review findings were investigated against the real code, reproduced with failing regression
tests on the starting HEAD wherever the behaviour genuinely differed, fixed at the root cause, and
re-verified. Two findings are reported as PARTIALLY CONFIRMED with the evidence for the narrower
claim, rather than overstated. All required automated checks pass at the final HEAD. The interactive
Linux qualification in section I is **NOT PERFORMED** and remains the outstanding gate.
