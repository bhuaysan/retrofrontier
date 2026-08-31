# Controller and focus (M8)

RetroFrontier's primary UI is navigable with a controller, a keyboard, a pointer, and assistive
technology at the same time. This document is the implementation contract for that: what the layers
are, which policies they enforce, and where each responsibility ends.

Design authority for what focus *looks like* is `docs/design/screens/A6 Fokus-Zustandsblatt.dc.html`.
Decision authority for acquiring controller input is [ADR-014](adr/ADR-014-input-acquisition-boundary.md);
ADR-008 remains the decision that controller navigation is foundational at all.

## Layers

```text
Gamepad API poll ──┐
                   ├─► InputAction ──► focus coordinator ──► DOM focus / element activation
window keydown ────┘                        │
                                            ├─► focus registry (semantic identity ↔ element)
                                            ├─► spatial navigation (rendered geometry)
                                            ├─► focus scopes (temporary surfaces)
                                            └─► supported actions ──► controller footer
```

| Module | Responsibility |
| --- | --- |
| `src/input/actions.ts` | The semantic vocabulary. |
| `src/input/keyboardAdapter.ts` | Keyboard event → action. Pure. |
| `src/input/gamepadAdapter.ts` | Polled gamepad frame → actions. Pure state machine. |
| `src/input/inputOwnership.ts` | The one application-input ownership predicate. |
| `src/hooks/useKeyboardInput.ts` | Window listener for the keyboard adapter. |
| `src/hooks/useControllerInput.ts` | Animation-frame poll loop and ownership gating. |
| `src/focus/focusNodes.ts` | Stable semantic focus identities and scope ids. |
| `src/focus/focusRegistry.ts` | Identity ↔ element, and live candidate collection. |
| `src/focus/focusability.ts` | The one focusability test: programmatic, navigable, and proven. |
| `src/focus/spatialNavigation.ts` | Geometry-derived directional resolution. Pure. |
| `src/focus/focusContext.ts` | Hooks components use: node, back, scope, API. |
| `src/focus/FocusProvider.tsx` | The coordinator: dispatch, requests, scopes, input mode. |
| `src/focus/footerHints.ts` | Supported actions → footer hints. Pure. |
| `src/components/ui/ControllerFooter.tsx` | Renders the derived hints and shell status. |
| `src/platform/appWindow.ts` | The Tauri application-window boundary. |
| `src/hooks/useAppWindowFocus.ts` | Whether the application window owns focus. |
| `src/hooks/useLaunchFocusReturn.ts` | Launch interaction lifetime, and window plus DOM focus return after a managed game exits. |

## Semantic actions

```ts
type InputAction =
  | 'moveUp' | 'moveDown' | 'moveLeft' | 'moveRight'
  | 'confirm' | 'back' | 'context';
```

No component, hook, or focus module refers to a key name or a gamepad button index. Physical
mappings exist in exactly two places: `keyboardAdapter.ts` and `GAMEPAD_BUTTON_INDEX` in
`gamepadAdapter.ts`.

## Acquisition adapters

### Keyboard

| Input | Action |
| --- | --- |
| `ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight` | `moveUp` / `moveDown` / `moveLeft` / `moveRight` |
| `Escape` | `back`, only outside a text-editing control |
| `ContextMenu`, `Shift+F10` | `context` |
| `Enter`, `Space` | `confirm`, only when the target has no native activation |

The adapter declines more often than it accepts, because the platform already does the right thing
in most cases:

- an event whose default was already prevented is left alone, so the existing element-level `Escape`
  cancellations in Settings act once rather than twice;
- `Ctrl`/`Alt`/`Meta` chords belong to the browser and the window manager;
- `Tab` and `Shift+Tab` are never handled, so the native tab order is untouched;
- inside a text-editing control — `input` (except button-like types), `textarea`, `select`,
  `contenteditable`, `role="textbox"` — **every** mapped key is suppressed, `Escape` included, so
  typing, caret movement, native select behaviour, and a field's own `Escape` all stay with the
  platform. Turning `Escape` into a page-level `back` there was wrong twice over: it navigated away
  from the Settings credential fields, and it suppressed the Library search field's native behaviour
  on a route that has no semantic Back at all. A scope that wants `Escape` from inside a field
  handles it locally and consumes the event — which is exactly what both Settings confirmations
  already do, and the adapter honours that through `defaultPrevented`;
