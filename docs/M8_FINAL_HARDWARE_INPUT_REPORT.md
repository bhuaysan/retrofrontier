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
| Local `HEAD` | `a16e10acb0b5835fb79ef05c6d6659748c09ba6d` |
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

`release_id`, `release_sequence`, `manifest_id`, `channel`, `minimum_safe_release_sequence`,
`app_run_path`, and every existing component's pin are unchanged. No release id or sequence semantics
were mutated. No manifest was hand-edited: every generated artefact in this report came out of
`rf-runtime-release` itself.

**Reinstallation is required.** The authenticated contents of the release changed, so the currently
installed `i-18d0a4fda8be7c01-1-293535` does not contain the profile component and its
`verified_launch_runtime` will now refuse to launch with `runtimeNotReady`. That refusal is the
intended behaviour, and the operator steps are in section L.

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

**Release-construction validation.** `rf-runtime-release build` was run against a reduced definition
containing the unchanged `retroarch` component plus the new `joypad-autoconfig` component, using the
verified pinned inputs. The real tool derived the artefact, matched it to its pin, built a 1 258-entry
inventory, validated the manifest with the client-side validator, and proved it by extracting through
the production extractor and running `verify_tree` and `validate_app_run`:

```text
target        joypad-autoconfig-1.22.0.tar   2336768  d81e3ac266d592b1732a7b16d77563aa513b270d2a5e592bf2040d633d6906cc
```

A build of the **complete** committed definition could not be completed, for a reason that predates
this pass: the four libretro **nightly** core URLs the definition pins have rolled upstream, so their
pinned lengths and digests no longer match what the server serves (`nestopia_libretro.so.zip` is now
724 254 bytes against a pinned 723 373). The RetroArch 7z and the new autoconfig zip both verify
exactly. Re-pinning the nightly cores would change what the release ships and is out of scope here;
it is recorded as a risk in section N.

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

1. Rebuild and republish the qualification Runtime Release from the updated definition:
   `rf-runtime-release publish --definition release/linux-x86_64/runtime-release.json --output <dir> --keys <keys-dir>`.
   **This will fail on the four nightly core inputs until they are re-pinned** (section L) — treat
   that re-pin as a separate reviewed decision.
2. Install through RetroFrontier's own Settings → managed runtime action, not by hand.
3. Confirm the new installation reports `Ready`, and that
   `runtime/versions/<new-installation>/runtime/support/joypad-autoconfig/udev/` contains the
   DualSense profile.

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
- **The release cannot currently be rebuilt end to end.** Four nightly core inputs have rolled
  upstream and no longer match their pins. This predates this pass and blocks any full
  reconstruction, not only this one. It needs a separate reviewed re-pin — and it is an argument for
  moving the cores to immutable revisions the way the profile database now is.
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

## P. Verdict

```text
M8 FINAL HARDWARE INPUT PASS — READY FOR OPERATOR REQUALIFICATION
```

Both findings are fixed, both fixes are covered by regression tests, and the RetroArch fix is
demonstrated on the real managed binary with the real physical DualSense: `not configured` before,
`configured in Port 1` after. The physical controller must not be called fixed until the operator
runs the real in-game test in section N.2.
