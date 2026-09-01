# M8 — Controller and Focus: final corrective pass report

This report records the **second and final** adversarial corrective pass on `feat/m8-controller-focus`.
It is written to be re-checked by one more independent reviewer: every claim below names the code, the
regression test, and the observed result that supports it, and every finding that was *not*
reproducible says so with the reason.

Companion documents: [`docs/CONTROLLER_AND_FOCUS.md`](CONTROLLER_AND_FOCUS.md) (the behavioural
contract, updated by this pass), [`docs/M8_IMPLEMENTATION_REPORT.md`](M8_IMPLEMENTATION_REPORT.md),
[`docs/M8_CORRECTIVE_REPORT.md`](M8_CORRECTIVE_REPORT.md) (the first corrective pass), and
[ADR-014](adr/ADR-014-input-acquisition-boundary.md).

The previous corrective pass materially improved M8 and was **not** rewritten. Nothing here is an
architectural redesign: every change is the smallest correction that makes the existing architecture
satisfy the requirement.

## A. Starting state

|                            |                                                                                              |
| -------------------------- | -------------------------------------------------------------------------------------------- |
| Branch                     | `feat/m8-controller-focus`                                                                   |
| Starting local `HEAD`      | `5399be498e45c10adbf5117a77a8e463345f49d2` — `docs(input): record M8 corrective findings`     |
| Starting `origin/feat/m8-controller-focus` | `5399be498e45c10adbf5117a77a8e463345f49d2` — identical, so the branch had not advanced |
| M7.5 base (`main` / `origin/main`) | `77f5194c76c360bd6eb14e8546a7a4e0998be1aa` — both, unchanged                        |
| Final local `HEAD`         | the ninth commit, `docs(input): finalize M8 corrective qualification record`, which carries this report (a report cannot name its own commit's hash; `git log -1` gives it) |
| Pushed                     | The nine commits of this pass (eight code/hygiene plus the documentation commit) were **local only when this report was written**. They were **subsequently pushed** as a fast-forward: `origin/feat/m8-controller-focus` went from `5399be498e45c10adbf5117a77a8e463345f49d2` to `d19b7a917f521fb53beac3c35f908ffd5ca6ef58`. No history was rewritten. |
| PR / merge                 | None. No pull request was opened; nothing was merged to `main`.                               |

> **Repository-state note, added after the fact.** The "Pushed: No" row above was accurate at the
> moment this report was generated. The pass was reviewed and then pushed, so the row now records the
> actual history instead. A **third** pass followed — the launch-interaction lifecycle fix in
> [`docs/M8_LAUNCH_LIFECYCLE_FINAL_REPORT.md`](M8_LAUNCH_LIFECYCLE_FINAL_REPORT.md) — which starts
> from `d19b7a9` and is local only unless instructed otherwise. Behaviour that pass changed (transient
> launch-state ownership, and the content-selection scope's restoration) is authoritative **there**.

The starting state was verified, not assumed, before any file was changed:

```text
branch: feat/m8-controller-focus
HEAD:                             5399be498e45c10adbf5117a77a8e463345f49d2
origin/feat/m8-controller-focus:  5399be498e45c10adbf5117a77a8e463345f49d2
main / origin/main:               77f5194c76c360bd6eb14e8546a7a4e0998be1aa
```

The branch had **not** advanced beyond the expected SHA, so no additional commits needed inspection.
The full branch diff `77f5194..HEAD` was read before the first finding was investigated.

### Working-tree state at start

28 pre-existing untracked review artifacts (`M3_REVIEW.md` … `M6_FINAL_REVIEW_2.md`) plus one
*tracked* historical artifact, `docs/M5_IMPLEMENTATION_REPORT.md`, which should have been untracked
(MEDIUM-FINAL-2). No other modifications.

### Corrective commits

```text
46bbfe1 fix(focus): make game return restoration transition-safe
ddceea0 fix(launch): preserve focus origin across content selection
5c433e6 fix(focus): make native window ownership ordering-safe
42ddd53 fix(input): revoke controller polling ownership synchronously
64eeacb fix(focus): add deterministic launch failure scope
8ef8fb9 fix(ui): close dynamic controller footer hint gaps
30df1c0 chore(repo): restore historical review artifact to untracked state
06804b3 test(focus): cover scope replacement across the launch surfaces
```

A ninth commit, `docs(input): finalize M8 corrective qualification record`, carries this report and
the documentation updates, and is the final local `HEAD`.

## B. HIGH-FINAL findings

### HIGH-FINAL-1 — DOM focus return can be skipped if RetroFrontier is already focused — **CONFIRMED**

**Reproduction.** `src/hooks/useLaunchFocusReturn.test.tsx`
→ `restores DOM focus when the window was already focused as the process ended`:

```text
PLAY focused, origin captured
running = session, windowFocused = false, DOM focus blurred
windowFocused = true      (the user returns while the game still runs)
running = null            (RetroArch exits, window still focused)
```

Failed on the starting HEAD with `PLAY` never receiving focus (`toHaveFocus()` timed out after
1017 ms).

**Root cause.** The return was recorded in a ref (`returnPending.current = true`) by the effect that
observed `running -> null`, while a *second* effect keyed on `[api, windowFocused]` performed the DOM
restoration. When the window was already focused at exit, `windowFocused` did not change, that
effect's dependencies did not change, and it never re-ran. A ref mutation cannot schedule the effect
that reads it, so the return stayed pending for the rest of the session. The native `setFocus()` still
happened, which is why the bug was silent — the window came forward with no logical focus in it.

**Fix.** `src/hooks/useLaunchFocusReturn.ts`. The pending return became an explicit
**return-generation state** plus a ref payload:

- the exit transition sets `pendingReturn.current = { sessionId, origin }` **and** bumps
  `returnGeneration` state, so the transition itself is what schedules the restore effect;
- the restore effect depends on `[api, returnGeneration, windowFocused]`, so either signal wakes it;
- it consumes `pendingReturn.current` by clearing the ref — no state update inside the effect, which
  also keeps `react-hooks/set-state-in-effect` satisfied honestly rather than by suppression.

**Invariants held, each with a test:** exactly one native `setFocus()` per ended session
(`asks for the application window exactly once per ended session`); no retry loop and no repeated
focus stealing (`does not restore focus repeatedly once it completed`,
`does not restore again when idle state rerenders after a completed return`); DOM focus only once the
window really reports focus (`restores DOM focus only after the application window is focused
again`); blocked/uncertain state still delays honestly (`never requests the window while launch state
is still uncertain`).

**Adjacent cases audited.** Startup with an already-adopted running session (`falls back within the
current route when nothing recorded the launch origin`); route change during the run (`does not drag
the user back to the route the launch started from`); no obsolete request left in the old route
(`leaves no obsolete request that steals focus when the old route returns`).

**Result.** 16 tests in the file, passing; run three times consecutively with no variation.

### HIGH-FINAL-2 — Multi-content launch overwrites the original PLAY launch origin — **CONFIRMED**

**Reproduction.** Two tests, both failing on the starting HEAD:

- `keeps the original PLAY origin across a content-selection continuation` — PLAY, then
  `contentSelectionRequired`, then confirming a version, then run and exit. `PLAY` did not receive
  focus (timed out at 1017 ms), because the request targeted the unmounted
  `launch:content:101` and had to wait for the 1.2 s safety fallback.
- `leaves no request that an obsolete content option could satisfy later` — after the return, opening
  a content selection again let the stale `resolveOnRegister` request grab the temporary option node.

**Root cause.** `AppShell` wrapped `launch()` so that *every* call captured an origin. The second
call — the content-option confirmation — is part of the same launch attempt, but it replaced the PLAY
identity with the temporary `launch:content:<ContentUnitId>` node, which does not exist when
RetroArch exits.

**Fix.** A multi-step content selection is now modelled as one launch **interaction**.
`captureLaunchOrigin()` became `beginLaunchInteraction()`, which captures **only when no interaction
is open**; the shell still calls it on every launch, and the hook decides whether that is a start or a
continuation. The interaction's lifetime is decided from the launch facts the shell already owns
(`pendingGameId`, `contentSelectionOpen`, `running`, `blocked`) rather than from a node-prefix
special case. Precise dispositions:

| Case | Interaction |
| --- | --- |
| Failed initial launch | discarded — HIGH-FINAL-5's failure surface owns focus from there |
| `contentSelectionRequired` | held: a continuation, not a resolution |
| Cancelled content selection | discarded |
| Successful running launch | held, then consumed by the return |
| Failed second (content-selected) launch | discarded |
| New later launch | captures a fresh origin |
| `blocked` | held — nothing may be concluded while the state is uncertain |

**Tests.** The two reproductions above, plus `clears the origin when the content selection is
cancelled`, `clears the origin when a launch fails without ever running`, `clears the origin when the
second, content-selected launch fails`, and `holds the open interaction while launch state is
uncertain`. The four "clears" tests pass on the starting HEAD too — that is deliberate: they exist to
prove the new "capture only if nothing is open" rule did **not** make the origin sticky.

**Adjacent cases audited.** All references to the launch-return state were re-read
(`beginLaunchInteraction`, `interactionOrigin`, `pendingReturn`, `previousRunning`,
`requestedForSession`, `requestAppWindowFocus`); the shell has exactly one call site.

### HIGH-FINAL-3 — Native focus fail-closed logic still has ordering races — **CONFIRMED (both races)**

**Reproduction.** `src/hooks/useAppWindowFocus.test.tsx`, `useAppWindowFocus bootstrap ordering`.
Five tests failed on the starting HEAD:

- `does not read the native focus state before the subscription is established` — the read was issued
  at mount, which is the precondition for Race A.
- `observes a focus change that happened before the subscription attached` (**Race A**) — the native
  state is `true` at mount and `false` by the time the subscription attaches. The hook granted
  ownership from the stale `true` read.
- `never lets a stale initial read override a newer focus event` (**Race B**) — a `focus=false` event
  arrived while the read was in flight; the read then resolved `true` and resurrected ownership.
- `keeps an event-established focus state when the read fails afterwards`.
- `releases a subscription that resolves after unmount`.

**Root cause.** `isAppWindowFocused()` and `onAppWindowFocusChanged()` were started concurrently, so
there was (a) an interval with no listener attached in which a real transition was lost forever, and
(b) no ordering evidence to tell an older read from a newer event.

**Fix.** The two observations are sequenced: fail closed → subscribe → **then** read → and once
subscribed, events are authoritative. An event counter captured before the read is the ordering
evidence: a read that resolves after the counter moved stands down. No fail-open mode was
reintroduced; a read that returns `null` or rejects still fails closed while staying subscribed, so a
later event can establish ownership honestly.

**Adjacent cases audited, all with tests:** read failure and rejection; subscription returning `null`;
subscription rejecting; plain browser / dev runtime (`keeps a plain browser dev session usable`, which
also asserts the boundary is not called at all); unmount during a pending subscription; unmount during
a pending read; several rapid focus changes in order.

**Result.** 16 tests in the file, passing; run three times consecutively with no variation.

### HIGH-FINAL-4 — Gamepad ownership loss reaches the RAF loop through a passive effect — **CONFIRMED for controller; NOT REPRODUCED for keyboard**

**Reproduction.** `src/hooks/useControllerInput.test.tsx`,
`useControllerInput ownership revocation ordering`. The tests are ordering-sensitive by construction
rather than settled-state assertions: an `OwnershipFrameProbe` is rendered **before** the controller
host, so its own passive effect runs before the host's passive effects, and it fires a polled frame
from there — exactly the interval a passive ownership gate leaves open. Three tests failed on the
starting HEAD:

- `cannot dispatch a held activation on a frame that runs inside the revoking commit` — `confirm`
  was delivered after React had committed `ownsInput === false`;
- `cannot dispatch or repeat a held direction on a frame inside the revoking commit`;
- `adopts the physically held input when ownership returns, then honours a real press`.

**Root cause.** The dispatch gate (`enabledRef`) was written from `useEffect`. Passive effects are
flushed in a separate scheduler task after the commit, so a `requestAnimationFrame` callback can run
in between and observe the previous `true` — one extra semantic controller frame produced from input
that already belonged to the emulator.

**Fix.** The gate is written in `useLayoutEffect`, which runs synchronously inside the commit and
therefore before the browser can paint and before any animation frame of the next frame. There is no
render-phase side effect. Release and re-adoption at the transition are unchanged: held and repeat
state is dropped and whatever is physically held is adopted without emitting, in both directions.

**Keyboard audit — NOT REPRODUCED, and documented as such.** `useKeyboardInput`'s listener lifetime
had the same shape, and an equivalent probe test was written
(`cannot dispatch a key delivered inside the commit that revoked ownership`). It passes against **both**
the old and the new implementation, because React flushes pending passive effects before dispatching a
new discrete event, so the interval is not actually reachable from a `keydown`. The gate was still
moved to `useLayoutEffect` so there is one ownership contract rather than two, and the test's own
comment says plainly that it is a contract guard and not a reproduction. This is recorded rather than
presented as a fix that was needed.

**Result.** 10 tests in `useControllerInput.test.tsx`, 9 in `useKeyboardInput.test.tsx`, 48 in
`src/input/`; the set was run five times consecutively with no variation.

### HIGH-FINAL-5 — Launch failure is still outside the deterministic focus-scope model — **CONFIRMED**

**Reproduction.** `src/features/library/GameDetailFocus.test.tsx`,
`Game Detail launch failure scope`. Eight tests, all failing on the starting HEAD (the surface had no
`role="group"` container at all, so every locator failed). The failure is always raised *after* mount
in these tests, because that is the only way it can occur — it is the answer to a launch started on
this screen; rendering it at mount would race the route-entry heading focus and test an unreachable
state.

**Root cause.** Launch failure rendered as a bare `InlineError` with an `actionLabel`. No scope, no
stable focus identity, no entry focus, no `back` behaviour, no containment. The concrete acceptance
gap from the review reproduced exactly: after a content-selected launch, the closing selection scope
found Play disabled (pending) and took its `detail:back` fallback, so focus sat on BACK TO LIBRARY
when the failure appeared, and controller `back` navigated to the Library instead of dismissing it.

**Fix.** A Game Detail-specific `LaunchFailureNotice` (`InlineError` itself is untouched, because no
other screen has this requirement):

| Requirement | Implementation | Test |
| --- | --- | --- |
| Stable scope id | `focusScopes.launchFailure` = `scope:launch-failure` | — |
| Stable action identity | `detail:dismiss-launch-failure` | — |
| Entry focus on DISMISS | scope `initialFocus: 'auto'`; DISMISS is the surface's first focusable | `moves focus to DISMISS when a launch failure appears` |
| `confirm` dismisses | registered `confirm` metadata + native activation | `dismisses with confirm and restores the Play action` |
| `back` dismisses, does not navigate | scope `onDismiss`, asserted from focus deliberately moved outside | `dismisses with back instead of navigating to the Library` |
| Movement contained | scope container is the candidate root | `keeps directional movement inside the failure surface` |
| No controller reach-through | existing `activationAllowed()` refusal | `refuses to activate an underlying control reached with the pointer` |
| Dismissal restores PLAY | explicit `requestFocus(detail:play)` issued before the surface unmounts | `dismisses with confirm…`, `takes focus after a content-selected launch fails, then restores Play` |
| Truthful fallback | `detail:back` when PLAY cannot take focus | `falls back to the Back action when Play cannot take focus` |
| Route changed instead | `restore: 'none'`, so an unmount restores nothing | `does not force focus back when the surface unmounts with the route` |
| Pointer / Tab native | non-modal; nothing trapped, nothing force-focused | covered by the reach-through test |

No Retry is invented: the launch contract offers dismissal only, so the surface offers dismissal only.
Restoration is explicit rather than the scope's automatic restore, because a dismissal and an unmount
are different events and only the first one means "the user is still here and wants Play back".

**Adjacent cases audited.** `test(focus): cover scope replacement across the launch surfaces` covers
the scope stack directly: a content-selected launch that fails replaces one scope with another in a
single commit, and the test asserts the closing selection scope's release does not take the newly
registered failure `back` handler with it, that `back` reaches the innermost surface, and that `back`
returns to the screen once both temporary surfaces are gone.

**Two pre-existing AppShell tests were updated, not weakened.** `stops consuming controller input
while a launch request is still pending` and `does not replay a direction held across the launch
transition` both ended by asserting free movement after a *failed* launch. That is no longer the
correct behaviour — a failure now legitimately owns focus. Both now assert the stronger sequence:
ownership returns immediately, the failure surface takes focus, the controller dismisses it with
`confirm`, Play is restored, and only then does movement resume. The held-direction test additionally
still proves the held direction is not replayed while the failure surface holds focus.

## C. MEDIUM findings

### MEDIUM-FINAL-1 — Footer reactivity incomplete for unregistered dynamic native controls — **CONFIRMED, plus a second gap found**

**Reproduction.** Two failing tests:

- `src/features/settings/SettingsFocus.test.tsx` →
  `follows the managed-runtime action through its disabled transitions`. The managed-runtime action
  becomes disabled while it keeps focus; the footer kept its stale hint.
- `src/focus/FocusProvider.test.tsx` →
  `re-evaluates an unregistered control when its activatability changes`.

**A second gap surfaced while writing the second test** and is fixed with it: focusing an
*unregistered* control did not re-derive the hints **at all**, because two unregistered controls both
have a `null` semantic identity, so the identity-keyed state never changed. Moving focus between two
unregistered controls with different activatability therefore showed a stale hint for a different
reason than the one the finding described.

**Root cause.** The action revision was bumped only by `notifyNodeActionsChanged` (registered nodes),
`registerBack`, and `pushScope`. Nothing represented "a generic control's activatability changed" or
"the focused element changed but its identity did not".

**Fix — one policy, two levels, as the finding asked.**

- **Option A where it matters.** The managed-runtime install/repair action registers as
  `settings:runtime:action` with honest, dynamic metadata, so the footer names what the button really
  does (`INSTALL RUNTIME` / `REPAIR RUNTIME` / `REINSTALL RUNTIME`) and offers nothing while it is
  disabled. A generic `CONFIRM` was never the right hint for a primary action.
- **Option B as the floor.** `FocusProvider` invalidates the derivation when the focused *element*
  changes, and watches that element's own activation attributes. This is **not** a broad
  `MutationObserver`: exactly one element is observed — whichever currently has focus — with
  `attributeFilter` limited to what `isActivatableElement` reads (`disabled`, `href`, `type`,
  `hidden`, `aria-hidden`, `inert`). It is re-pointed on each focus change and disconnected on
  unmount, so it costs one observer for the whole application and cannot fan out over the tree.

**Audit of the other dynamic controller-reachable controls.** `disabled` toggles from local state on:
the Library pagination buttons and empty-state actions, the scan-issue "load more", the Settings
per-root actions, add/rescan, and metadata account save/clear. The Play button, the metadata action,
the "forget provider choice" action, and the metadata candidates were already registered. Every one of
the unregistered ones is now covered generically by the floor above; the policy is documented in
`docs/CONTROLLER_AND_FOCUS.md` so a future control has a rule to follow — a control whose activation
semantics change while it can hold focus should declare an identity.

`docs/CONTROLLER_AND_FOCUS.md` also documents `settings:root:<rootId>:<action>` honestly as
**reserved but not yet used**; it was listed as an implemented identity while nothing referenced it.

**SELECT → DESELECT still works.** `updates the footer action immediately when the focused card
changes state` (AppShell) passes unchanged.

**Second test kept as a guard:** `does not offer confirm for a runtime action this build cannot
perform` (no approved release source → no hint at all) passes on the starting HEAD too, and exists so
the registration cannot start claiming an action the build cannot perform.

### MEDIUM-FINAL-2 — Accidentally tracked historical artifact — **CONFIRMED**

`docs/M5_IMPLEMENTATION_REPORT.md` was tracked at the starting HEAD. `git log --diff-filter=A` shows
it was added by `5399be4` (`docs(input): record M8 corrective findings`) — the final documentation
commit of the first corrective pass — and it was **not** tracked at the original M8 HEAD `221d2da`,
exactly as the finding stated.

Removed from the index only. See § I for the verification.

### MEDIUM-FINAL-3 — Stale repository-state claims in the committed reports — **CONFIRMED**

Both reports claimed the corrective commits were not pushed. That was true when they were written and
false by the time this pass began. Corrected in place without rewriting past facts: each report now
states what actually happened and flags the sentence that has been superseded, rather than silently
editing history.

The recorded history is now: the six original M8 commits were pushed first, at
`221d2da571da831657f0e746c97516bf6f615120`; the first corrective pass was initially local and was
subsequently pushed, taking `origin/feat/m8-controller-focus` to
`5399be498e45c10adbf5117a77a8e463345f49d2`; the eight commits of this second/final pass remain local.
`docs/M8_CORRECTIVE_REPORT.md` additionally now flags the two other claims this pass found to be
wrong: the M5 artifact did not remain untracked, and five behaviours it reported as fixed were only
partly fixed.

### LOW — Keyboard ownership documentation — **CONFIRMED; conservative behaviour kept**

The code was already correct and intentional: `AppShell` passes the same `ownsApplicationInput()`
value to `useKeyboardInput` and `useControllerInput`, so semantic keyboard navigation is disabled
during a pending launch, a running game, a blocked state, and an unfocused window. That is kept —
there is no concrete requirement for keyboard navigation to stay live during an emulator session, and
a second, more permissive rule would be a second copy of the ownership question that could drift.

Only the documentation was wrong. "Keyboard behaviour follows OS window focus naturally" was replaced
with an explicit statement of the shared gate, what it does *not* suppress (Tab order, native
`Enter`/`Space` on real controls, text entry, browser shortcuts — the adapter never handled those),
and the note that any future keyboard/controller ownership split is a deliberate decision needing its
own ADR, not a quiet divergence. No new ownership architecture was introduced.

## D. Final launch-return state machine

```text
                        beginLaunchInteraction()          (synchronous, at the user's click)
                                  │
                                  ▼
        ┌──────────────── interactionOrigin = { nodeId, routeKey } ────────────────┐
        │  captured ONLY if no interaction is already open                          │
        ▼                                                                           │
  pendingGameId set ──► contentSelectionRequired ──► beginLaunchInteraction() again │
        │                        │                    (continuation: no re-capture) │
        │                        └──► cancelled ───────────────► DISCARD ────────────┤
        │                                                                            │
        ├──► failed (either step) ──────────────────────────────► DISCARD ────────────┤
        │        └─ launch-failure scope owns focus and restores PLAY on dismissal    │
        │                                                                            │
        └──► running = session                                                       │
                 │                                                                   │
                 │  blocked ──► HOLD (session and interaction both held)             │
                 │                                                                   │
                 ▼  running -> null, once per sessionId                              │
          requestAppWindowFocus()   (exactly once, no retry)                          │
          pendingReturn = { sessionId, origin };  returnGeneration += 1  ◄────────────┘
                 │
                 ▼  windowFocused === true  (immediately, if already true)
          consume pendingReturn (ref cleared; cannot repeat)
                 │
                 ├─ origin.routeKey === current routeKey ──► requestFocus(origin.nodeId, fallback: route target)
                 └─ otherwise ─────────────────────────────► requestFocus(current route target)
```

**Launch origin lifetime.** Captured synchronously at the launch click; that is the moment of intent,
and it records the semantic identity *and* the logical route. It survives a content-selection
continuation and is discarded the moment the interaction resolves without a process.

**Content-selection continuation.** One interaction, one origin. The shell calls
`beginLaunchInteraction()` on every launch; the hook decides start vs. continue. The temporary
`launch:content:<ContentUnitId>` node is therefore never a return target.

**Running / blocked / exit.** The backend stays the only authority. While `blocked`, nothing is
concluded: neither the session nor the interaction is consumed. On a real `running -> null`
transition, guarded once per `sessionId`, the native focus request is issued and the pending return
becomes observable.

**Native focus request.** `requestAppWindowFocus()` exactly once per ended session, through the Tauri
window API. No retry; a compositor that refuses is not fought.

**DOM focus restoration.** Only once the window really reports focus — and immediately if it already
does. Consumed by clearing a ref, so it cannot repeat and cannot fight a focus the user has since
moved themselves.

**Route changes.** The origin is only restored while its own route is still on screen. Otherwise the
current route's deterministic target is used (`library:heading`, `settings:heading`, or `detail:play`)
and no obsolete request is left pending in another route.

## E. Native focus ownership

```text
state = { focused: null, subscribed: null }        → ownership refused (fail closed)
   │
   ├─ onAppWindowFocusChanged(handler) ────────────┐
   │      handler: observedEvents += 1;            │  events are authoritative once subscribed
   │               focused = payload               │
   │                                               │
   ├─ resolves null  ──► subscribed = false ──────► ownership refused, and the state is NOT read
   ├─ rejects        ──► subscribed = false ──────► ownership refused
   └─ resolves release ─► subscribed = true
              │
              ▼   only now
        eventsAtStart = observedEvents
        isAppWindowFocused()
              ├─ resolves v, observedEvents === eventsAtStart ──► focused = v
              ├─ resolves v, observedEvents  >  eventsAtStart ──► DISCARDED (older observation)
              ├─ rejects / null, no event seen ────────────────► focused = null (fail closed, still subscribed)
              └─ any resolution after unmount ────────────────► ignored

ownership = focused === true  ∧  subscribed === true      (desktop)
ownership = true                                          (plain browser dev server)
```

**Ordering.** Subscribe first, read second. Reading first is what loses a transition that happens
before the listener attaches — the precondition for Race A.

**Stale-read prevention.** `observedEvents` is captured before the read is issued and compared after
it resolves. A read that resolves after an event arrived is an older observation of a state the event
already superseded, so it is discarded rather than allowed to overwrite it (Race B).

**No fail-open.** Every unknown on the desktop side refuses ownership. An unreadable state does not
become a permanent refusal either: the subscription stays, so a later event still establishes
ownership honestly. The plain-browser dev runtime remains intentionally usable and does not call the
native boundary at all.

## F. Controller ownership transition

`ownsInput` is committed by React; the poller reads a ref. The question is whether any animation frame
can observe the ref *after* the commit but *before* the ref is updated.

- The ref is written in `useLayoutEffect`. Layout effects run **synchronously inside the commit**,
  before the browser can paint and therefore before any `requestAnimationFrame` callback of the next
  frame. There is no scheduler task between the commit and the write for a frame to slip into.
- With the previous `useEffect`, the write was a passive effect flushed in a *separate* scheduler task
  after the commit — the interval the three ordering tests exploit and which they demonstrated by
  firing a frame from a sibling's passive effect that runs earlier in the same flush.
- Nothing happens during render, so no render-phase side effect was introduced.

At the transition, in both directions: `releaseGamepadOwnership()` drops direction, `directionArmed`,
`nextRepeatAt`, and all three button states, and sets `adopting`; a single `stepGamepad()` then adopts
whatever is physically held **without emitting**. A held `confirm` or direction therefore produces
nothing until it is released and pressed again, and adoption happens at the moment ownership changes
rather than on the next frame, so a genuine press immediately after focus returns is still delivered.

`useKeyboardInput` uses the same mechanism for the same contract. Its interval is not reachable in
practice (React flushes pending passive effects before a discrete event), which is stated in the code
and in the test rather than claimed as a fix.

## G. Launch-failure focus surface

| Aspect | Behaviour |
| --- | --- |
| Entry | The scope focuses its first focusable on attach; DISMISS is that element |
| `confirm` | Dismisses (registered `confirm: { label: 'DISMISS' }`, plus native activation) |
| `back` | Dismisses, from wherever focus sits — the scope owns `back` while it is open |
| Containment | The scope container is the candidate root, so directional movement cannot leave it |
| Reach-through | `confirm`/`context` refused while focus is outside the scope; the footer stops offering them |
| Restore on dismissal | `requestFocus(detail:play, { fallback: detail:back, resolveOnRegister })`, issued *before* the surface unmounts, while PLAY is present and enabled |
| Restore on unmount | Nothing (`restore: 'none'`) — a user who navigated away is not dragged back |
| Pointer / Tab | Untouched; the surface is deliberately not browser-modal |
| Retry | None. The launch contract offers dismissal only |

## H. Footer reactivity

The derived hints are invalidated by a `FocusProvider`-owned revision, bumped on: a focused node's
declared labels changing; a scope opening or closing; a back handler appearing or disappearing; the
focused **element** changing; and the focused element's own activation attributes changing.

- **Registered dynamic controls** report through `notifyNodeActionsChanged` and get an honest label.
  `settings:runtime:action` is the one this pass added.
- **Unregistered native controls** are covered by the last two triggers. The attribute watch is one
  `MutationObserver` on one element, attribute-filtered, re-pointed on focus change, disconnected on
  unmount — not a broad DOM observer.
- Only the footer subscribes to the revision context, so nothing else re-renders.

## I. Repository hygiene

`docs/M5_IMPLEMENTATION_REPORT.md` **is not tracked.**

```text
$ git ls-files docs/M5_IMPLEMENTATION_REPORT.md
                                     (empty — untracked)
$ ls -l docs/M5_IMPLEMENTATION_REPORT.md
-rw-r--r--. 1 ben ben 26195 ...       (present locally)
```

Byte-identical before and after: 26195 bytes, md5 `465bbb2b3481765d549be2727c0d71f4`. Removed from the
index with `git rm --cached` only; the working file was never touched.

All 29 historical artifacts are present locally and untracked. None of the other 28 was staged at any
point during this pass. The branch diff `77f5194..HEAD --name-status` no longer contains
`docs/M5_IMPLEMENTATION_REPORT.md`.

## J. Automated verification

Run at `06804b3` (the last code commit) and re-run at the final documentation HEAD, with identical
results. The documentation commit touches only `docs/`.

| Command | Result |
| --- | --- |
| `pnpm typecheck` | pass (`tsc -b`) |
| `pnpm lint` | pass (`eslint .`, 0 problems) |
| `pnpm format:check` | pass |
| `pnpm test` | **36 files, 562 tests, all passing** |
| `pnpm build` | pass |
| `cargo fmt -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | **411 passed, 0 failed, 1 ignored** |
| `cargo build --release` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo test --all-features` | **431 passed, 0 failed, 7 ignored** |
| `git diff --check` | clean |

Frontend test count: 529 at the starting HEAD → **562** at the final HEAD (+33).

### Repeated runs for timing flakiness

| Suite | Runs | Result |
| --- | --- | --- |
| `useLaunchFocusReturn` | 3 | 16/16 each time |
| `useAppWindowFocus` | 3 | 16/16 each time |
| `useControllerInput` + `useKeyboardInput` + `src/input` | 5 | 67/67 each time |
| `src/focus` + `src/features/settings` + `src/app` | 3 | 194/194 each time |
| `GameDetailFocus` + `AppShell` + `src/hooks` | 3 | 225/225 each time |

### Branch shape

`git diff 77f5194..HEAD --name-status` is 51 files: 23 `.ts`, 17 `.tsx`, 1 `.css`, 1 `.json`,
9 `.md`. `git diff --numstat` reports no binary entries, and
`docs/M5_IMPLEMENTATION_REPORT.md` is not among them.

## K. Manual qualification

### Environment — re-verified, not assumed

| Fact | Observed | Verdict |
| --- | --- | --- |
| Fedora 44 | `Fedora release 44 (Forty Four)` | PASS |
| KDE Plasma 6 | `plasmashell 6.7.4` | PASS |
| Wayland | `XDG_SESSION_TYPE=wayland`, `XDG_CURRENT_DESKTOP=KDE` | PASS |
| DualSense at `/dev/input/js1` | `/sys/class/input/js1/device/name` = `Sony Interactive Entertainment DualSense Wireless Controller` | PASS |
| WebKitGTK 2.52.5 | `pkg-config --modversion webkit2gtk-4.1` = `2.52.5` | PASS |
| libmanette 0.2.13 | `libmanette-0.2.13-2.fc44.x86_64` | PASS |
| Tauri 2.11.5 | `src-tauri/Cargo.lock` → `tauri 2.11.5` | PASS |

Note for the operator: four joystick devices are present, not one. `js0` is an `ASRock LED
Controller`, `js1` the DualSense, `js2` the DualSense motion sensors, `js3` a
`Microsoft X-Box 360 pad 0`. This matters for the enumeration item below — RetroFrontier's controller
selection is "lowest connected index whose `mapping === 'standard'`", so which pad the frontend picks
is worth recording rather than assuming.

### Interactive items — NOT PERFORMED

Every item below is marked **NOT PERFORMED — HUMAN INTERACTION REQUIRED**. Claude Code cannot press a
physical DualSense button, cannot observe the application window or the compositor's activation
decision, and cannot read WebKitGTK's `Gamepad.mapping` (which is the value the M8 contract depends
on, and which a Chrome-based check would not answer). Hardware presence is **not** accepted as
evidence for any of them.

#### Operator checklist

Start the application with `pnpm tauri dev` from the repository root and keep the WebKit inspector
open where a value must be recorded.

**1. Controller enumeration**

| # | Step | Record | Verdict |
| --- | --- | --- | --- |
| 1.1 | Start RetroFrontier; press any DualSense button | footer shows `CONTROLLER CONNECTED` | NOT PERFORMED |
| 1.2 | In the inspector: `[...navigator.getGamepads()].filter(Boolean).map(p => [p.index, p.id, p.mapping])` | the full list | NOT PERFORMED |
| 1.3 | Confirm the pad RetroFrontier selected | `Gamepad.id` | NOT PERFORMED |
| 1.4 | Confirm its mapping | `Gamepad.mapping` — **PASS requires exactly `"standard"`** | NOT PERFORMED |
| 1.5 | If any non-Standard pad is present, confirm it neither takes ownership nor blocks the DualSense | which index won | NOT PERFORMED |

**2. Library**

| # | Step | Verdict |
| --- | --- | --- |
| 2.1 | D-pad up/down/left/right moves between cards along visual rows and columns; an edge is a stop, never a wrap | NOT PERFORMED |
| 2.2 | Left stick does the same; jitter near a threshold changes nothing | NOT PERFORMED |
| 2.3 | Holding a direction: one step, ~400 ms pause, then a steady ~110 ms repeat; no burst after a stall | NOT PERFORMED |
| 2.4 | An exact/near diagonal produces exactly one direction and does not alternate while held | NOT PERFORMED |
| 2.5 | Resize the window so the grid reflows; movement still follows the *rendered* rows | NOT PERFORMED |
| 2.6 | `X` selects the focused card; footer switches `SELECT` → `DESELECT` with focus unmoved; `X` again reverts | NOT PERFORMED |
| 2.7 | `A` opens Game Detail; `B` returns to the Library with focus on the **same** card | NOT PERFORMED |
| 2.8 | Move left off the grid into the sidebar and back | NOT PERFORMED |

**3. Search / Settings**

| # | Step | Verdict |
| --- | --- | --- |
| 3.1 | Click the Library search field and type: characters, caret movement, and selection all behave natively | NOT PERFORMED |
| 3.2 | Press `Escape` in the search field: it does **not** navigate and does not clear via a semantic Back | NOT PERFORMED |
| 3.3 | Settings → metadata account: type in username and password normally; `Escape` does not navigate away | NOT PERFORMED |
| 3.4 | `Tab` / `Shift+Tab` walk the native tab order everywhere, including out of a temporary surface | NOT PERFORMED |
| 3.5 | Settings root removal: confirmation takes focus; `B`/`Escape` cancels and returns focus to the trigger | NOT PERFORMED |
| 3.6 | Settings metadata-account clear: same confirmation behaviour | NOT PERFORMED |
| 3.7 | Focus the managed-runtime action: footer shows its real label (`INSTALL`/`REPAIR`/`REINSTALL RUNTIME`), not `CONFIRM` | NOT PERFORMED |

**4. Game Detail**

| # | Step | Verdict |
| --- | --- | --- |
| 4.1 | `A` on Play launches; `A` on Favorite toggles and the footer label flips | NOT PERFORMED |
| 4.2 | `B` returns to the Library | NOT PERFORMED |
| 4.3 | Multi-content game: the version list takes focus, movement stays inside it, `B` cancels and focus returns to Play | NOT PERFORMED |
| 4.4 | Trigger a reproducible launch failure — the safest is Settings → runtime not installed, then Play — and verify: DISMISS takes focus; movement stays in the failure surface; `A` dismisses; `B` also dismisses **without** navigating to the Library; focus returns to Play | NOT PERFORMED |
| 4.5 | With the failure open, click Favorite with the mouse, then press `A` and `X`: neither may activate it | NOT PERFORMED |

**5. RetroArch ownership** (needs a legally owned local ROM)

| # | Step | Verdict |
| --- | --- | --- |
| 5.1 | Focus Play and launch | NOT PERFORMED |
| 5.2 | **While the launch is still pending**, press D-pad and `A`: RetroFrontier navigation must stop *immediately*, with no single extra step | NOT PERFORMED |
| 5.3 | RetroArch receives the controller | NOT PERFORMED |
| 5.4 | Footer says `RETROARCH HAS CONTROLLER INPUT` and offers no action hints | NOT PERFORMED |
| 5.5 | Exit RetroArch: RetroFrontier requests focus **once** (no repeated raising, no fighting KWin) | NOT PERFORMED |
| 5.6 | DOM focus is restored **exactly once**, to Play | NOT PERFORMED |
| 5.7 | Hold `A` or a direction across the exit: nothing is replayed; release and press again works immediately | NOT PERFORMED |

**6. Already-focused exit** — the HIGH-FINAL-1 case

| # | Step | Verdict |
| --- | --- | --- |
| 6.1 | Launch a game | NOT PERFORMED |
| 6.2 | While RetroArch still runs, switch back to RetroFrontier manually (Alt+Tab or click) | NOT PERFORMED |
| 6.3 | Keep RetroFrontier focused and do not touch anything else | NOT PERFORMED |
| 6.4 | Exit RetroArch | NOT PERFORMED |
| 6.5 | DOM focus is restored to Play **without** requiring another window focus change | NOT PERFORMED |
| 6.6 | Press D-pad: navigation works immediately from Play | NOT PERFORMED |

**7. Multi-content return** — the HIGH-FINAL-2 case (needs legally owned multi-content content)

| # | Step | Verdict |
| --- | --- | --- |
| 7.1 | Focus Play and launch a multi-content game | NOT PERFORMED |
| 7.2 | Choose a version from the selection surface | NOT PERFORMED |
| 7.3 | RetroArch runs; exit it | NOT PERFORMED |
| 7.4 | Focus returns to **Play**, not to a content option, and **not** after a visible ~1.2 s delay | NOT PERFORMED |
| 7.5 | Launch again and choose a version again: no stale option steals focus when the surface reappears | NOT PERFORMED |

**8. Focus visuals**

| # | Step | Verdict |
| --- | --- | --- |
| 8.1 | With the controller, A6's focus states appear (cursor column, inversion, card scale + pixel shadow) — the document carries `data-input-mode="controller"` | NOT PERFORMED |
| 8.2 | After a keyboard or pointer interaction, `data-input-mode` reverts and the same states appear from `:focus-visible` | NOT PERFORMED |
| 8.3 | No focus ring and no new accent colour was introduced anywhere | NOT PERFORMED |

## L. Remaining risks

1. **The whole interactive gate is unperformed.** Every behavioural claim about real DualSense input,
   real WebKitGTK gamepad enumeration, real KWin activation, and the real RetroArch handoff rests on
   jsdom tests and reasoning. In particular the M8 contract's hard requirement — WebKitGTK reporting
   `mapping === "standard"` for the DualSense — is **unverified**. If it reports anything else, M8's
   controller navigation does not work on this machine at all, and the footer will say
   `CONTROLLER NOT SUPPORTED`. This is the single largest open risk.
2. **A focused control that becomes disabled loses focus in a real browser.** jsdom does not blur it;
   Chrome and WebKit do. The footer is now correct either way, but the *focus* may land on `body`
   after, say, the runtime action becomes disabled mid-installation. The registered nodes that this
   matters for (Play) are covered by explicit restoration; the Settings runtime action is not, and a
   controller user may need one directional press to re-enter. Not a regression from this pass, and
   not covered by an automated test.
3. **`docs/M5_IMPLEMENTATION_REPORT.md` is untracked but not ignored.** Like the other 28 artifacts it
   has no `.gitignore` entry, so a future over-broad `git add -A` can re-track it. Adding an entry for
   one artifact and not the other 28 would have been inconsistent, so it was left alone; the risk is
   the same one that produced MEDIUM-FINAL-2.
4. **Layout-effect ownership depends on React's commit semantics.** The guarantee "no animation frame
   between commit and gate write" is a property of React's layout-effect contract. It is stable and
   documented, but it is a framework guarantee rather than something the application enforces itself.
   A future move to a scheduler that defers layout effects would silently reopen HIGH-FINAL-4; the
   ordering tests are the guard.
5. **The 1.2 s focus-request safety timer still exists** and is still the fallback when a target never
   mounts. This pass removed the paths that *routinely* relied on it, but it remains observable if a
   restoration target genuinely disappears.
6. **Windows and macOS remain unqualified**, as does controller remapping (B10) and RetroArch's own
   input configuration. Unchanged from M8's stated scope.
7. **KWin may refuse activation** from a window that is not the user's current focus under Wayland.
   That is expected and untreated as an error; DOM restoration then waits for the user, which is the
   honest behaviour but means item 5.5/5.6 above can legitimately look different on another
   compositor.

## M. Final verdict

The automated gate is complete and green, every HIGH-FINAL finding was reproduced with a failing test
before being fixed at its root cause, the two non-reproducible sub-findings are documented as such
rather than papered over, and the repository and report hygiene findings are closed. The interactive
Linux/DualSense gate is **not** performed and cannot be self-certified from this session.

`M8 FINAL CORRECTIVE PASS — READY FOR FINAL REVIEW`

Subject to the manual qualification in § K, which remains an open, operator-owned gate.

This verdict stood for the state at `d19b7a9`. A subsequent narrow review found one further launch
lifecycle defect — transient launch state had no owning game — which is fixed and recorded in
[`docs/M8_LAUNCH_LIFECYCLE_FINAL_REPORT.md`](M8_LAUNCH_LIFECYCLE_FINAL_REPORT.md).