- `Enter`/`Space` on a `button`, `a[href]`, `input`, `select`, `textarea`, or `summary` produce no
  semantic action, because the browser already activates those. Nothing is ever activated twice.

The listener is attached to `window` in the bubble phase, after React's own handlers.

### Controller

Read from the browser Gamepad API on `requestAnimationFrame`, using the W3C Standard Gamepad
mapping.

**Only `mapping === 'standard'` is accepted.** These indices mean nothing on a pad the browser could
not normalize, so reading them would produce arbitrary actions from arbitrary buttons. A pad with any
other mapping is never selected, is never interpreted even if a caller passes it in, and cannot block
a usable Standard-mapped pad at a higher index. It is reported honestly as connected-but-unsupported
— the footer says `CONTROLLER NOT SUPPORTED` and the document carries
`data-controller="unsupported"` — rather than being shown as a working controller or hidden
entirely. Remapping is out of M8 scope (B10); see [ADR-014](adr/ADR-014-input-acquisition-boundary.md).

| Physical | Action |
| --- | --- |
| Button 0 (A / cross) | `confirm` |
| Button 1 (B / circle) | `back` |
| Button 2 (X / square) | `context` |
| Buttons 12–15 (D-pad) | `moveUp` / `moveDown` / `moveLeft` / `moveRight` |
| Axes 0/1 (left stick) | directional, see below |

`GAMEPAD_TUNING` is the whole analogue and repeat policy:

| Setting | Value | Why |
| --- | --- | --- |
| `enterDeadzone` | `0.55` | Deflection required to *start* a direction. |
| `exitDeadzone` | `0.35` | Deflection below which a held direction is released. |
| `axisDominanceMargin` | `0.15` | How far the other axis must exceed the held axis to take over. |
| `initialRepeatDelayMs` | `400` | Pause before a held direction begins repeating. |
| `repeatIntervalMs` | `110` | Bounded interval while it stays held. |

- **Hysteresis.** The exit threshold is well below the enter threshold, so a stick resting near one
  threshold cannot oscillate in and out of a direction. Jitter inside the band changes nothing.
- **Dominant axis, with a deterministic tie-break.** Exactly one direction is produced, never two.
  While a direction is held, that axis keeps dominance until the other axis exceeds it by
  `axisDominanceMargin`; with nothing held the larger deflection wins, and an exact tie resolves to
  the **horizontal** axis, the documented fixed priority. The margin is the dominance counterpart to
  the deadzone hysteresis: it gives the exact 45° case an answer instead of a dead spot, and it stops
  a stick resting near the diagonal from alternating direction every time `|x|` and `|y|` cross.
- **D-pad precedence.** A pressed D-pad direction always wins over the stick, so holding both is
  deterministic.
- **Repeat is UI-paced, not frame-paced.** One action on press, then a pause, then a bounded
  interval, and never more than one directional action per polled frame — a stalled frame cannot
  emit a burst. Changing direction and releasing both reset the repeat state.
- **Activation buttons are edge-triggered.** `confirm`, `back`, and `context` fire once per press,
  never once per frame while held.

**Controller selection** is deterministic: the active controller keeps ownership while it stays
connected, so plugging in a second pad neither moves control nor duplicates actions; otherwise the
lowest connected index wins. Nothing is ever read from two pads at once.

**Ownership changes release and re-adopt.** On disconnect, on replacement of the active controller,
and whenever RetroFrontier stops or starts owning input, held and repeat state is dropped and the
next observation *adopts* whatever is physically held without emitting. A button or direction held
across the change therefore produces nothing until it is released and pressed again. Adoption
happens at the moment ownership changes, not on the next frame, so a genuine press immediately after
focus returns is still delivered.

## Focus architecture

### Identity

Focus restoration keys off stable semantic identities, never DOM selectors and never timeouts:

| Identity | Covers |
| --- | --- |
| `library:game:<GameId>` | A Library game's Game Detail target |
| `library:heading` | The Library heading, the deterministic Library fallback |
| `sidebar:system:<id\|all>` | A sidebar system filter |
| `sidebar:route:<route>` | A sidebar menu destination |
| `detail:<action>` | Game Detail actions: back, play, favorite, metadata, cancel |
| `detail:candidate:<providerGameId>` | A metadata candidate |
| `launch:content:<ContentUnitId>` | A launch content choice |
| `detail:dismiss-launch-failure` | The launch-failure surface's dismiss action |
| `settings:runtime:action` | The managed-runtime install/repair action |
| `settings:root:<rootId>:<action>` | Reserved for per-root Settings actions (not yet used) |

A component declares its identity with `useFocusNode`, which returns a callback ref. The registry
holds identity ↔ element and a live getter for the node's action metadata, so a label can follow
component state without re-registering.

### Navigation

Directional movement is resolved from **rendered geometry**, not from an index or a column count,
because the Library grid is responsive and no fixed column count exists.

Candidates are collected from the DOM at the moment a directional action is dispatched — never
cached — so a re-queried page, a pagination change, or an unmounted card cannot leave navigation
pointing at a detached node. The candidate set is every focusable element inside the active scope
that is not disabled, not `aria-hidden`, not inside an `inert` subtree, not `tabindex="-1"`, and has
a non-zero rect. Elements that exist only as programmatic focus targets — headings carrying
`tabindex="-1"` — are therefore reachable by a focus request but never by movement, though a focused
heading is still a valid origin to move away from.

Resolution: candidates strictly ahead in the requested direction are scored by
`primaryDistance + 3 × crossAxisDistance`, with candidates that overlap the current row (horizontal)
or column (vertical) preferred over any that do not. Left and right therefore stay on the visual
row, up and down stay in the column, and both fall back to the nearest candidate ahead when the row
or column ends — which is how movement leaves the grid for the sidebar. **There is no wrapping:** an
edge is a stop. Ties resolve by document order, so movement is reproducible.

Registration is not required for reachability. Explicitly registered nodes carry a semantic identity
and honest action labels; every other visible, enabled control in the scope is still reachable,
because a control a user can see and click must also be reachable with a controller.

### Focus requests

`requestFocus(target, { fallback, awaitSettle, resolveOnRegister })` is the only way focus is moved
programmatically across a route or data change.

- With `resolveOnRegister`, the request resolves the moment the target registers — used when the
  target is about to re-mount, such as restoring the Play action after the content-selection surface
  closes.
- With `awaitSettle`, the request is held even if the target is already present, until the owning
  surface calls `settleFocusRequest()`. This is what the Library return uses: a page still rendered
  from the previous query result must not take a focus it is about to lose.
- A single bounded timer (1.2 s) applies the fallback if neither happens. It is a safety net, not a
  retry loop: there is no polling, no repeated `setTimeout`, and a resolved request never fires
  again. **The timer may never resolve an `awaitSettle` target.** That request said explicitly that
  its target may not be trusted until the surface settles, so an expired one takes the deterministic
  fallback and nothing else; focusing the stale target would be the exact bug the flag exists to
  prevent. A later `settleFocusRequest()` then finds nothing pending and steals no focus.
- A target that is *present but cannot take focus* — disabled, hidden, inert — is a definitive
  answer, not something to wait for: the fallback is used immediately rather than after the timeout.

**A restoration counts as successful only when focus really moved.** `focusability.ts` is the single
test, and it separates two questions: a focus *request* may target a programmatic-only node such as a
heading with `tabindex="-1"`, while directional *movement* may not. `focusMoved()` performs the
attempt and then reads `document.activeElement`, because only the browser knows whether an element
accepted focus. A request is never consumed by an attempt that silently did nothing.

### Library → Game Detail → Library

1. Opening a game records its `GameId` in the shell.
2. Returning to the Library issues `requestFocus(library:game:<GameId>, { awaitSettle, fallback: library:heading })`.
3. `useLibraryQuery` exposes `resultVersion`, incremented once per committed query outcome. The
   Library settles the request only after a *new* result has committed and no load channel is
   active.
4. If the card is present, it takes focus. If the game disappeared, or no longer belongs to the
   current search, filter, or page, focus falls back to the Library heading.

