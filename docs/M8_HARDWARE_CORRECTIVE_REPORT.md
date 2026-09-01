# M8 hardware-qualification corrective pass

One corrective pass for the two findings that came out of **real operator qualification** on
Fedora/KDE Plasma Wayland with a physical DualSense. It is not an M8 redesign: the automated M8
review, the launch interaction lifecycle, and the M7/M7.5 process authority are all unchanged.

## A. Starting state

| Ref | SHA |
| --- | --- |
| Branch | `feat/m8-controller-focus` |
| Local `HEAD` | `85706d5c4b2dca31d4a2b1db5ce5fd219d410a84` |
| `origin/feat/m8-controller-focus` | `85706d5c4b2dca31d4a2b1db5ce5fd219d410a84` |
| `main` | `77f5194c76c360bd6eb14e8546a7a4e0998be1aa` |
| `origin/main` | `77f5194c76c360bd6eb14e8546a7a4e0998be1aa` |

The 29 pre-existing untracked local review artifacts were preserved, and
`git ls-files docs/M5_IMPLEMENTATION_REPORT.md` remained empty throughout.

## B. Real hardware observations

Reported by the operator from the physical qualification session:

```text
DualSense detected by WebKitGTK:                PASS
navigator.getGamepads(): controller present:    PASS
Gamepad.mapping === "standard":                 PASS
Basic controller navigation:                    PASS
Real managed RetroArch game launch:             PASS
```

The two findings this pass addresses:

```text
Library sidebar/main navigation stability:      CHANGE REQUIRED
RetroArch fullscreen launch:                    FAIL
```

Everything in this section is the operator's report. Nothing in it was produced by automation.

## C. Sidebar/main root cause

Directional movement was resolved by `spatialNavigation.findNextNode` over **one candidate field**
covering the whole active scope, which on the Library route is the document body. The sidebar rows
and the game cards were therefore peers in the same geometric field, and the algorithm answered the
only question it can answer: *which candidate lies nearest in the pressed direction?*

Concretely, with the sidebar column on the left and the card grid to its right:

- `moveRight` from a sidebar row found no sidebar candidate ahead, so the row-overlap preference fell
  through to the nearest candidate ahead — a game card — and took it;
- `moveLeft` from the leftmost card did the mirror image and took a sidebar row;
- `moveDown` from the last sidebar entry, whenever a card rendered below its baseline, left the
  sidebar downwards.

Nothing was faulty in the geometry. The model simply had **no notion of a region**, so every
sidebar/main crossing was a side effect of layout rather than an expression of intent, and on real
hardware — where the D-pad is used continuously rather than one press at a time — that reads as
unstable and accidental. This was a missing semantic concept, not a tolerance that needed tuning:
adding a coordinate threshold would have encoded one particular layout into the input model and
would still have been geometry answering a question about intent.

### Reproduction before the fix

`AppShell.test.tsx` → "A1: keeps directional movement in the sidebar instead of jumping to a game
card", run against `85706d5`:

```text
focus  <button class="pixel-row pixel-row--active" data-test-rect="20,200,240,240">All systems</button>
press  D-pad Right

Expected element with focus:
  <button class="pixel-row pixel-row--active" data-test-rect="20,200,240,240"> … All systems …
Received element with focus:
  <a aria-label="Open Kirby’s Adventure details" data-test-rect="300,200,460,420" href="/games/1">
```

Focus crossed from the sidebar into the game grid on a single `Right`, purely because a card's
rectangle lay to the right of the row's.

## D. Final Library controller-navigation contract

The Library declares two explicit **focus zones**. A zone is a permanent region of one screen whose
membership is decided by **DOM containment in a declared container** — a semantic question, with no
coordinate threshold and no geometry tolerance anywhere in it. The existing geometry algorithm keeps
resolving movement normally *within* whichever region it is given.

| Zone | Container |
| --- | --- |
| `zone:library-sidebar` | the Library route's `<aside>` |
| `zone:library-main` | the Library route's `<main>` |

