# M8 final hardware input report

The narrow corrective pass that followed the operator's requalification of the M8 hardware
corrective work. Two findings, both from real hardware, both fixed here:

1. the controller could not reach the Library Search field at all;
2. a physical DualSense produced **no input inside the managed RetroArch session** — the M8
   acceptance blocker.

Its predecessors are [`M8_HARDWARE_CORRECTIVE_REPORT.md`](M8_HARDWARE_CORRECTIVE_REPORT.md) and
[`M8_LAUNCH_LIFECYCLE_FINAL_REPORT.md`](M8_LAUNCH_LIFECYCLE_FINAL_REPORT.md). The contracts it
changes are [`CONTROLLER_AND_FOCUS.md`](CONTROLLER_AND_FOCUS.md) and
[`RETROARCH_LAUNCH.md`](RETROARCH_LAUNCH.md).

---

## A. Starting state

| | |
| --- | --- |
| Branch | `feat/m8-controller-focus` |
| Local `HEAD` at the start of the input pass | `a16e10acb0b5835fb79ef05c6d6659748c09ba6d` |
| Local `HEAD` at the start of the release-integrity pass (section Q) | `8700a91eaab39516e43c9ffaddd37f2531ccb2a1` |
| `origin/feat/m8-controller-focus` | `a16e10acb0b5835fb79ef05c6d6659748c09ba6d` |
| `main` / `origin/main` | `77f5194c76c360bd6eb14e8546a7a4e0998be1aa` |
| Working tree | Clean except the historical untracked review artefacts, which are preserved |
| `git ls-files docs/M5_IMPLEMENTATION_REPORT.md` | Empty, before and after |

Qualification machine: Fedora 44, KDE Wayland, AMD Ryzen 7 5700X3D.
Managed runtime installation `i-18d0a4fda8be7c01-1-293535`, release
`rf-runtime-1.22.2-linux-x86_64-001`, RetroArch `1.22.2 (Git 69a4f0e, Nov 20 2025)`.

---

## B. Operator requalification results already passed

Recorded as passed by the operator on the previous corrective pass. Nothing here was re-tested by
this pass and nothing here was changed by it.

```text
Library sidebar/main zones:                    PASS
Sidebar Up/Down containment:                   PASS
Confirm enters main:                           PASS
Main-area containment:                         PASS
Back returns to sidebar:                       PASS
RetroArch fullscreen:                          PASS
RetroFrontier return after RetroArch exit:     PASS
```

The two headline results this pass must not regress:

```text
Library zones:        PASS
RetroArch fullscreen: PASS
```

---

## C. New Search UX finding

The zones are correct, and that is exactly why the gap existed. Library Search lives in the shared
top bar, which belongs to neither `zone:library-sidebar` nor `zone:library-main`, so semantic
directional movement deliberately cannot leave a zone to reach it. Pointer and `Tab` worked; a
controller alone had no route to the field.

The wrong fix would have been to let one direction leak out of a zone at one edge, which would make
the zone stop being a zone and would bring back the accidental boundary crossings the previous pass
removed. The right fix is an **explicit semantic transition**: a named action that exits the zone on
purpose, from anywhere on the screen.

---

## D. Search-button contract

```text
Standard Gamepad button 3
DualSense Triangle
Xbox-style Y
semantic action: search
```

The complete physical mapping is now:

| Button | Action |
| --- | --- |
| 0 (A / cross) | `confirm` |
| 1 (B / circle) | `back` |
| 2 (X / square) | `context` |
| 3 (Y / triangle) | `search` |
| 12–15 | D-pad directions |

Button 3 was the one remaining unmapped face button in the W3C Standard Gamepad layout. There is no
DualSense-specific branch anywhere: the adapter reads Standard Gamepad indices and refuses any pad
the browser did not normalize to that mapping, exactly as before.

**Semantic architecture.** `search` is an `InputAction` and an `ActivationAction`, so:

- the physical mapping exists only in `GAMEPAD_BUTTON_INDEX`;
- it is edge-triggered and never repeats while held, like the other three activations;
- it adopts and releases with the same ownership discipline — a button held across a loss and
  return of ownership emits nothing until it is released and pressed again;
- it is unavailable while RetroFrontier does not own application input, because the whole action
  stream is gated by the one `ownsApplicationInput` predicate. No new gate was added.

**Where the behaviour lives.** `useFocusSearch({ label, run })` in `focusContext.ts` mirrors
`useFocusBack`. Entries are scope-tagged, so an owning temporary scope that declares no search entry
simply has none: the action does nothing there and the footer says nothing about it, the same
discipline that already refuses `confirm`/`context` through an open scope. Unlike a zone `back`, a
zone never answers `search` — it is an exit, so it is the same entry wherever inside the screen focus
sits. `AppShell` registers it only under the exact condition the header renders the field under
(Library route, no scan running, a non-empty library), so an absent field means an inert button and
no `SEARCH` hint.

**Footer.** `Y SEARCH`, derived through the existing hint architecture: `SupportedActions` gained a
`search` slot, `ACTION_BUTTON_GLYPH` gained `search: 'Y'`, and the action revision is bumped when a
search entry appears or disappears. Nothing is hard-coded per page.

---

## E. Search focus-return behaviour

Taking the transition captures the semantic focus identity it came *from*, if one exists. While the
field owns focus, `back` returns to it:

1. the captured origin, if it is still focusable;
2. the selected Library sidebar entry;
3. the all-systems sidebar entry;
4. the Library heading.

It never navigates — the Library is the root route.

This is **not** a focus trap. The `back` entry is registered only while the field really has focus,
and the captured origin is dropped the moment focus leaves, so a pointer or `Tab` departure can never
arm a later forced restoration. Typing, caret keys, `Escape`, `Tab`, `Shift+Tab`, and pointer
interaction inside the field are untouched: the keyboard adapter already suppresses every mapped key
inside a text-editing control, and that was not changed.

---

## F. RetroArch controller failure reproduction

### F.1 What the operator's own launch left behind

The real launch produced **no RetroArch log at all**:

```console
$ ls -la ~/.local/share/com.retrofrontier.desktop/logs/retroarch
total 0
```

The generated configuration does set `log_to_file = "true"` and a `log_dir`, but RetroArch only
initialises file logging when verbosity is enabled, and RetroFrontier does not set `log_verbosity`.
So the old log was insufficient and no reproduction was invented from it.

### F.2 What the installation showed directly

The directory the generated configuration named for controller profiles was **empty**:

```console
$ grep joypad_autoconfig_dir ~/.local/share/com.retrofrontier.desktop/runtime-user/config/retroarch.cfg
joypad_autoconfig_dir = "/home/ben/.local/share/com.retrofrontier.desktop/runtime-user/autoconfig"

$ find ~/.local/share/com.retrofrontier.desktop/runtime-user/autoconfig -type f | wc -l
0
```

The qualified RetroArch AppImage ships none either — the extracted AppDir contains no autoconfig
directory and no profile matching `dualsense` anywhere.

### F.3 The reproduction that was actually run