Nothing is focused speculatively, no detached node is ever focused, and once the request resolves it
does not fire again — later focus changes by the user are not overridden.

### Scopes

A temporary surface owns focus **and controller actions** while it is mounted. `useFocusScope`
returns a container ref; while the container is attached, it is the root of candidate collection, so
directional movement cannot leave it, and the scope's own dismiss handler answers `back`.

Focus itself can still leave a scope through `Tab`, `Shift+Tab`, or a pointer click — these surfaces
are deliberately not browser-modal, and trapping `Tab` would break ordinary accessibility. What is
forbidden is *acting* out there:

> **A controller can never activate an underlying surface while a temporary scope is active.**

`confirm` and `context` are refused whenever `document.activeElement` is outside the active scope,
and the footer stops offering them, because a hint that names an action which would be refused is a
lie. Nothing is trapped and nothing is force-focused: the next directional action simply re-enters
the scope at its first candidate. `back` still reaches the innermost scope's dismiss handler
regardless of where focus sits.

**Scope restoration is resolved after the commit that closed the scope.** React detaches a deleted
subtree's refs *before* it applies that same commit's sibling updates, so a restoration performed
inside the cleanup sees the pre-update DOM. The generic `restoreTo`/`restoreFallback` mechanism
therefore runs once the commit has settled, still as a single attempt, and stands aside if focus
already landed somewhere real or if another owner (a route change, say) already has a request
outstanding.

**Both launch scopes restore explicitly instead, because the generic mechanism cannot tell *why* the
surface closed.** A user action and a route unmount are different events with different honest
answers, and the generic cleanup fires for both. On a route unmount neither `detail:play` nor
`detail:back` exists any more, so the restoration became a pending request with the 1.2 s safety
timer — and the next Game Detail route to mount registered `detail:play`, satisfied that stale
request through `resolveOnRegister`, and stole focus from its own route-entry heading. Both launch
scopes therefore declare `restore: 'none'` and restore per user action:

| Event | Focus |
| --- | --- |
| Content selection **cancelled** | `detail:play`, requested before the surface closes while Play is still mounted and enabled, so it resolves at once; `detail:back` is the fallback |
| A **version confirmed** | `detail:back` — deliberately *not* Play, which the launch this click issues disables in the same commit. Back is the only Game Detail control that stays enabled through a pending launch, so it is the truthful interim target; when the launch resolves, the failure surface takes focus or the post-exit return restores Play |
| Launch failure **dismissed** | `detail:play`, requested before the surface closes; `detail:back` as the fallback |
| **Route unmount** (either scope) | Nothing. The user navigated away, so the old route must not be dragged back, and no request survives to steal focus from the next route |

| Scope | Entry / exit focus | `back` |
| --- | --- | --- |
| Launch content selection | Entry managed by the scope; **Cancel** restores `detail:play`, confirming a version moves to `detail:back`, route unmount restores nothing | Cancels the selection |
| Launch failure | Entry focuses DISMISS; dismissal restores `detail:play`, falling back to `detail:back` | Dismisses the failure |
| Settings root removal | Left to Settings' existing behaviour | Cancels the removal |
| Settings metadata-account clear | Left to Settings' existing behaviour | Cancels the confirmation |

The Settings confirmations already had correct, tested entry and exit focus behaviour — confirmation
receives focus, cancel returns to the trigger, a removed trigger falls back to the roots heading. M8
adds containment and `back` there and deliberately leaves that behaviour alone rather than rewriting
a working screen. Their existing `Escape` handlers still run first and consume the event.

### Launch failure

A normalized launch failure is a temporary surface with exactly the same requirements as the content
selection, so it gets the same treatment rather than being a bare `InlineError`. Without a scope,
focus stayed wherever the pending launch had left it — after a content-selected launch, typically
BACK TO LIBRARY, because the closing selection scope found Play disabled and took its fallback — and
controller `back` then navigated to the Library instead of dismissing a failure the user had not yet
acknowledged.