| Rule | Behaviour |
| --- | --- |
| **sidebar Up/Down** | Traverses sidebar entries only. An edge is a stop, never a doorway into the grid. |
| **sidebar Right** | Does **not** reach a game card. No geometry-based crossing exists any more. |
| **Confirm enters main** | `confirm` on a system-filter row applies the filter normally, then moves focus to the first game of the **committed** result for that view, or to `library:heading` when the view has none. No focusable element is invented. |
| **main directional containment** | The filter bar, the grid, and pagination navigate by rendered geometry exactly as before, and movement never falls back into the sidebar. |
| **Back returns sidebar** | `back` from the main area returns focus to the sidebar entry that is actually selected, falling back to the all-systems row. It is a **focus transition only** — the Library is the root route and is never navigated away from. Up/Down then move through sidebar entries again, and Confirm may re-enter. |
| **pointer/Tab unaffected** | Nothing is browser-modal and no key is trapped. `Tab`, `Shift+Tab`, pointer clicks, search typing, and assistive-technology focus are untouched; `focusin` still adopts whatever the platform focused. |

Two details are deliberate:

- **The handoff is settle-aware.** Confirming a *different* filter starts a bounded query while the
  previous result is still rendered, so entering at "the first card" immediately would focus a game
  belonging to the filter the user just left — a card that may unmount in the next commit. The
  handoff waits for a newly committed result with no load channel active and only then names its
  target. Confirming the filter that is *already* active starts no query, so it resolves at once. If
  a query never resolves, nothing is focused and the user stays on the sidebar row; no focus is
  stolen into an uncertain view.
- **The semantic path and the pointer path are separate.** A filter row's `confirm` applies the
  filter *and* hands focus on; its `onClick` applies the filter and nothing else. A mouse user who
  clicks a filter keeps their focus where it was, and can still click a card directly.

Only the Library declares zones. Game Detail and Settings keep the reviewed M8 behaviour, and the
launch scopes on Game Detail keep owning `back` unconditionally — `activeZone()` stands aside
whenever a temporary scope is open, so a scope remains the stronger claim.

**One consequence worth naming:** while focus sits inside a zone, movement stays in that zone, so the
shared header's search field and theme toggle are reached by pointer or `Tab` rather than by a D-pad
direction. While focus sits in the header — which is in no zone — movement behaves exactly as before
and can enter either region. See §J.

## E. Fullscreen root cause

The generated `retroarch.cfg` set **no** video-presentation key at all. The live config from the
operator's own qualification session confirms it:

```console
$ grep -c fullscreen ~/.local/share/com.retrofrontier.desktop/runtime-user/config/retroarch.cfg
0
```

RetroArch therefore fell back to its **compiled-in** default. In RetroArch 1.22.2's `config.def.h`,
`DEFAULT_FULLSCREEN` is `true` only for Steam, Dingux, WinRT, and Winapi-Family builds:

```c
#if defined(HAVE_STEAM) || defined(DINGUX) || defined(__WINRT__) || defined(WINAPI_FAMILY) && WINAPI_FAMILY == WINAPI_FAMILY_PHONE_APP
#define DEFAULT_FULLSCREEN true
#else
#define DEFAULT_FULLSCREEN false
#endif
```

The managed runtime is the official generic Linux x86_64 build, so the effective default was
`video_fullscreen = false` and RetroArch opened its ordinary default-sized window — the "very small
window" the operator observed. This was never host or user configuration leaking in: `--config`
names RetroFrontier's file and `XDG_CONFIG_HOME` is redirected into managed data. It was
RetroFrontier declining to state an opinion and inheriting the build's.

## F. Fullscreen implementation

Two keys were added to the RetroFrontier-owned generated configuration:

```text
video_fullscreen = "true"
video_windowed_fullscreen = "true"
```

| Key | Why |
| --- | --- |
| `video_fullscreen` | The setting RetroArch itself reads to start fullscreen. Owning it replaces the inherited build default. |
| `video_windowed_fullscreen` | *How* fullscreen is entered: borderless fullscreen at the current desktop resolution rather than an exclusive video-mode change. A Wayland client cannot set a video mode at all, and this path never shows an intermediate window. Its build default happens to be `true` on Linux, but inheriting a compiled-in default is exactly what caused this finding, so it is stated explicitly and pinned by a test. |

`video_fullscreen_x` / `video_fullscreen_y` apply only to the exclusive path and are deliberately
**not** written.

### Why the generated config, and not `--fullscreen`

RetroArch also accepts a launch flag, and the managed binary's own help states its purpose:

```console
$ AppRun --help
  -f, --fullscreen               Start the program in fullscreen regardless of config setting.
```

