# ADR-014: Browser Gamepad API behind a replaceable input-acquisition boundary

- Status: Accepted

## Context

ADR-008 accepted controller navigation as a foundational capability and fixed the semantic action
vocabulary. It did not decide *where* physical controller input is read.

M8 has two candidate acquisition points:

1. the WebView's **Gamepad API**, polled from the React application, and
2. a **Rust-native global input listener** (evdev/udev on Linux, XInput on Windows, GameController
   on macOS) delivering events over IPC.

The native listener is the more powerful option: it can read a controller while the application
window is unfocused, and it is not bound to a browser engine's device support. It is also
substantially more expensive. It needs a per-platform device backend, device permissions
(`/dev/input` access and udev rules on Linux), hotplug handling, its own mapping database for pads
that do not follow a standard layout, and a new always-on IPC event stream. Every one of those is a
cross-platform surface RetroFrontier would own for the rest of V1.

Crucially, the extra power it buys is power M8 must not use. While a managed game runs, RetroArch —
not RetroFrontier — owns the controller, and while the RetroFrontier window is unfocused, another
application owns it. Reading input in those states is precisely what M8 forbids.

## Decision

Acquire controller input through the **browser Gamepad API**, behind an explicit acquisition
boundary.

The boundary is the module that produces `InputAction` values. Everything above it — the focus
registry, spatial navigation, focus scopes, the footer — consumes semantic actions only and knows
nothing about gamepads, button indices, axes, deadzones, or polling. Replacing the acquisition
adapter is therefore a change to one module plus its hook, with no change to focus or navigation
code and no change to any component.

The browser adapter is chosen because it satisfies every M8 requirement with no new platform
surface: the Standard Gamepad mapping normalizes the common pads, hotplug is reported by the
browser, and the API is *only* readable while the page is live, which matches the ownership model
M8 must enforce anyway.

Physical button indices and the analogue policy live in exactly one file, so a native adapter would
replace a known, tested contract rather than a scattering of assumptions.

### What would justify replacing it

- The Gamepad API in the shipped WebView does not see a controller that the operating system does
  see, on a platform RetroFrontier must support.
- A future feature genuinely requires input while the RetroFrontier window is unfocused — a global
  "return to library" hotkey, for instance — which the browser API cannot provide by design.
- Per-controller remapping (B10) needs identity or capability information the browser does not
  expose.

None of these is established. B10 remapping is explicitly outside M8, and no cross-platform
qualification has yet contradicted the browser adapter.

## Consequences

M8 ships without any new native input dependency, device permission, or IPC stream, and the
ownership rules are enforced where the actions are produced. The cost is that the WebView's device
support is now a dependency: a pad the engine does not recognize is invisible to RetroFrontier even
though the emulator may still use it perfectly well, because RetroArch reads the device directly.
That failure mode is confined to *navigating the frontend*, never to playing a game.

Controller support therefore remains a per-platform qualification item. Only Linux x86_64 with a
DualSense is in scope for M8's qualification; Windows and macOS remain unproven.