| Requirement | How |
| --- | --- |
| Stable scope id | `scope:launch-failure` |
| Stable action identity | `detail:dismiss-launch-failure` |
| Entry focus | The scope's own `initialFocus: 'auto'`, and DISMISS is the surface's first focusable |
| `confirm` | Activates DISMISS |
| `back` | The scope's dismiss handler — the same dismissal, from anywhere focus happens to sit |
| Containment | The scope container is the candidate root, so directional movement cannot leave it |
| No reach-through | `confirm`/`context` are refused while focus sits outside the scope, and the footer stops offering them |
| Restore | `detail:play`, with `detail:back` as the truthful fallback when Play cannot take focus |
| Pointer / Tab | Untouched: the surface is deliberately not browser-modal |

No Retry is invented. The M7 launch contract offers dismissal and nothing else here, so the surface
offers dismissal and nothing else.

**Restoration is explicit, not the scope's automatic restore.** A dismissal is a user action, so at
that moment the route is certainly still current and Play is certainly the honest target; the focus
request is issued before the surface unmounts, while Play is still present and enabled. An *unmount*
is a different event: if the user navigated away before dismissing, the old route must not be dragged
back, so the scope restores nothing on unmount at all (`restore: 'none'`) and leaves no request that a
later Game Detail could satisfy.

`InlineError` itself is unchanged. Every other screen that uses it has no M8 focus requirement, and
rewriting a shared primitive to serve one surface would have changed behaviour nobody asked to change.

The content-selection scope now follows the same explicit pattern; see the table above.

### Pointer, Tab, and assistive technology

The coordinator listens for `focusin`, so focus produced by a pointer click or by Tab becomes the
logical focus that the next directional action moves from. Tab order, native activation, links,
`aria-*` semantics, and live regions are untouched. The keyboard adapter never consumes `Tab`.

## Focus visuals

A6's accepted V5 language is unchanged: a cursor column for rows, foreground/background inversion
for standalone surfaces, scale plus a stronger pixel shadow for image cards. No focus ring, no new
accent token, and focus stays visually distinct from active/selected state. The M6.7 vector
exceptions for `PixelArrow` and `PixelStar` are untouched.

One condition was added. `:focus-visible` is a keyboard and pointer heuristic: the Gamepad API
produces no input events, so a browser cannot know a controller moved focus, and the accepted focus
states would simply not appear. Every A6 focus rule therefore carries a companion selector that
applies **the same declarations** while `data-input-mode="controller"` is set on the document
element. The coordinator sets that attribute when it dispatches a gamepad action and reverts it on
the next keyboard or pointer interaction. A style test asserts that every `:focus-visible` selector
has a companion and that no companion introduces an outline or a new token.

## Window focus and RetroArch ownership

`src/input/inputOwnership.ts` holds the one ownership predicate for the whole application:

```
windowFocused  ∧  !launch.blocked  ∧  launch.running === null  ∧  launch.pendingGameId === null
```

`pendingGameId` is in there deliberately. M7 creates the process and settles the launch inside the
backend, so between the launch request and the authoritative running state there is an interval in
which React still sees `running === null` while RetroArch may already exist. Ownership is therefore
released at the **launch request**, not when the running state arrives. It returns as soon as the
backend can describe the state honestly again: a failed launch and a `contentSelectionRequired`
response both clear the pending id without starting a process, so the failure message and the
content-selection surface stay immediately interactive.

There is exactly one predicate rather than a copy per consumer, because a second copy of this rule
would drift. (The Play button's own `disabled` state is a different question — whether *this control*
may be pressed — and stays where it is.)

**Window focus is not assumed.** In the Tauri desktop shell, ownership requires a native focus state
that has really been observed as focused *and* a focus subscription that was really established; an
unreadable state or a subscription that failed to attach fails **closed**, because RetroFrontier
cannot honestly claim to own the controller and, without a subscription, could never learn that it
had lost it. In a plain browser dev server there is no native window to own anything and no emulator
to take input away, so the window counts as focused and controller development stays usable. The two
are distinguished with Tauri's own `isTauri()` check, not a user-agent guess.

**The two native observations are sequenced, not raced**, because racing them can grant ownership
from an observation that was already wrong:

1. Start failed closed. Nothing is owned until something authoritative has been observed.
2. Establish the focus subscription **first**. Reading first leaves a gap in which a focus change can
   happen with no listener attached; that change is then lost forever, and the read becomes the only
   — stale — observation. Concretely: read `true`, lose focus before the listener attaches, then
   subscribe successfully, and RetroFrontier would own the controller while actually unfocused.
