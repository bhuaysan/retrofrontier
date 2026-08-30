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
| `src/hooks/useKeyboardInput.ts` | Window listener for the keyboard adapter. |
| `src/hooks/useControllerInput.ts` | Animation-frame poll loop and ownership gating. |
| `src/focus/focusNodes.ts` | Stable semantic focus identities and scope ids. |
| `src/focus/focusRegistry.ts` | Identity ↔ element, and live candidate collection. |
| `src/focus/spatialNavigation.ts` | Geometry-derived directional resolution. Pure. |
| `src/focus/focusContext.ts` | Hooks components use: node, back, scope, API. |
| `src/focus/FocusProvider.tsx` | The coordinator: dispatch, requests, scopes, input mode. |
| `src/focus/footerHints.ts` | Supported actions → footer hints. Pure. |
| `src/components/ui/ControllerFooter.tsx` | Renders the derived hints and shell status. |
| `src/platform/appWindow.ts` | The Tauri application-window boundary. |
| `src/hooks/useAppWindowFocus.ts` | Whether the application window owns focus. |
| `src/hooks/useLaunchFocusReturn.ts` | Window and DOM focus return after a managed game exits. |

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
| `Escape` | `back` |
| `ContextMenu`, `Shift+F10` | `context` |
| `Enter`, `Space` | `confirm`, only when the target has no native activation |

The adapter declines more often than it accepts, because the platform already does the right thing
in most cases:

- an event whose default was already prevented is left alone, so the existing element-level `Escape`
  cancellations in Settings act once rather than twice;
- `Ctrl`/`Alt`/`Meta` chords belong to the browser and the window manager;
- `Tab` and `Shift+Tab` are never handled, so the native tab order is untouched;
- inside a text-editing control — `input` (except button-like types), `textarea`, `select`,
  `contenteditable`, `role="textbox"` — movement, `confirm`, and `context` are all suppressed, so
  typing, caret movement, and native select behaviour are never hijacked. `back` stays available so
  a scope can be dismissed from inside a field;
- `Enter`/`Space` on a `button`, `a[href]`, `input`, `select`, `textarea`, or `summary` produce no
  semantic action, because the browser already activates those. Nothing is ever activated twice.

The listener is attached to `window` in the bubble phase, after React's own handlers.

### Controller

Read from the browser Gamepad API on `requestAnimationFrame`, using the W3C Standard Gamepad
mapping:

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
| `initialRepeatDelayMs` | `400` | Pause before a held direction begins repeating. |
| `repeatIntervalMs` | `110` | Bounded interval while it stays held. |

- **Hysteresis.** The exit threshold is well below the enter threshold, so a stick resting near one
  threshold cannot oscillate in and out of a direction. Jitter inside the band changes nothing.
- **Dominant axis.** Only the larger of `|x|` and `|y|` produces a direction. A diagonal therefore
  yields exactly one direction, never two, and an exactly equal deflection yields none rather than
  an arbitrary winner.
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
| `settings:root:<rootId>:<action>` | A per-root Settings action |

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
  again.

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

A temporary surface owns focus while it is mounted. `useFocusScope` returns a container ref; while
the container is attached, it is the root of candidate collection, so directional movement cannot
leave it, and the scope's own dismiss handler answers `back`.

| Scope | Entry / exit focus | `back` |
| --- | --- | --- |
| Launch content selection | Managed by the scope; exit restores `detail:play` | Cancels the selection |
| Settings root removal | Left to Settings' existing behaviour | Cancels the removal |
| Settings metadata-account clear | Left to Settings' existing behaviour | Cancels the confirmation |

The Settings confirmations already had correct, tested entry and exit focus behaviour — confirmation
receives focus, cancel returns to the trigger, a removed trigger falls back to the roots heading. M8
adds containment and `back` there and deliberately leaves that behaviour alone rather than rewriting
a working screen. Their existing `Escape` handlers still run first and consume the event.

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

RetroFrontier owns UI input only while

```
window is focused  ∧  launch.running === null  ∧  !launch.blocked
```

The backend remains the only authority on whether a game is running. React never infers an exit from
a timer, never inspects processes, and the M7 launch contract is unchanged.

**While a game runs, or while launch state is blocked or uncertain:** the controller dispatcher
stops delivering semantic actions and its held state is released. The poll loop keeps running so the
footer can still say whether a controller is attached, but nothing reaches the UI. RetroFrontier does
not raise its window, does not request focus, and uses no `xdotool`, `wmctrl`, or compositor
scripting. Keyboard behaviour follows OS window focus naturally.

**When the backend reports the managed game really ended:** the launch origin — the semantic
identity focused when the launch started, captured once and deliberately not updated during the run
— is restored in two steps.

1. `requestAppWindowFocus()` is called **exactly once** per ended session, through the Tauri window
   API. There is no retry, and a window manager that refuses is not fought.
2. DOM focus is restored **only after** the application window actually reports focus. If the window
   never comes forward, no DOM focus is stolen into an invisible window.

While `blocked` is true the last known session is held rather than consumed, so a return still
happens once the backend can describe the state honestly again.

Reading the window's focus state is covered by `core:default`; setting focus is not. The capability
gained exactly one permission, `core:window:allow-set-focus`, and nothing else was broadened.

## Controller footer

Hints are derived from the focus model, never hard-coded per page: the focused node's declared
`confirm`/`context` labels and the active scope's `back` label. A node that declares no `context`
action produces no `X` hint. An unregistered but natively activatable control produces a generic
`CONFIRM`. While RetroFrontier does not own input, no action hint is shown at all, and while a
managed game runs the footer says so instead. The existing shell status — `LOCAL LIBRARY` and the
scan state — is unchanged, and controller connection state is shown where the static note was.

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