Its documented job is to override *a config setting*. RetroFrontier owns that setting, so the flag
would be a second authority on the same question with nothing to override — and two control paths
that can disagree is precisely the class of problem this architecture avoids elsewhere. The
generated configuration is therefore the **single canonical control path**, the launch argument
contract is unchanged, and a test pins it.

Nothing resizes, raises, polls for, or scripts a window. No `xdotool`, no `wmctrl`, no KWin or
compositor API, no post-spawn manipulation, no host config inheritance, and no write into the
immutable managed runtime tree. RetroArch is instructed to start fullscreen and does it itself.

### Evidence it is supported by the managed RetroArch version

All of this was verified against the *actual installed managed runtime*, not from memory:

```console
$ RA=~/.local/share/com.retrofrontier.desktop/runtime/versions/i-18d0a4fda8be7c01-1-293535/runtime/retroarch
$ $RA/AppRun --version
RetroArch - Frontend for libretro
Version: 1.22.2 (Git 69a4f0e) Nov 20 2025

$ strings -a $RA/usr/bin/retroarch | grep -x 'video_fullscreen\|video_windowed_fullscreen'
video_fullscreen
video_windowed_fullscreen
```

and against the matching upstream source tag:

```console
$ curl -sSL https://raw.githubusercontent.com/libretro/RetroArch/v1.22.2/configuration.c \
    | grep -n '"video_fullscreen"\|"video_windowed_fullscreen"'
1893:   SETTING_BOOL("video_windowed_fullscreen",     &settings->bools.video_windowed_fullscreen, true, DEFAULT_WINDOWED_FULLSCREEN, false);
1910:   SETTING_BOOL("video_fullscreen",              &settings->bools.video_fullscreen, true, DEFAULT_FULLSCREEN, false);
```

Both are boolean settings read from the configuration file in exactly the qualified version. Neither
name was guessed.

## G. Regression tests

### Library navigation (`src/app/AppShell.test.tsx`)

Written against `85706d5` before any implementation change.

| Test | Before | After |
| --- | --- | --- |
| A1 — sidebar directional containment (`Right` does not reach a card) | **FAIL** | PASS |
| A2 — sidebar vertical navigation traverses only sidebar entries, with the grid laid out *below* the column so a geometric resolution would leave it | **FAIL** | PASS |
| A3 — `confirm` applies the filter and transfers focus to the first main-content target | **FAIL** | PASS |
| A4 — main-area containment, including `Left` at the sidebar boundary and `Up` | **FAIL** | PASS |
| A5 — `back` returns to the active sidebar filter, no route navigation | **FAIL** | PASS |
| A5 — `back` returns to the all-systems entry when no system filter is active | **FAIL** | PASS |
| A6 — pointer click, native `Tab`/`Shift+Tab`, direct card opening | PASS | PASS |
| A6 — search field stays editable; caret keys and `Escape` are not consumed | PASS | PASS |
| A7 — a filtered-empty view lands on the honest Library heading, not on nothing | **FAIL** | PASS |

Failing before: **7 of 9**. The two A6 tests passed before and after by design — they are protective
regressions proving the corrective pass did not reach into native behaviour.

The pre-existing test `moves Library focus across the rendered grid with the D-pad` asserted the old
crossing (`Right` from the sidebar lands on a card). It was rewritten to the new contract: the first
directional action with nothing focused still enters the first navigable node, the main area is now
entered with `confirm`, and grid geometry still resolves movement inside it.

### RetroArch fullscreen (`src-tauri/src/services/`)

| Test | Before | After |
| --- | --- | --- |
| B1 — `the_generated_configuration_requests_fullscreen_explicitly` | **FAIL** (`video_fullscreen` was `None`) | PASS |
| B2 — `repeated_generation_renders_identical_fullscreen_entries` | **FAIL** (rendered `[]`) | PASS |
| B3 — `the_fullscreen_request_is_independent_of_any_host_or_user_retroarch_state` | **FAIL** | PASS |
| B4 — `the_prepared_launch_uses_absolute_managed_paths_and_no_path_lookup`, extended to pin the exact flag set and assert the written file carries the fullscreen request | PASS (extended) | PASS |

Verbatim pre-fix failure:

```text
assertion `left == right` failed: video_fullscreen
  left: None
 right: Some("true")
```

All pre-existing RetroArch safety tests were kept and still pass unchanged: controlled paths,
config-save disabled, forbidden host RetroArch paths, deterministic quoted rendering, atomic
`0600` write, and launch-argument ownership.