3. Only once the subscription is established, read the current native state. The listener is already
   attached, so nothing from that point on can go unobserved.
4. Once subscribed, **focus events are authoritative.** A read that resolves after an event arrived
   is an older observation of a state the event has already superseded, and it is discarded rather
   than allowed to overwrite it. An event counter captured before the read is the ordering evidence.

There is exactly one read, one subscription, and then events: no polling, no retry, and no fail-open
mode on the desktop side.

The backend remains the only authority on whether a game is running. React never infers an exit from
a timer, never inspects processes, and the M7 launch contract is unchanged.

**While a game runs, or while launch state is blocked or uncertain:** the controller dispatcher
stops delivering semantic actions and its held state is released. The poll loop keeps running so the
footer can still say whether a controller is attached, but nothing reaches the UI. RetroFrontier does
not raise its window, does not request focus, and uses no `xdotool`, `wmctrl`, or compositor
scripting.

**Semantic keyboard navigation is gated by the same predicate, deliberately.** `useKeyboardInput`
receives the identical `ownsApplicationInput()` value as the controller, so `Escape`, the arrow keys,
and the `Enter`/`Space` fallback produce no semantic action while a managed game runs, while launch
state is blocked or pending, or while the application window is unfocused. That is intentional for
M8 and not a leftover: there is one input-ownership boundary, and a second, more permissive rule for
keyboard would be a second copy of the ownership question that could drift from the first. Nothing
platform-level is suppressed by this — Tab order, native `Enter`/`Space` activation on real controls,
text entry, and the browser's own shortcuts are untouched, because the adapter never handled those in
the first place; what stops is only RetroFrontier's *semantic* layer. If a concrete requirement ever
asks for keyboard navigation to stay live during an emulator session, that is a deliberate ownership
split and needs its own decision, not a quiet divergence here.

**Ownership revocation is synchronous with the commit that revokes it.** The controller poll loop
reads its dispatch gate from a value written in a **layout** effect, not a passive one. Passive
effects are flushed in a separate scheduler task after the commit, which leaves an interval in which
React has already committed `ownsInput === false` while an animation frame still observes the old
`true` — one more semantic controller frame, produced from a button that already belongs to the
emulator. A layout effect runs inside the commit, before the browser can paint and therefore before
any `requestAnimationFrame` callback of the next frame, so no frame can ever see a stale gate. At the
transition, held and repeat state is dropped and whatever is physically held is *adopted* without
emitting, in both directions. `useKeyboardInput` applies its gate the same way; its interval is not
actually reachable, because React flushes pending passive effects before dispatching a discrete
event, but both adapters honour the one contract rather than relying on that scheduling detail.

**The launch origin is captured by an explicit handoff, not by sampling.**
`beginLaunchInteraction()` runs synchronously where the UI issues the launch — that is the moment of
the user's intent. Sampling whichever node happens to be focused once `running` arrives is a
different moment: focus, and even the route, can change in between, so the recorded "origin" could
belong to something the user never launched from. What is recorded is the semantic identity **and**
the logical route it belonged to.

**A multi-step launch is one interaction.** PLAY, a `contentSelectionRequired` answer, and the
version the user then confirms are the same launch attempt, so the origin is captured once at its
beginning. The shell calls `beginLaunchInteraction()` on *every* launch, and the hook decides whether
that call starts a new interaction or continues the open one — capturing again would replace the PLAY
identity with `launch:content:<ContentUnitId>`, a temporary node that does not exist when RetroArch
exits. The return would then either wait for the bounded safety fallback or, if the selection surface
remounted, hand focus to an obsolete control.

The interaction is discarded exactly when it resolves **without** a process:

| Outcome | Interaction |
| --- | --- |
| Launch request in flight (`pendingGameId` set) | held |
| `contentSelectionRequired` | held — this is a continuation, not a resolution |
| Content selection cancelled | discarded |
| Normalized launch failure, at either step | discarded; the failure scope owns focus from there |
| Transport error | discarded |
| Process started, then exited | consumed by the return |
| Launch state `blocked` | held, because nothing may be concluded while it is uncertain |