The **managed** RetroArch binary from the installed version tree, driven with the **real generated
configuration** and only display/log settings overridden (`video_fullscreen=false` so no fullscreen
window was forced on the operator's session, `log_verbosity=true` so a log exists at all). The
physical DualSense was connected over USB throughout.

```console
$ .../runtime/versions/i-18d0a4fda8be7c01-1-293535/runtime/retroarch/usr/bin/retroarch \
    --menu --config <real config + log_verbosity>
```

Log, verbatim:

```text
[INFO] [udev] Pad #0 (/dev/input/event4) supports force feedback.
[INFO] [udev] Pad #0 (/dev/input/event4) supports 16 force feedback effects.
[INFO] [Autoconf] Sony Interactive Entertainment DualSense Wireless Controller (1356/3302) nicht konfiguriert.
[INFO] [Input] Found joypad driver: "udev".
```

(The session locale is German; `nicht konfiguriert` is RetroArch's *not configured*.)

That is the defect, reproduced: RetroArch **sees** the pad on `udev`, and reports it **unconfigured**.

---

## G. Root cause

### Confirmed facts

| Fact | Evidence |
| --- | --- |
| RetroArch detects the physical pad | `[udev] Pad #0 (/dev/input/event4)` |
| The selected joypad driver is `udev` | `[Input] Found joypad driver: "udev"` |
| Autodetection is already enabled | The `[Autoconf]` pass ran without RetroFrontier setting anything |
| The pad is reported **unconfigured** | `[Autoconf] … (1356/3302) nicht konfiguriert` |
| `joypad_autoconfig_dir` pointed at an empty directory | `find …` reported `0` files |
| The managed AppImage ships no profiles | No autoconfig or DualSense file anywhere in the extracted AppDir |
| Providing the official profile database fixes it | Same binary, same config, profile tree substituted → `[Autoconf] Sony DualSense konfiguriert in Port 1.` |
| RetroFrontier's own input ownership was never at fault | Nothing in the frontend was changed to fix this |

### Inference

An autoconfigured pad is how RetroArch obtains RetroPad binds for a device; a pad reported
unconfigured has none, which is why every button did nothing inside the game while the same pad
worked in RetroFrontier's own WebKitGTK Gamepad API. The observed before/after transition on the
real binary with the real controller is the direct evidence; the binding mechanism itself is the
inference, and nothing in the fix depends on it.

**Only the profile directory was wrong.** The driver and autodetection defaults are already correct,
so RetroFrontier pins neither `input_joypad_driver` nor `input_autodetect_enable`. A redundant
authority over a value that is already right is one more thing that can drift.

---

## H. Managed controller-profile architecture

`joypad_autoconfig_dir` now names the **verified immutable managed profile database** in the active
runtime version, at `runtime/support/joypad-autoconfig`.

**Why this shape, and why it is the smallest one.**

- Controller profiles are read-only support data, so RetroArch is pointed straight at the verified
  tree. The architecture already allows this: `libretro_directory` does the same, and Dolphin's
  managed `Sys` data is likewise consumed directly from the verified boundary.
- No writable `runtime-user/autoconfig` is composed any more, and the method that created it is
  gone. A directory RetroFrontier composes is a directory RetroFrontier can leave empty — which is
  precisely the bug. Removing it makes the failure mode unrepresentable rather than merely fixed.
- The component installs the database **verbatim at a tagged upstream revision**, not a hand-picked
  subset. That keeps provenance trivial to audit, redistributes the database's own MIT licence text
  with it, and covers every joypad driver rather than only the one today's build happens to select.
  The cost is ~2.2 MB of text, including upstream build scripts and CI metadata that RetroArch never
  reads; they are inventoried, digest-verified, and installed non-executable.
- RetroArch resolves a profile at `joypad_autoconfig_dir/<joypad driver>/<device>.cfg`. Because the
  install path *is* the database root, the generated value needs no derivation.

**Launch authority.** `RetroArchService::resolve_controller_profiles` refuses to launch when the
authenticated component is absent, when its `udev` directory is missing, or when either is a
symbolic link — a symlink would let something outside the verified tree decide what RetroArch reads.
The failure is `runtimeNotReady`, which is honest: the defect is in the installed release, not in the
game, the core, or the user's BIOS. A game whose controller cannot work is not started.

**What was explicitly not done.** No hard-coded DualSense mapping in Rust. No dependency on
`~/.config/retroarch/autoconfig`, `/usr/share/libretro/autoconfig`, or any other host location. No
download at game-launch time. No instruction for the user to configure RetroArch by hand. And no
input proxying of any kind — no virtual gamepad, uinput, evdev forwarding, synthetic keyboard
translation, or background button mirroring. RetroArch reads the physical controller itself, and the
M8 ownership transition is unchanged.

---

## I. Trust and provenance

| | |
| --- | --- |
| Upstream source | `github.com/libretro/retroarch-joypad-autoconfig` — the official libretro joypad autoconfig profile database |
| Immutable revision | tag `v1.22.0`, commit `38cf938bba0adbde375972053068f10d955a9d14` |
| Pinned URL | `https://codeload.github.com/libretro/retroarch-joypad-autoconfig/zip/38cf938bba0adbde375972053068f10d955a9d14` |
| Input SHA-256 | `45e2c28e4691073a7bc45b0fb86bc91f2aa9d2c0de9773e4dab0fb1341abe744` |
| Input size | 870 555 bytes |
| Derived artefact | `joypad-autoconfig-1.22.0.tar`, deterministic tar of the repository subtree |
| Artefact SHA-256 | `d81e3ac266d592b1732a7b16d77563aa513b270d2a5e592bf2040d633d6906cc` |
| Artefact size | 2 336 768 bytes |
| Licence | MIT (`COPYING`, "Copyright (c) 2019 The RetroArch team"), redistributed inside the component |

`v1.22.0` is the profile release accompanying the RetroArch 1.22 line this runtime pins; it is the
highest tag upstream publishes, and there is no `v1.22.2`.

**Why a tagged commit rather than the buildbot asset.** `buildbot.libretro.com/assets/frontend/
autoconfig.zip` is the artefact RetroArch's own updater fetches, but it is a rolling URL with no
revision identity — it was last modified the day before this pass. A tagged commit gives an immutable
revision that can be recorded, reviewed, and rebuilt.

It flows through the existing M7.5 trusted machinery unchanged: pinned by the committed release
definition → downloaded and verified against length and digest → derived by the deterministic
repackager → the derived artefact verified against its own pin → published as a TUF target →
authenticated, extracted, and inventory-verified by the client → installed into the immutable managed
version tree → exposed to launch through `VerifiedLaunchRuntime`. There is no side download and no
unauthenticated path.

---

## J. Runtime Release implications

**Inventory delta** — one input and one component added; nothing else touched.

| | Before | After |
| --- | --- | --- |
| Inputs | 6 | 7 (`retroarch-joypad-autoconfig-1.22.0-zip`) |
| Components | 6 | 7 (`joypad-autoconfig`, kind `support_asset`) |
| Published targets | 6 + manifest + policy | 7 + manifest + policy |
| Added installed bytes | — | 2 336 768 (tar), 1039 files, 1 044 inventory entries |

> **Corrected.** This section originally claimed that `release_id`, `release_sequence`, and
> `manifest_id` were unchanged and that "no release id or sequence semantics were mutated". Keeping
> them unchanged *was* the mutation: adding an authenticated component changes what the release
> ships, and ADR-012 makes an authenticated Runtime Release target immutable. The controller-profile
> component therefore belongs to a **new generation**, `rf-runtime-1.22.2-linux-x86_64-002`,
> sequence 2. Release 001 is preserved verbatim as a historical record, and no modified byte is
> published under any `001` identity. **Section Q is the authoritative account** of the release
> identity, its inputs, and its construction proof; where this section disagrees with section Q,
> section Q is correct.
>
> `channel`, `minimum_safe_release_sequence`, and `app_run_path` genuinely are unchanged — see
> section Q.2 for why the anti-rollback floor was deliberately **not** raised.

No manifest was hand-edited: every generated artefact in this report came out of
`rf-runtime-release` itself.

**Reinstallation is required.** The authenticated contents of the release changed, so the currently
installed `i-18d0a4fda8be7c01-1-293535` does not contain the profile component and its
`verified_launch_runtime` will now refuse to launch with `runtimeNotReady`. That refusal is the
intended behaviour, and the operator steps are in section Q.8.

---

## K. Regression tests

### Frontend (Search)

| Id | Test | Where |
| --- | --- | --- |
| A1 | Sidebar focused → `search` → Search receives focus | `AppShell.test.tsx` |
| A2 | Main card → `search` → Search receives focus; also from the Library heading and another header control | `AppShell.test.tsx` |
| A3 | Main card → `search` → `back` returns to that card; the same for a sidebar entry; pressing `search` again inside Search keeps the original origin | `AppShell.test.tsx` |
| A4 | The origin disappears while Search is focused → `back` takes the documented fallback | `AppShell.test.tsx` |
| A5 | No Search field rendered → button 3 does nothing and `SEARCH` is not offered | `AppShell.test.tsx` |
| A6 | Holding button 3 emits `search` exactly once; button 3 is index 3 and the other faces are unaffected | `gamepadAdapter.test.ts` |
| A7 | Button 3 held across ownership loss and return emits no stale action | `gamepadAdapter.test.ts` |
| A8 | Pointer, `Tab`, `Shift+Tab`, typing, caret keys, and `Escape` keep their native/text-editing behaviour; leaving Search by pointer arms no later restoration | `AppShell.test.tsx` |
| — | `SEARCH` is offered from both zones while the field exists | `AppShell.test.tsx` |
| — | `Y SEARCH` derives in stable hint order and a blank label is treated as unsupported | `footerHints.test.ts` |

### Runtime (controller profiles)

| Id | Test | Where |
| --- | --- | --- |
| B1 | The current-defect shape is refused: a runtime without the component cannot launch, a component present but empty is refused, and no writable profile directory is composed at all | `roundtrip_tests.rs` |
| B2 | The final release contains an authenticated, pinned joypad-autoconfig component, from an immutable revision | `roundtrip_tests.rs`, `definition.rs` |
| B3 | The generated config points `joypad_autoconfig_dir` only at the verified managed tree, and follows the installation it was built for | `retroarch_config.rs`, `retroarch.rs`, `roundtrip_tests.rs` |
| B4 | No host RetroArch autoconfig location is consulted, in the config or in the release definition | `retroarch_config.rs`, `retroarch.rs`, `definition.rs` |
| B5 | The built release contains the expected `udev` DualSense profile — asserted on its **content**: driver, device name, vendor id `1356`, product id `3302` — plus the redistributed licence text | `roundtrip_tests.rs` |
| B6 | `video_fullscreen`, `video_windowed_fullscreen`, `config_save_on_exit`, and every existing controlled-path guarantee are unchanged | `retroarch_config.rs`, `roundtrip_tests.rs` |
| B7 | Process identity, runtime entry point, core, and BIOS authority are unchanged | `roundtrip_tests.rs`, plus the untouched `launch.rs` suite |
| — | The profile root must be a real directory, must contain the `udev` directory, and must not be a symbolic link | `retroarch.rs` |

---

## L. Automated verification

All commands run at the final tree.

| Command | Result |
| --- | --- |
| `pnpm typecheck` | clean |
| `pnpm lint` | clean |
| `pnpm format:check` | clean |
| `pnpm test` | **36 files, 592 tests, 0 failed** |
| `pnpm build` | clean |
| `cargo fmt -- --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo test` | **418 passed, 0 failed, 1 ignored** |
| `cargo build --release` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test --all-features` | **440 passed, 0 failed, 7 ignored** |
| `git diff --check` | clean |

**Repeated runs.** The controller/focus/runtime suites were run three times each with identical
results: frontend `AppShell` + `focus` + `input` + `useControllerInput` — 8 files, 214 tests, three
times; Rust `retroarch` 40, `release::` 22, `launch` 55, three times. No flakiness.

**Re-run after the release-integrity pass (section Q).** The whole table above was re-run at the final
tree. Frontend results are byte-for-byte unchanged, which is the point: the release-integrity pass
touched no frontend code and no controller behaviour.

| Command | Result |
| --- | --- |
| `pnpm typecheck` | clean |
| `pnpm lint` | clean |
| `pnpm format:check` | clean |
| `pnpm test` | **36 files, 592 tests, 0 failed** — unchanged |
| `pnpm build` | clean |
| `cargo fmt -- --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test` | **418 passed, 0 failed, 1 ignored** — unchanged |
| `cargo test --all-features` | **448 passed, 0 failed, 7 ignored** — `+8` from section Q.7 |
| `cargo test --features release-tools --lib release::` | **30 passed, 0 failed, 6 ignored** (was 22 passed) |
| `cargo build --release` | clean |
| `git diff --check` | clean |

**Release-construction validation.** At the time this section was first written, `rf-runtime-release
build` had only been run against a *reduced* definition — the unchanged `retroarch` component plus the
new `joypad-autoconfig` component — because the four rolling nightly core URLs no longer matched their
pins. That is no longer the state of the work: the **complete** release now builds and publishes, as
Release 002. See **section Q.5** for the full proof and **section Q.3** for the immutable core source
that made it possible. The reduced-fixture result is superseded and is not evidence of anything this
report still claims.

**Real-hardware proof of the fix.** The managed RetroArch binary, the real generated configuration,
and the physical DualSense, with `joypad_autoconfig_dir` pointed at the **tar the release tool just
produced**:

```text
[INFO] [udev] Pad #0 (/dev/input/event4) supports force feedback.
[INFO] [Autoconf] Sony DualSense konfiguriert in Port 1.
[INFO] [Input] Found joypad driver: "udev".
```

This proves detection and profile matching. It does **not** prove in-game play, which only the
operator can establish — see section M.

---

## M. DualSense evidence

The RetroArch-visible device identity, taken from the input stack RetroArch actually reads (evdev),
never from WebKitGTK's `Gamepad.id`:

| | |
| --- | --- |
| Connection | USB (`Bus=0003`) |
| Device name | `Sony Interactive Entertainment DualSense Wireless Controller` |
| Vendor id | `0x054C` (1356) |
| Product id | `0x0CE6` (3302) |
| Node | `/dev/input/event4`, `js1` |
| Driver | `udev` |

The selected profile, `udev/Sony Interactive Entertainment DualSense Wireless Controller.cfg`:

```text
input_driver = "udev"
input_device = "Sony Interactive Entertainment DualSense Wireless Controller"
input_vendor_id = "1356"
input_product_id = "3302"
input_device_alt1 = "DualSense Wireless Controller"
```

Every field matches the physical device, and RetroArch's own `[Autoconf]` line quotes the same
`(1356/3302)` pair. The database also carries a `udev` DualSense **Edge** profile and Bluetooth-mode
aliases (`input_device_alt1`), so the Bluetooth connection mode is covered by the same profile even
though this qualification ran over USB.

---

## N. Operator requalification checklist

Every physical step below is:

```text
NOT PERFORMED — HUMAN INTERACTION REQUIRED
```

### N.0 Reinstall the managed runtime first

The authenticated release contents changed, so the installed runtime must be replaced before any
controller test is meaningful. Until it is, launching will correctly refuse with `runtimeNotReady`.

The exact procedure now lives in **section Q.8**, because the release the operator installs is
Release 002 and the qualification environment must select the 002 manifest target. The old
instruction here — republish and expect the four nightly core inputs to fail — no longer applies.

```text
NOT PERFORMED — HUMAN INTERACTION REQUIRED
```

### N.1 Search

1. Focus the sidebar. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
2. Press Triangle. Search takes focus. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
3. Press B. Focus returns to the original sidebar entry. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
4. Enter the main area (confirm a sidebar filter). → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
5. Press Triangle. Search takes focus. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
6. Press B. Focus returns to the original main card. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
7. Pointer, Tab, and typing in Search all behave normally. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`

### N.2 RetroArch

1. Connect the same DualSense used for qualification. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
2. Launch RetroFrontier. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
3. Confirm the browser controller is still Standard-mapped (footer says `CONTROLLER CONNECTED`). → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
4. Launch a legal game. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
5. RetroArch opens fullscreen. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
6. Verify the D-pad, the left stick, and the face buttons work in the game. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
7. Verify Start (Options) and Select (Create). → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
8. If practical, verify L1/R1 and the L2/R2 triggers. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
9. Exit RetroArch. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
10. RetroFrontier regains window focus and controller input. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`
11. No held button is replayed on return. → `NOT PERFORMED — HUMAN INTERACTION REQUIRED`

### N.3 If a controller problem remains

Only then, and once: relaunch with `log_verbosity = "true"` added to the generated configuration and
collect `logs/retroarch/retroarch.log`. The `[Autoconf]` and `[Input] Found joypad driver` lines are
the two that matter.

---

## O. Remaining risks

- **In-game input is not proven.** Detection and profile matching are proven on the real binary with
  the real pad. Whether every button behaves correctly *inside a running game* is a physical result
  only the operator can produce.
- **~~The release cannot currently be rebuilt end to end.~~** Fixed in the release-integrity pass:
  every core now comes from the version-addressed stable bundle and the complete release builds and
  publishes (sections Q.3 and Q.5).
- **One upstream input still has no version-addressed URL.** `assets/system/Dolphin.zip` is
  regenerated upstream — its bytes changed during this very pass while its length stayed identical.
  The derived `dolphin-sys` component is unaffected and provably byte-identical, because the
  deterministic repackager normalises the container metadata that actually changed (section Q.4), but
  a real change to the Sys data upstream would still break reconstruction from the network. The
  maintainer input cache is the mitigation; an immutable revision is the proper follow-up.
- **The four core binaries are new component versions, and in-game play with them is not proven.**
  The stable 1.22.2 build of each core is not byte-identical to the nightly build Release 001 shipped
  (section Q.3). All four load and identify themselves correctly through the real installed tree, and
  the real DualSense is configured by the real RetroArch 002 binary, but nothing about *playing* a
  game on the new binaries is proven without the operator.
- **`log_to_file` is set but produces nothing.** RetroArch only initialises file logging when
  verbosity is enabled, so the configured log directory has always stayed empty. Deliberately not
  changed here — this pass is narrow — but the generated configuration currently promises a log it
  never writes, which cost real diagnostic time on this very defect.
- **The managed version tree is verified-immutable, not filesystem-immutable.** Its directories are
  mode `0755` and user-owned, so RetroArch's *Save Controller Profile* menu action could in principle
  write into the profile tree and make the next verification refuse the installation. This is the
  same exposure Dolphin's managed `Sys` data already has, and it is a menu action RetroFrontier never
  triggers.
- **`v1.22.0` is not `v1.22.2`.** Upstream publishes no `v1.22.2` profile tag; the 1.22 line tag is
  the closest immutable revision, and it contains a profile that matches the qualification device
  exactly.
- **Bluetooth mode is covered but not qualified.** The profile's `input_device_alt1` covers the
  Bluetooth device name, and the database ships a matching profile, but this qualification ran over
  USB only.
- **Search origin capture depends on a semantic identity.** Reaching Search from an unregistered
  control (the wordmark, the theme toggle) captures no origin, so `back` takes the documented
  fallback. That is the intended, honest behaviour rather than a guess.

---

## Q. Runtime Release integrity corrective pass

A separate corrective pass on top of `8700a91eaab39516e43c9ffaddd37f2531ccb2a1`. It changes no
controller behaviour at all; it fixes two release-engineering defects the controller fix exposed.

### Q.1 Why changing authenticated components requires Release 002

ADR-012 gives a Runtime Release three properties that have to hold together: an **immutable release
id**, a **monotonically increasing release sequence**, and **immutable authenticated targets**. The
third is the one the controller fix broke. Adding `joypad-autoconfig` changes the release manifest,
changes the published target set, and changes the installed inventory a client verifies — while the
definition still said `rf-runtime-1.22.2-linux-x86_64-001`, `release_sequence = 1`, and
`rf-runtime-linux-x86_64-001.manifest.json`.

That is not a re-publication of Release 001. It is a *different release wearing Release 001's name*,
and it defeats the point of pinning an identity:

- a client that has already authenticated Release 001 cannot tell the two apart by identity;
- an installed `i-18d0a4fda8be7c01-1-293535` claims to be Release 001 and legitimately is — the old
  Release 001 — so "reinstall Release 001" becomes ambiguous rather than idempotent;
- the sequence stops being a monotonic record of what was published;
- any archived Release 001 manifest and any Release 001 TUF target now contradict the "current"
  Release 001, so provenance can no longer be retraced.

So the fix is a new generation, not an edited one:

| | Release 001 (historical) | Release 002 (active) |
| --- | --- | --- |
| `release_id` | `rf-runtime-1.22.2-linux-x86_64-001` | `rf-runtime-1.22.2-linux-x86_64-002` |
| `release_sequence` | 1 | **2** |
| `manifest_id` | `rf-runtime-linux-x86_64-001` | `rf-runtime-linux-x86_64-002` |
| `manifest_target_name` | `rf-runtime-linux-x86_64-001.manifest.json` | `rf-runtime-linux-x86_64-002.manifest.json` |
| Definition | `release/linux-x86_64/history/runtime-release-001.json` | `release/linux-x86_64/runtime-release.json` |
| Components | 6 | 7 |
| Inventory entries | 2 932 | 3 985 |

Release 001 is preserved **verbatim** at
[`release/linux-x86_64/history/runtime-release-001.json`](../release/linux-x86_64/history/runtime-release-001.json),
including the four rolling nightly URLs it pinned. Re-pinning a historical definition would destroy
the record rather than fix anything. No modified byte is published under any `001` identity: the
Release 002 repository publishes only `002` targets.

**Every place the identity is selected or embedded** was checked:

| Location | Kind | State |
| --- | --- | --- |
| `release/linux-x86_64/runtime-release.json` | `manifest_id`, `release_id`, `release_sequence`, `manifest_target_name` | Release 002 |
| `release/linux-x86_64/history/runtime-release-001.json` | archived definition | Release 001, untouched |
| `RETROFRONTIER_RUNTIME_MANIFEST_TARGET` | the **only** runtime selector; no default is compiled in | supplied by the environment |
| `src-tauri/src/release/qualification.rs` | documented qualification environment | selects 002 |
| `docs/M7_5_RUNTIME_QUALIFICATION.md` | documented qualification environment | selects 002, with a superseded-release note |
| `docs/CORE_MATRIX.md` | which release ships the four resolved cores | Release 002 |
| `src/features/settings/*.test.ts(x)` | frontend fixtures | deliberately left as arbitrary ids — these assert the panel renders *whatever* release id it is given, and pinning the active id into them would create false coupling |

`ReleaseDefinition::supersedes` now encodes the rule itself, so it is enforced rather than remembered:
a successor must advance the sequence, and if the authenticated contents differ it must also change
the release id, the manifest id, and the manifest target name.

### Q.2 Why `minimum_safe_release_sequence` stays 1

It was **not** raised to 2. That field is the client's anti-rollback floor: it says "refuse to install
anything at or below this sequence, because it is unsafe". Raising it is a security revocation of
Release 001, and the finding here is not that Release 001 is unsafe — it is that Release 001 is
*superseded* and was *unreconstructable*. Those are different claims with different consequences: a
revocation permanently removes an operator's ability to fall back, and it should be a deliberate
security decision recorded against a specific vulnerability, not a side effect of shipping a newer
generation. The committed policy target is therefore:

```json
{"minimum_safe_release_sequence":1,"revoked_release_ids":[]}
```

A regression test asserts the floor did not move with the generation.

### Q.3 Why the rolling nightly core URLs were unacceptable, and what replaced them

Release 001 pinned four cores under
`buildbot.libretro.com/nightly/linux/x86_64/latest/`. `latest` is a moving pointer, so the bytes
behind those four URLs had already been replaced upstream and the committed release could no longer be
reconstructed from its own recorded provenance. Re-pinning today's `latest` bytes would restore the
build for exactly as long as it takes upstream to publish the next nightly, so it was not treated as
a fix. The original runtime spike had already written this rule down —
[`RUNTIME_SPIKE.md`](RUNTIME_SPIKE.md): "Core directories such as `nightly/<platform>/latest/` are
mutable evidence sources, never production release IDs" — so this was a known rule that the committed
definition violated, not a newly discovered property of the upstream host.

**The immutable source selected for all four cores** is the official *version-addressed* stable core
bundle that accompanies the RetroArch build this runtime already pins:

| | |
| --- | --- |
| Source URL | `https://buildbot.libretro.com/stable/1.22.2/linux/x86_64/RetroArch_cores.7z` |
| SHA-256 | `4b7ed8dc97d4bf035fce182c64b5658c7662e2e9e5d42129538afbd4b6096307` |
| Byte length | 274 237 400 |
| Upstream `Last-Modified` | Thu, 20 Nov 2025 02:50:05 GMT — the RetroArch 1.22.2 publication date |
| Contents | 199 libretro cores, 5 directories, 1 651 432 240 bytes uncompressed, single solid LZMA2 block |
| Licence recorded | `GPL-3.0-or-later` as the bundle aggregate; each derived component keeps its own core licence |

The archive was **inspected, not assumed**. Its layout is a portable-home tree, and every core is a
bare `.so` — there is no per-core archive to redistribute:

```text
RetroArch-Linux-x86_64/RetroArch-Linux-x86_64.AppImage.home/.config/retroarch/cores/nestopia_libretro.so
RetroArch-Linux-x86_64/RetroArch-Linux-x86_64.AppImage.home/.config/retroarch/cores/bsnes_mercury_balanced_libretro.so
RetroArch-Linux-x86_64/RetroArch-Linux-x86_64.AppImage.home/.config/retroarch/cores/mednafen_psx_libretro.so
RetroArch-Linux-x86_64/RetroArch-Linux-x86_64.AppImage.home/.config/retroarch/cores/dolphin_libretro.so
```

All four required cores are present, so this bundle is the source. One consequence had to be handled
rather than papered over: a component's target artefact is an archive whose installed layout the
manifest declares, and a bare `.so` is not one. A new derivation, `seven_zip_member_tar`, lifts the
named member out of the bundle and packages it as a **deterministic single-entry tar** under a
declared flat `entry_name`, executable because it is native code the runtime `dlopen`s. It shares one
tar builder with the existing `zip_subtree_tar` derivation, so determinism is proven in one place, and
the resulting artefact is pinned by its own digest exactly as before. Support data still gets mode
`0644`; only this derivation marks its entry executable.

**The core binaries changed. They are not byte-identical to the M7.5 cores.** The stable 1.22.2 build
of a core is a different build from the nightly Release 001 happened to capture, and the hashes prove
it — including `bsnes-mercury-balanced`, whose length is identical and whose content is not, which is
exactly why identity is never inferred from size:

| Core | Release 001 `.so` (nightly) | Release 002 `.so` (stable 1.22.2) | Identical? |
| --- | --- | --- | --- |
| `nestopia_libretro.so` | 5 431 800 · `bde9bbe38da4d0c715320d26c931ba80960c11947b1071fe186a6748182f5300` | 5 360 704 · `3f1b76f6d8e68c149a3269c314b406d15f806597333466b1f6a0af01afab52c7` | **No** |
| `bsnes_mercury_balanced_libretro.so` | 1 786 872 · `a546d3b04a81325c7397f140231d5b0f6bc700777ba8ca2f4ce836103c5b07de` | 1 786 872 · `06fe34874cf8fdec00801a2d22c497c477721a23a87a6e7b5cae82dc1770c5be` | **No** (same length) |
| `mednafen_psx_libretro.so` | 12 504 960 · `56163b4d5df810c645973fa1ea792b5a5640b1a235d7cf29c853a5eea085ff0a` | 10 471 424 · `ffc1c18a1fc41bf1f28cccaaa7e30e6677ec2aeda91c39b2d8f72d3bd4e2e641` | **No** |
| `dolphin_libretro.so` | 16 549 360 · `1f2f21eb032949e903bfde850b71f697bb2288a48c8ba8802a8d76fbcf9858ad` | 20 514 952 · `c28dc9a2207ffed938810abf3e24df23dc39ef58c6a16c036fc2c58c2240ef10` | **No** |

The Release 001 column is taken from that release's own authenticated manifest inventory and confirmed
against the bytes still installed at `i-18d0a4fda8be7c01-1-293535`; the Release 002 column is the
bytes extracted from the pinned bundle and confirmed again after installation. No byte identity is
claimed anywhere, because no hash supports it. All four are therefore **new Release 002 component
versions**, and Q.6 is the re-run qualification evidence that goes with them.

### Q.4 The other mutable input, and why the component is still safe

Fixing the cores exposed a second instance of the same class, not named in the finding:
`https://buildbot.libretro.com/assets/system/Dolphin.zip` has no version-addressed form and is
regenerated upstream. Its bytes changed **during this pass** — pinned
`a406e5207481806f358b726ccc674f169d6e1a0c0528ae135b76b9e9259ee313`, now
`5d4b217991187abfc326ccd13849aa0ad0af623c78b26ff25979933843c67c30` — at an identical length of
3 195 803, which is the signature of a rebuilt zip container rather than changed data.

That reading was tested rather than assumed. Deriving the component from the new zip through the same
deterministic repackager produces a **byte-identical** artefact:

```text
dolphin-sys.tar   7959552   591b8df55ad99064824244c33ae9640714dc1701251aa2d2ba65810876fbda90   matches pin
```

So the authenticated `dolphin-sys` component of Release 002 is exactly the component Release 001
shipped; only the provenance record of the container needed refreshing, and the derived pin is what
actually protects the bytes. This is recorded as a residual risk in section O — a genuine change to
upstream's Sys data would still break network reconstruction, with the maintainer input cache as the
mitigation and an immutable revision as the follow-up. It is **not** a `/latest/` URL, and no core
input has a rolling path any more.

### Q.5 Complete Release 002 construction and publication proof

Run against the **complete** committed definition, not a reduced fixture, with the four verified
pinned inputs in the maintainer cache:

```console
$ rf-runtime-release build \
    --definition release/linux-x86_64/runtime-release.json \
    --output <work>/out --cache <work>/input-cache
release       rf-runtime-1.22.2-linux-x86_64-002
sequence      2
retroarch     1.22.2
manifest      rf-runtime-linux-x86_64-002.manifest.json
manifest hash a6205d4fde92753bd10db3a47c48b3b75f65e96cee1781fffdb4d15e447594a5
inventory     3985 entries
target        retroarch-1.22.2-linux-x86_64.AppImage                   10390008  794b0f65d4efa918e2ad05cac34b444a4f3207ed6c74834b7c14eb5fb15e1cc4
target        nestopia_libretro.so.tar                                  5362688  9ef74939752057dbf8aae167984d909a2053e03d76e145c1b5cf993e174fd0d6
target        bsnes_mercury_balanced_libretro.so.tar                    1788416  3e13256e7f9f0bc73a9011460c2064644ac2f8e2d68461a97b7a2edbc2114f95
target        mednafen_psx_libretro.so.tar                             10472960  8112f600f7f69c861edb2c09e1389cdfaff9a3925bd86d06888147cfc1360251
target        dolphin_libretro.so.tar                                  20516864  42fab8f87403f32d71eeeeb29bb13f1eccffc347082dfb377b901a5c6144d3df
target        dolphin-sys.tar                                           7959552  591b8df55ad99064824244c33ae9640714dc1701251aa2d2ba65810876fbda90
target        joypad-autoconfig-1.22.0.tar                              2336768  d81e3ac266d592b1732a7b16d77563aa513b270d2a5e592bf2040d633d6906cc
target        rf-runtime-linux-x86_64-002.manifest.json                  870739  a6205d4fde92753bd10db3a47c48b3b75f65e96cee1781fffdb4d15e447594a5
target        runtime-policy.json                                            60  ecc7471609ac23b88c6dae65323ea4420c335ea597c8201dd95c8be9fc980877
```

Every stage completed, in order: all four pinned inputs verified against length and digest → every one
of the seven derived component artefacts matched its own pin → the 3 985-entry inventory was generated
from the artefacts → `RuntimeManifest::validate_for_linux_x86_64` (the *client's* validator) accepted
the manifest → every component was extracted through the production `LinuxRuntimeArchiveExtractor` →
`verify_tree` accepted the extracted tree against the inventory → `validate_app_run` accepted the entry
point. A failure at any stage is a non-zero exit and no output, so the printed target table *is* the
proof that all of them passed.

Then the qualification TUF publication path, same tool, same complete definition, existing
qualification keys at `~/.retrofrontier-qualification-keys`:

```console
$ rf-runtime-release publish --definition release/linux-x86_64/runtime-release.json \
    --output <work>/out --cache <work>/input-cache --keys ~/.retrofrontier-qualification-keys
metadata      <work>/out/metadata
targets       <work>/out/repository-targets
trusted root  <work>/out/metadata/root.json
```

Published repository: `1.root.json`, `1.targets.json`, `1.snapshot.json`, `timestamp.json`, and **9
consistent-snapshot targets** (`<sha256>.<name>`). Ed25519 only, 2-of-3 thresholds on root and targets,
separately scoped snapshot and timestamp keys. Nothing was hand-edited — no manifest, no TUF metadata.

**Release 002 exact figures**

| | |
| --- | --- |
| Manifest size | **870 739 bytes** (Release 001: 626 988) |
| Manifest SHA-256 | `a6205d4fde92753bd10db3a47c48b3b75f65e96cee1781fffdb4d15e447594a5` |
| Inventory entries | **3 985** (Release 001: 2 932) |
| Components | **7** |
| Published targets | **9** (7 components + manifest + policy) |
| Pinned inputs | **4** (Release 001: 6) |

**All seven component identities**

| Component | Kind | Target | Format | Artefact SHA-256 · bytes | Installed at | Executable |
| --- | --- | --- | --- | --- | --- | --- |
| `retroarch` | `runtime` | `retroarch-1.22.2-linux-x86_64.AppImage` | `app_image` | `794b0f65d4efa918e2ad05cac34b444a4f3207ed6c74834b7c14eb5fb15e1cc4` · 10 390 008 | `runtime/retroarch` | `usr/bin/retroarch` |
| `nestopia` | `core` | `nestopia_libretro.so.tar` | `tar` | `9ef74939752057dbf8aae167984d909a2053e03d76e145c1b5cf993e174fd0d6` · 5 362 688 | `cores/nestopia` | `nestopia_libretro.so` |
| `bsnes-mercury-balanced` | `core` | `bsnes_mercury_balanced_libretro.so.tar` | `tar` | `3e13256e7f9f0bc73a9011460c2064644ac2f8e2d68461a97b7a2edbc2114f95` · 1 788 416 | `cores/bsnes-mercury-balanced` | `bsnes_mercury_balanced_libretro.so` |
| `beetle-psx` | `core` | `mednafen_psx_libretro.so.tar` | `tar` | `8112f600f7f69c861edb2c09e1389cdfaff9a3925bd86d06888147cfc1360251` · 10 472 960 | `cores/beetle-psx` | `mednafen_psx_libretro.so` |
| `dolphin` | `core` | `dolphin_libretro.so.tar` | `tar` | `42fab8f87403f32d71eeeeb29bb13f1eccffc347082dfb377b901a5c6144d3df` · 20 516 864 | `cores/dolphin` | `dolphin_libretro.so` |
| `dolphin-sys` | `support_asset` | `dolphin-sys.tar` | `tar` | `591b8df55ad99064824244c33ae9640714dc1701251aa2d2ba65810876fbda90` · 7 959 552 | `runtime/support/dolphin-sys` | — |
| `joypad-autoconfig` | `support_asset` | `joypad-autoconfig-1.22.0.tar` | `tar` | `d81e3ac266d592b1732a7b16d77563aa513b270d2a5e592bf2040d633d6906cc` · 2 336 768 | `runtime/support/joypad-autoconfig` | — |

**Is every source reconstructable from immutable or pinned provenance?**

| Input | Addressing | Reconstructable |
| --- | --- | --- |
| `retroarch-1.22.2-linux-x86_64-7z` | `/stable/1.22.2/` — version-addressed | Yes |
| `retroarch-cores-1.22.2-linux-x86_64-7z` | `/stable/1.22.2/` — version-addressed | Yes |
| `retroarch-joypad-autoconfig-1.22.0-zip` | `codeload…/zip/38cf938b…` — immutable commit | Yes |
| `dolphin-system-assets-zip` | no version-addressed form upstream; container regenerated | **Component yes, container no** — the derived artefact is provably stable (Q.4); the container digest is pinned and cached, but a real upstream data change would break network reconstruction |

Six of seven components are reconstructable from an immutable, version-addressed or commit-addressed
upstream URL. The seventh (`dolphin-sys`) is reconstructable from its pinned derived digest and the
maintainer input cache, and its bytes are unchanged since Release 001. **No core input has a rolling
`/latest/` or `/nightly/` path.**

### Q.6 Re-run real qualification evidence for the new component versions

Because all four core binaries changed, the real-runtime evidence was re-run rather than inherited.
Everything below is a real result on this machine, against the **published Release 002 qualification
repository**, in an isolated app-data root so the operator's own installation was not disturbed.

**Install through the production TUF path** — the same `ToughTrustedReleaseSource`, `RuntimeManager`,
extractor, inventory verification, and activation protocol the application composes:

```console
$ RETROFRONTIER_RUNTIME_SOURCE=qualification \
  RETROFRONTIER_RUNTIME_MANIFEST_TARGET=rf-runtime-linux-x86_64-002.manifest.json \
  … cargo test --features release-tools --lib qualification -- --ignored
install_the_real_managed_runtime  ... ok
report_the_verified_managed_runtime ... ok

state=Ready source=Some(Qualification) release=Some("rf-runtime-1.22.2-linux-x86_64-002")
             installation=Some("i-18d144307a8e023a-1-26661")
verified cores: ["beetle-psx", "bsnes-mercury-balanced", "dolphin", "nestopia"]
support: dolphin-sys       -> … present=true
support: joypad-autoconfig -> … present=true
```

The four content-dependent qualification tests (`launch_a_real_game_through_the_m7_path`,
`rescan_the_managed_library`, `report_bios_and_system_readiness`, `reconcile_after_a_crash`) were not
run: they require `RETROFRONTIER_QUALIFICATION_LIBRARY` and the operator's own legally owned content.
They are part of section N, not of this pass.

**Every new core binary really loads.** Each installed core was `dlopen`ed from the verified Release
002 tree and asked to identify itself through the libretro API:

| Component | `retro_api_version` | `library_name` | `library_version` | Installed mode · bytes |
| --- | --- | --- | --- | --- |
| `nestopia` | 1 | Nestopia | `1.53.2 5deada5` | `0755` · 5 360 704 |
| `bsnes-mercury-balanced` | 1 | bsnes-mercury | `v094 (Balanced) 0f35d04` | `0755` · 1 786 872 |
| `beetle-psx` | 1 | Beetle PSX | `0.9.44.1 d6383bf` | `0755` · 10 471 424 |
| `dolphin` | 1 | dolphin-emu | `fd1aca3a` | `0755` · 20 514 952 |

Every installed digest equals the bundle-extracted digest in Q.3, and `retro_api_version = 1` matches
the release's declared `retroarch_core_api`. The executable bit the new derivation sets survives all
the way to the installed tree, which is what `validate_for_linux_x86_64` requires of a component's
declared executable.

**The controller fix still works on Release 002.** The Release 002 RetroArch binary, the Release 002
profile component (420 `udev` profiles, DualSense and DualSense Edge present), and the same physical
DualSense over USB:

```text
[INFO] RetroArch 1.22.2 (Git 69a4f0e)
[INFO] [udev] Pad #0 (/dev/input/event4) supports force feedback.
[INFO] [Input] Found joypad driver: "udev".
[INFO] [Autoconf] Sony DualSense konfiguriert in Port 1.
```

`konfiguriert in Port 1` — configured, on the new release. As before, this proves detection and
profile matching, **not** in-game play, which stays section N.2's operator step.

### Q.7 Regression coverage added

| Id | What it prevents | Where |
| --- | --- | --- |
| R1 | Changing component contents while keeping a published release identity. Asserts the committed 002 definition legitimately supersedes 001, and refuses a synthetic definition with changed contents under 001's id, manifest id, or manifest target — down to a single re-pinned component. Republishing byte-identical contents is deliberately still allowed. | `definition.rs` |
| R2 | A new generation that does not advance the release sequence, and a generation that silently raises the anti-rollback floor. | `definition.rs` |
| R3 | Any `/latest/` or `/nightly/` URL in the active definition, and any core not derived from the version-addressed stable bundle by a named member. | `definition.rs` |
| R4 | A complete Release 002 that omits `joypad-autoconfig` — asserts the exact seven-component set. | `definition.rs` |
| R5 | Qualification selection continuing to request the 001 manifest: every documented `RETROFRONTIER_RUNTIME_MANIFEST_TARGET` must equal the active manifest target and must not be a superseded one. | `definition.rs` |
| — | The new core derivation: reproducible bytes, the entry at the component's `executable_relative_path`, executable, and nested/escaping/empty entry names refused. | `inventory.rs` |
| — | Read-only support data never becoming executable now that both derivations share one tar builder. | `inventory.rs` |

### Q.8 Exact reinstall procedure

`rf-runtime-release` never vendors runtime binaries into the repository, so the maintainer cache and
the published repository live outside it. Substitute your own working directory for `<work>`.

```bash
# 1. Construct and publish the complete Release 002 qualification repository.
#    Inputs are downloaded once and verified against their pins; nothing is trusted for
#    having downloaded successfully.
cd src-tauri
cargo run --features release-tools --bin rf-runtime-release -- publish \
  --definition ../release/linux-x86_64/runtime-release.json \
  --output <work>/out \
  --cache <work>/input-cache \
  --keys ~/.retrofrontier-qualification-keys

# 2. Point the application at that repository. The manifest target MUST be the 002 one:
#    there is no compiled-in default, so this environment is the release selection.
export RETROFRONTIER_RUNTIME_SOURCE=qualification
export RETROFRONTIER_RUNTIME_TUF_ROOT=<work>/out/metadata/root.json
export RETROFRONTIER_RUNTIME_METADATA_URL=file://<work>/out/metadata/
export RETROFRONTIER_RUNTIME_TARGETS_URL=file://<work>/out/repository-targets/
export RETROFRONTIER_RUNTIME_MANIFEST_TARGET=rf-runtime-linux-x86_64-002.manifest.json

# 3. Launch RetroFrontier and install through Settings → managed runtime. Never by hand:
#    installation is what authenticates, extracts, verifies, and activates.
pnpm tauri:dev
```

Then confirm, before touching the controller:

1. Settings reports the runtime `Ready`, release `rf-runtime-1.22.2-linux-x86_64-002`, and a **new**
   installation id (not `i-18d0a4fda8be7c01-1-293535`).
2. `runtime/versions/<new-installation>/runtime/support/joypad-autoconfig/udev/` contains
   `Sony Interactive Entertainment DualSense Wireless Controller.cfg`.
3. All four cores report as available in readiness.

The previous Release 001 installation is retained by the ordinary retention policy and is not deleted
by this pass; activation moves to the new installation. Only then is section N.2 meaningful.

```text
NOT PERFORMED — HUMAN INTERACTION REQUIRED
```

---

## P. Verdict

```text
M8 FINAL HARDWARE INPUT PASS — READY FOR OPERATOR REQUALIFICATION
```

Both input findings are fixed, both fixes are covered by regression tests, and the RetroArch fix is
demonstrated on the real managed binary with the real physical DualSense: `not configured` before,
`configured in Port 1` after.

Both release-engineering findings are fixed as well, and the verdict stands on the condition it was
given: the **complete** Release 002 definition builds through every stage and publishes to a
qualification TUF repository (section Q.5), with no rolling core URL left anywhere in the active
definition.

The physical controller must not be called fixed until the operator reinstalls Release 002 per
section Q.8 and runs the real in-game test in section N.2.

---

## R. Final physical face-button mapping correction

This section records one further corrective pass, opened by a newly proven physical mapping defect.
Nothing in sections A–Q was re-tested or rewritten by it, and no Runtime Release file was touched.

### R.1 The ownership result, now recorded as passed

The operator physically performed the application-input ownership test — launch a game in RetroArch,
leave RetroArch running, Alt-Tab back to RetroFrontier, use the controller:

```text
RetroArch running + Alt-Tab to RetroFrontier:
RetroFrontier ignores controller input — PASS
```

The `ownsApplicationInput` contract therefore passed real hardware qualification. It was not
redesigned, weakened, or otherwise touched by this pass: `pendingGameId`, `running`, native
window-focus handling, ownership release/adoption, stale held-button protection, and the RetroArch/
RetroFrontier handoff are unchanged.

### R.2 The raw hardware probe

The operator probed the real browser `Gamepad.buttons` array on the qualification hardware — Linux /
WebKitGTK application frontend, physical Sony DualSense, `mapping === 'standard'`:

```text
Cross     = 0
Circle    = 1
Square    = 3
Triangle  = 2
```

Observed UI behaviour agreed with the probe: Square invoked Search, Triangle invoked the focused
game's Context action, Circle correctly performed Back.

The canonical W3C Standard Gamepad layout — which RetroFrontier is written against, and which
`mapping === 'standard'` is a promise of — puts the **left** face button at index 2 and the **upper**
face button at index 3. The probe shows the opposite: index 2 is the upper button (Triangle) and index
3 is the left button (Square). The two upper/left face buttons arrive transposed while the browser
still claims the Standard mapping.

### R.3 Why the canonical adapter produced swapped Search/Context

`GAMEPAD_BUTTON_INDEX` reads canonical index 2 as `context` and canonical index 3 as `search`, which
is correct. It was reading a transposed array, so the two actions arrived on the wrong physical
buttons. Nothing in the adapter, the focus layer, or the UI was wrong; the input was.

The transposition was traced to the engine on the qualification machine, not inferred:

| Fact | How it was established |
| --- | --- |
| The attached pad is `054c:0ce6`, version `0x8111`, kernel name `Sony Interactive Entertainment DualSense Wireless Controller` | `/sys/class/input/js1/device/{name,id/*}`, `lsusb` |
| The engine is WebKitGTK 2.52.5 over libmanette 0.2.13 | `pkg-config --modversion webkit2gtk-4.1`, `rpm -q libmanette` |
| WebKitGTK's `Gamepad.id` is **only** the kernel device name | Its Gamepad backend imports `manette_device_get_name` and no libmanette vendor/product accessor at all (`nm -D libwebkit2gtk-4.1.so.0` → `manette_device_get_name`, `manette_event_get_button`, `manette_event_get_absolute`, `manette_monitor_*`) |
| WebKitGTK translates evdev button codes to Standard Gamepad indices with `BTN_X (0x133) → 2` and `BTN_Y (0x134) → 3` | Disassembly of its `manette_event_get_button` call site: a jump table over `code - 0x130` whose entries 3 and 4 land on the `2` and `3` cases |
| On Linux `BTN_X == BTN_NORTH == 0x133` and `BTN_Y == BTN_WEST == 0x134` | `linux/input-event-codes.h` |

So WebKitGTK reads those two codes under their **letter** meaning (X is the left button, Y the upper
one) while the DualSense's kernel driver emits them under their **positional** meaning — north
(Triangle) as `0x133`, west (Square) as `0x134`. The upper and left face buttons therefore change
places on the way into the Standard Gamepad array, and `mapping` still says `standard`.

This also explains why the defect is device-scoped rather than engine-wide: a pad whose driver uses
the letter convention — an Xbox-style pad via `xpad`, where the X button is `BTN_X` — comes through
that same table correctly.

### R.4 The quirk detection rule

```text
runtime is WebKitGTK on Linux   AND   Gamepad.id matches /dualsense/i
```

Both halves are required, and each is narrow for a reason:

- **The engine half** matches the engine, not the packaging, because the defective translation lives
  in the engine: `AppleWebKit/` present, a `Linux` platform token present, and `Chrome|Chromium|
  CriOS|Edg/` absent. Chromium advertises `AppleWebKit/` too and is excluded explicitly, so a
  Chrome/Chromium development browser on the same machine keeps the canonical path, as does WebKit on
  macOS, which has a different Gamepad backend entirely.
- **The device half** matches the kernel device name because that string is the *whole* of what
  WebKitGTK puts in `Gamepad.id` (R.3) — no vendor or product id is available to match on. One token
  covers both connection namings: `Sony Interactive Entertainment DualSense Wireless Controller` over
  USB, `DualSense Wireless Controller` over Bluetooth.

Deliberately **not** implemented:

- `Linux ⇒ swap 2 and 3` — the defect needs the driver's positional convention as well as the engine,
  and Xbox-style pads on the same engine are correct.
- `all PlayStation controllers ⇒ swap 2 and 3` — only the DualSense has been physically measured.
  Other pads using the positional convention (a DualShock 4, for instance) are plausible candidates
  and are left alone until someone measures one.
- Any change to the canonical semantics themselves. See R.6.

Because the predicate is a conjunction of two positive matches, everything it does not recognize
takes the canonical path unchanged. It fails safe.

### R.5 Where normalization happens

A new module, `src/input/gamepadQuirks.ts`, sits between the browser and the existing adapter:

```text
navigator.getGamepads()
        ↓
gamepadQuirks: quirk normalization      ← the only place a non-canonical physical layout exists
        ↓
canonical W3C Standard Gamepad layout
        ↓
gamepadAdapter (GAMEPAD_BUTTON_INDEX)   ← unchanged
        ↓
confirm / back / context / search
        ↓
FocusProvider, Library, footer          ← unchanged
```

It is applied in `readGamepads()` in `src/hooks/useControllerInput.ts`, the acquisition boundary, so
every reader — ownership selection *and* the state machine, on the poll path *and* on the ownership
layout effect — sees the same canonical layout. `gamepadAdapter.ts` carries no browser or device
identity, and neither does any focus, Library, footer, Search, Context, or `AppShell` code.

The normalization is a two-entry permutation of canonical indices 2 and 3 and nothing else.
`confirm` (0), `back` (1), the shoulders, the sticks, the D-pad, the guide button, and every axis are
already canonical on the affected pad and are passed through untouched, as are `index`, `id`,
`mapping`, and `connected` — this corrects a layout, it does not disguise a device. An unaffected
snapshot is returned as the very same object, so the canonical path allocates nothing per frame.

For requalification only, the hook also publishes `data-controller-layout="transposed-face-buttons"`
on the document element while an affected pad is active, so the operator can confirm the predicate
actually matched the pad in their hand without attaching a debugger to the WebView. Nothing
behavioural reads it.

### R.6 Why the canonical semantic mapping is unchanged

```text
canonical button 0 = confirm
canonical button 1 = back
canonical button 2 = context   (X / Square, the left face button)
canonical button 3 = search    (Y / Triangle, the upper face button)
```

Unchanged, and deliberately so. A global swap of 2 and 3 would have fixed this one pad by breaking
every correctly mapped one — Xbox-style pads, browsers that implement the canonical mapping, other
Linux/browser combinations, and any future platform. The semantic actions were not renamed to match
the broken raw indices either: `search` remains upper-face semantics and `context` remains left-face
semantics, which is what normalization restores.

The footer likewise still expresses semantic layout, not raw browser indices — `A/Cross` confirm,
`B/Circle` back, `X/Square` context, `Y/Triangle` search — and the internally Xbox-oriented glyph
naming is preserved.

After normalization the physical behaviour on the affected pad is:

```text
Cross     -> Confirm
Circle    -> Back
Square    -> Context
Triangle  -> Search
```

### R.7 Regression coverage added

`src/input/gamepadQuirks.test.ts` (24 tests):

- runtime detection: WebKitGTK/Linux recognized; Chromium on Linux, WebKit on macOS, Firefox on
  Linux, and a missing or empty user agent all unaffected;
- the predicate requires both halves, and covers both DualSense connection namings;
- **a correctly mapped Standard Gamepad keeps `0 → confirm`, `1 → back`, `2 → context`, `3 → search`**
  on the affected engine, and is returned as the identical object;
- **the qualified quirk**: raw Cross 0 → canonical confirm, raw Circle 1 → canonical back, raw Square
  3 → canonical context, raw Triangle 2 → canonical search;
- the D-pad, shoulders, guide, and all axes are untouched; `index`/`id`/`mapping`/`connected` are
  preserved; a pad reporting fewer buttons than the permutation names keeps its own button;
- slots and empty entries survive, and only the affected pad in a mixed pair is normalized;
- **a pad shaped like a real browser `Gamepad`**, whose fields are prototype getters rather than own
  properties, keeps the identity the adapter gates on and is still transposed. See R.10.

`src/hooks/useControllerInput.test.tsx` (10 new tests) presses **raw** indices through the real
acquisition path and asserts the semantic actions delivered — a canonical-index test could not prove
the boundary is in the path:

- all four raw face buttons produce the correct canonical actions;
- an Xbox-style pad on the same engine, and the same DualSense on Chromium, stay canonical;
- the diagnostic attribute appears and disappears with the affected pad;
- **ownership is unaffected**: a raw Triangle held across a loss and return of ownership is adopted
  silently and only fires when released and pressed again; the active pad keeps ownership when a
  second pad is plugged in; a disconnect releases and the replacement adopts rather than replaying.

`src/app/AppShell.test.tsx` (5 new tests) drive the whole application with raw physical indices:

- physical Triangle reaches Search from `zone:library-sidebar`, and Back restores the original
  sidebar entry;
- physical Triangle reaches Search from `zone:library-main`, and Back restores the original game card;
- physical Square selects the focused card without opening it;
- physical Cross opens the focused card;
- the footer still shows `X` for `SELECT` and `Y` for `SEARCH`.

Library zone containment was not weakened to achieve any of this: Search remains the same explicit
semantic zone exit it already was. The existing canonical-index controller, focus, zone,
accessibility, and Search tests are unchanged and still pass, which is what proves pointer, Tab,
Shift+Tab, typing, caret keys, and native focus behaviour did not regress.

### R.8 Runtime Release

No Runtime Release file changed in this pass. `release/linux-x86_64/runtime-release.json` and every
Release 001 and Release 002 artefact are byte-identical to the starting commit; the defect was in
browser input acquisition, which the managed runtime has no part in. The RetroArch controller profile
shipped in Release 002 is unrelated to this fix and is untouched — the controller already worked
correctly inside managed RetroArch.

### R.9 Remaining physical requalification — HUMAN REQUIRED

Automated tests cannot press a physical button. Every item below stays not performed until the
operator runs it on the qualification hardware after this patch.

```text
Sidebar:
  Triangle -> Search                              NOT PERFORMED — HUMAN INTERACTION REQUIRED
  B        -> original sidebar entry restored      NOT PERFORMED — HUMAN INTERACTION REQUIRED

Main Library:
  Triangle -> Search                              NOT PERFORMED — HUMAN INTERACTION REQUIRED
  B        -> original game card restored          NOT PERFORMED — HUMAN INTERACTION REQUIRED

Game card:
  Square   -> Context / checkbox selection         NOT PERFORMED — HUMAN INTERACTION REQUIRED

Regression:
  Cross    -> Confirm                              NOT PERFORMED — HUMAN INTERACTION REQUIRED
  Circle   -> Back                                 NOT PERFORMED — HUMAN INTERACTION REQUIRED

RetroArch:
  launch a game                                    NOT PERFORMED — HUMAN INTERACTION REQUIRED
  controller still works in-game                   NOT PERFORMED — HUMAN INTERACTION REQUIRED
  exit RetroArch                                   NOT PERFORMED — HUMAN INTERACTION REQUIRED
  RetroFrontier controller works again             NOT PERFORMED — HUMAN INTERACTION REQUIRED
```

If the mapping still behaves as before, the first thing to read is
`document.documentElement.dataset.controllerLayout`: `transposed-face-buttons` means the predicate
matched and the cause is elsewhere; absent means the predicate did not match, and the actual
`Gamepad.id` and `navigator.userAgent` strings are then the two values to capture.

### R.10 Corrective fix within this pass: the pad stopped working entirely

The first implementation of R.5 built the corrected snapshot with `{ ...snapshot }`. The operator
reported the controller not working in the frontend at all, and that spread was the cause.

A browser `Gamepad` exposes `index`, `id`, `mapping`, `connected`, `buttons`, and `axes` as
**prototype getters**, so it has no own enumerable properties: `{ ...gamepad }` copies *nothing*. The
affected DualSense — and only the affected pad, because unaffected pads are returned by identity —
came out of normalization with `mapping` and `connected` `undefined`. `isSupportedGamepad` then
correctly refused to interpret it, `selectActiveGamepad` never selected it, and the pad was treated as
absent: no actions at all, and the footer reporting no controller rather than an unsupported one. The
gate behaved exactly as designed; it was handed a snapshot that had lost its identity.

Every field is now read and assigned explicitly, and the buttons array is built with `Array.from` so
an array-like `FrozenArray` is handled too.

Why the original test suite missed it: every fake pad in the suite is a plain object literal whose
properties *are* own properties, so a spread copied them faithfully. The suite proved the
transposition arithmetic and never touched the shape the real API actually has.

`src/input/gamepadQuirks.test.ts` therefore gains a pad shaped like a real `Gamepad` — a class whose
six fields are prototype getters — asserting that it has no own enumerable properties, that
normalization preserves the `mapping`/`connected`/`id`/`index`/`axes` identity the adapter gates on,
and that the face-button transposition still happens. Reverting the fix fails that test with
`expected undefined to be 'standard'`, so it is a real guard rather than a restatement.

### R.11 Verdict for this pass

```text
M8 DUALSENSE NORMALIZATION PASS — READY FOR OPERATOR REQUALIFICATION
```

The defect is narrowly identified at the engine level, normalized at the acquisition boundary only,
regression-tested from the raw physical indices upward, and the canonical controller architecture —
semantics, adapter, footer, zones, and input ownership — is intact. M8 is **not** hardware-qualified
until the operator performs the Square/Triangle physical test in R.9.