## H. Automated verification

Frontend:

| Command | Result |
| --- | --- |
| `pnpm typecheck` | PASS |
| `pnpm lint` | PASS |
| `pnpm format:check` | PASS |
| `pnpm test` | PASS — **36 files, 579 tests** |
| `pnpm build` | PASS — built in 172 ms |

Rust (`src-tauri/`):

| Command | Result |
| --- | --- |
| `cargo fmt -- --check` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo test` | PASS — **414 passed, 1 ignored** |
| `cargo build --release` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --all-features` | PASS — **434 passed, 7 ignored** |

Repository:

| Command | Result |
| --- | --- |
| `git diff --check` | clean |
| `git status --short` | only the intended changes plus the 29 pre-existing untracked artifacts |
| `git ls-files docs/M5_IMPLEMENTATION_REPORT.md` | empty — still untracked |

Flakiness: the focus/controller/library suites (18 files, 408 tests) were run **5×** — 408 passed
every time. The zone block alone was run **3×** and the RetroArch Rust tests **3×**, all identical.

## I. Manual requalification checklist

### Library

1. Focus the sidebar.
2. Up/Down stays in the sidebar.
3. Right does not jump into the Library.
4. Confirm a sidebar item.
5. Focus enters the Library main area.
6. Navigate around the games.
7. Left near the sidebar does not jump back.
8. B returns to the active sidebar item.
9. Pointer and Tab still work normally.

### RetroArch

1. Launch a legal test game through RetroFrontier.
2. RetroArch opens fullscreen immediately.
3. No tiny intermediate/default gameplay window remains.
4. The controller is owned by RetroArch while running.
5. Exit RetroArch.
6. RetroFrontier returns to the foreground and restores focus exactly as the existing M8 contract
   specifies.
7. Repeat the already-focused exit case.
8. Repeat the multi-content launch/return case.

**Status of every item above: `NOT PERFORMED — HUMAN INTERACTION REQUIRED`.**

No manual result is recorded here. Both findings came from physical hardware behaviour that no
automated suite in this repository can observe: jsdom performs no layout (the navigation tests
supply explicit rectangles), and nothing in CI maps a real RetroArch window onto a real compositor.

## J. Remaining risks

- **Neither fix is proven on hardware yet.** Automated tests prove the contract and the generated
  file; they cannot prove how a DualSense feels in the hand or how KWin maps a fullscreen surface.
- **Header reachability by controller changed on the Library route.** While focus is inside a zone,
  directional movement stays in that zone, so the shared header's search field and theme toggle are
  no longer reached by a D-pad direction from the grid or the sidebar. They remain reachable by
  pointer and `Tab`, and from the header (which is in no zone) movement can still enter either
  region. This is a direct consequence of the requested containment contract; if the operator wants
  the search field back on the controller path, the honest options are an explicit third zone or a
  named transition, not a geometry tolerance.
- **Wayland activation is still the compositor's call.** M7.5's observation stands: RetroFrontier is
  neither raised nor lowered by a launch, and the single polite `setFocus()` on exit may be refused.
  Fullscreen presentation does not change that, and the M8 return contract already treats a refusal
  as expected rather than as an error.
- **`video_windowed_fullscreen = true` is a product decision**, not the only possible one. It is the
  Wayland-safe choice and avoids any mode switch; a future requirement for exclusive fullscreen with
  a specific resolution would set it `false` and then own `video_fullscreen_x/y` as well.
- **Multi-monitor placement is not specified.** Which display RetroArch fills is decided by
  `video_monitor_index` (unset, so RetroArch's default) and by the compositor. Real qualification
  should note which display the game appeared on.
- **The settle-aware handoff depends on the Library query resolving.** If a query never settles, the
  entry never happens and focus stays on the sidebar row. That is deliberate — it steals no focus
  into an uncertain view — but it is a silent non-event rather than a visible message.
- **Only Linux x86_64 is in scope.** Windows and macOS zone behaviour and fullscreen semantics are
  unqualified.

## K. Verdict

`M8 HARDWARE CORRECTIVE PASS — READY FOR OPERATOR REQUALIFICATION`

The two findings are implemented, contract-tested, and documented, and the full frontend and Rust
verification suites pass. Neither hardware issue may be called solved on that basis: the operator
must confirm the new navigation feel and the fullscreen launch on the real
DualSense/WebKitGTK/RetroArch environment using §I.