Discarding is what keeps a later, independent launch honest: it captures a fresh origin rather than
inheriting a stale one. This resolution test is deliberately not a second copy of
`ownsApplicationInput()` — it asks whether *this interaction* is over, which has nothing to do with
window focus and treats an open content selection as still in progress.

### Transient launch state has an owning game

Transient launch state — a pending launch surface, a content-option list, a normalized failure —
belongs to the Game Detail route that started it. `useGameLaunch` therefore owns an explicit
presentation identity:

```ts
interface LaunchInteraction {
  gameId: number;                                       // who started it
  phase: 'pending' | 'contentSelection' | 'failure';    // what it is presenting
}
```

It answers three questions without inference: which game owns the current transient launch
interaction, whether that interaction is still open, and whether *this* Game Detail route may render
its transient UI. It is **presentation ownership only** and is never consulted about process state —
the backend remains the sole authority on whether a game is running.

Without it, `contentOptions` and `failure` were application-global with no owner, so whichever Game
Detail route happened to be current rendered them. Because M8 deliberately does **not** browser-trap
Tab or the pointer inside a focus scope, leaving a temporary launch surface through ordinary
navigation is a valid path — and taking it, then opening another game, showed Game A's version list
(holding focus inside it) or Game A's failure on Game B. Confirming an option there would have called
`launch(GameB, GameAContentUnitId)`; Rust rejects the mismatch, so this was never an authority
failure, but the frontend state and the UI were wrong.

Two things enforce it:

- **The shell abandons the interaction** when the current route is no longer the owning game's. One
  effect covers every navigation path — pointer, Tab, browser back, semantic back, sidebar, wordmark,
  mobile nav — rather than one guard per control.
- **Game Detail receives a route-scoped view.** `contentOptions` and `failure` are masked unless the
  interaction belongs to the current game, so a screen structurally *cannot* render transient state it
  does not own, not even for the single render between a route change and the abandonment effect.
  `pendingGameId`, `running`, and `blocked` stay global, because those are facts about the
  application, not about one screen's surface.

**Route abandonment drops presentation and nothing else.** An IPC request cannot be cancelled by
deleting frontend state, so the request is still allowed to resolve; `pendingGameId` stays set and
input ownership stays released, because RetroArch may already exist. Every response is then judged on
two independent questions — is it still the current request, and does the interaction still belong to
the game that asked:

| Response after abandonment | Handling |
| --- | --- |
| `started` | **Adopted.** The user asked for that process and the backend created it; a route change must never make a real running process disappear. `pendingGameId` clears, the interaction closes, and the return lifecycle takes over. |
| `contentSelectionRequired` | `pendingGameId` clears; the option list is **discarded**, never painted on another route. |
| `failed` | `pendingGameId` clears; the failure is **discarded**. |
| transport rejection | Same as `failed`. |
| launch-state event | Always authoritative, unconditionally. |

`pendingGameId` clears on **every** resolution regardless of ownership, because the request really did
resolve and the ownership predicate depends on that fact.

**Only one frontend launch request may be unresolved at a time.** The invariant lives in
`useGameLaunch`, not only in the UI: a second request would make the first response irrelevant through
the request-generation counter, and the first request may already have created a real process. Play
says so truthfully — `ANOTHER GAME IS LAUNCHING` — instead of looking idle. A content-option
continuation is not a second request; the first has already resolved by then. Keying availability on
`pendingGameId === gameId` alone was what let the second request displace the first.

**When the backend reports the managed game really ended:**

1. `requestAppWindowFocus()` is called **exactly once** per ended session, through the Tauri window
   API. There is no retry, and a window manager that refuses is not fought.
2. DOM focus is restored **only after** the application window actually reports focus. If the window
   never comes forward, no DOM focus is stolen into an invisible window. **If the window already
   owns focus when the process ends — the user came back to RetroFrontier while the game was still
   running — the restoration happens immediately**, because the exit transition itself makes the
   pending return observable. That is why the pending return is state (a generation token) and not a
   ref: a ref mutation cannot schedule the restore, so a return recorded that way would wait for a
   focus change that is never going to arrive and stay pending for the rest of the session. The
   payload lives in a ref beside it, so consuming the return needs no second state update, and once
   consumed it cannot repeat — a later rerender, route change, or focus change finds nothing pending
   and steals nothing.
3. The target is resolved against the route that is current **at that moment**. If it is still the
   route the launch started from, the captured identity is restored. If the user navigated elsewhere
   during the run they are *not* dragged back: that route's own deterministic target is used —
   `library:heading`, `settings:heading`, or `detail:play` — and no obsolete request is left pending
   in another route to steal focus later.

While `blocked` is true the last known session is held rather than consumed, so a return still
happens once the backend can describe the state honestly again.

Reading the window's focus state is covered by `core:default`; setting focus is not. The capability
gained exactly one permission, `core:window:allow-set-focus`, and nothing else was broadened.

## Controller footer

Hints are derived from the focus model, never hard-coded per page: the focused node's declared
`confirm`/`context` labels and the active scope's `back` label. They follow the *state* behind those
actions, not only the focused identity. A `FocusProvider`-owned revision is bumped whenever that
state can have changed, and the footer subscribes to it; only the footer re-renders, and nothing else
is invalidated.

The revision is bumped when:

| Trigger | Why |
| --- | --- |
| A focused node's declared labels change | A Library card switching `SELECT` → `DESELECT`, Play losing `confirm` as it becomes disabled |
| A scope opens or closes | It owns `back`, and it changes whether `confirm`/`context` may act at all |
| A back handler appears or disappears | It changes what `B` may claim |
| The focused **element** changes | Two unregistered controls both have a `null` identity and need not support the same actions, so an identity-keyed signal alone would never re-derive |
| The focused element's own activation attributes change | A generic native control going disabled from local state, with nothing near the footer rerendering |

The last two are the floor under everything that is not explicitly registered. The attribute watch is
a single `MutationObserver` pointed at **one** element — whichever currently has focus — filtered to
the attributes the activatability test actually reads, re-pointed on each focus change and
disconnected on unmount. It is deliberately not a broad DOM observer.

**Important dynamic actions are still registered explicitly**, because registration also buys an
honest label. The Settings managed-runtime install/repair action is the case that motivated this: it
goes enabled → disabled → enabled purely from local runtime state, and as `settings:runtime:action`
the footer now names what the button really does (`INSTALL RUNTIME`, `REPAIR RUNTIME`,
`REINSTALL RUNTIME`) and stops offering it while an installation runs, instead of showing a generic
`CONFIRM` for a control that would refuse it. The policy is: **a control whose activation semantics
change while it can hold focus should declare a focus identity; anything that does not is still
covered, but only generically.**

A node that declares no `context` action produces no `X` hint. An unregistered but natively
activatable control produces a generic `CONFIRM`. While RetroFrontier does not own input, no action
hint is shown at all, and while a managed game runs the footer says so instead. The existing shell
status — `LOCAL LIBRARY` and the scan state — is unchanged, and controller connection state is shown
where the static note was.

## Known limitations

### Linux / Wayland

- M7.5 observed that RetroArch opens as a separate decorated window under KDE Plasma 6 Wayland and
  that RetroFrontier is neither raised nor lowered by a launch. M8 does not change that: it adds one
  polite `setFocus()` on exit and accepts whatever the compositor decides.
- Under Wayland a compositor may refuse activation from a window that is not the user's current
  focus. That is expected and is not treated as an error. DOM focus restoration then simply waits
  for the user to return to RetroFrontier, which is the honest behaviour.
- Whether RetroArch takes *keyboard* focus when it maps is governed by KWin's activation policy and
  is still not instrumented.

### Cross-platform

- Only Linux x86_64 is in scope for M8. Windows and macOS controller behaviour, window activation
  semantics, and focus-visible behaviour are **unqualified**.
- The WebView's device support is a dependency: a pad the engine does not expose is invisible to
  frontend navigation even though RetroArch may drive it perfectly, because RetroArch reads the
  device directly. See ADR-014.

### Out of scope

Controller remapping and its persistence (B10), RetroArch input configuration, an on-screen keyboard
for the search field (B2), and TV mode are not part of M8.
