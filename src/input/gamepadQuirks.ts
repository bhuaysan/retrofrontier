import { selectActiveGamepad, type GamepadSnapshot } from './gamepadAdapter';

/**
 * Browser/device layout quirk normalization, the one place a physical layout that does **not** match
 * the W3C Standard Gamepad mapping is corrected before anything semantic reads it.
 *
 * ```text
 * navigator.getGamepads()  →  quirk normalization  →  canonical Standard Gamepad  →  gamepadAdapter
 * ```
 *
 * `gamepadAdapter` and everything above it keep reasoning in canonical Standard Gamepad indices
 * only; no device or browser identity exists there, and none exists in focus or UI code. A quirk
 * that cannot be recognized here is never guessed at: the snapshot passes through untouched.
 */

/** The browser engine acquiring gamepad input, to the extent it changes the physical layout. */
export type InputRuntime = 'webkitgtk-linux' | 'other';

/**
 * Recognizes WebKitGTK on Linux — the engine RetroFrontier's own Linux Tauri WebView runs on.
 *
 * The affected translation lives in the engine, not in the application, so this matches the engine
 * rather than the packaging: the same defect is present in any WebKitGTK host on Linux. Chromium
 * advertises `AppleWebKit/` too and is excluded explicitly, so a Chrome/Chromium development browser
 * on the same machine keeps the canonical path.
 *
 * WebKitGTK 2.52.5 reports:
 * `Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/60.5 Safari/605.1.15`
 */
export function detectInputRuntime(userAgent: string | undefined): InputRuntime {
  if (userAgent === undefined || userAgent === '') return 'other';
  if (!/AppleWebKit\//.test(userAgent)) return 'other';
  if (!/\bLinux\b/.test(userAgent)) return 'other';
  if (/\b(?:Chrome|Chromium|CriOS|Edg)\//.test(userAgent)) return 'other';
  return 'webkitgtk-linux';
}

/** Reads the live browser identity. Deliberately not cached at module scope, so tests can stub it. */
export function currentInputRuntime(): InputRuntime {
  return detectInputRuntime(
    typeof navigator === 'undefined' ? undefined : (navigator.userAgent as string | undefined),
  );
}

/**
 * Devices whose face buttons arrive transposed on {@link detectInputRuntime} `webkitgtk-linux`.
 *
 * Matches the Linux kernel device name, because that string is the *whole* of what WebKitGTK puts in
 * `Gamepad.id`: its Gamepad backend fills the id from `manette_device_get_name()` and links no
 * libmanette vendor/product accessor at all. No vendor or product id is available to match on, so
 * the name is the only identity there is. Both connection namings carry the token — USB reports
 * `Sony Interactive Entertainment DualSense Wireless Controller`, Bluetooth `DualSense Wireless
 * Controller` — so one token covers both without widening to other devices.
 *
 * Deliberately **not** widened to all PlayStation pads or to Linux as a whole: the transposition is
 * a property of how a specific driver's raw button codes reach WebKitGTK's translation table, and
 * only the DualSense has been physically measured. See `docs/CONTROLLER_AND_FOCUS.md`.
 */
const TRANSPOSED_FACE_BUTTON_DEVICE = /dualsense/i;

/**
 * Canonical index → the raw index it must be read from on an affected pad.
 *
 * Exactly the two upper/left face buttons, and nothing else. `confirm` (0), `back` (1), the
 * shoulders, the sticks, the D-pad, and every axis are already canonical on the affected pad and are
 * left strictly alone.
 *
 * Why these two: on Linux `BTN_X` and `BTN_NORTH` are the same code `0x133`, and `BTN_Y` and
 * `BTN_WEST` the same code `0x134`. WebKitGTK's translation table reads them under their *letter*
 * meaning — `BTN_X → 2`, `BTN_Y → 3` — while the DualSense's kernel driver emits them under their
 * *positional* meaning, north (Triangle) as `0x133` and west (Square) as `0x134`. The two therefore
 * arrive swapped, and `Gamepad.mapping` still says `standard`.
 */
const TRANSPOSED_FACE_BUTTONS: ReadonlyMap<number, number> = new Map([
  [2, 3],
  [3, 2],
]);

/** The quirk applied to a snapshot, or `null` when it is already canonical. */
export type GamepadLayoutQuirk = 'transposed-face-buttons' | null;

/**
 * Whether this exact runtime *and* device pair is the qualified quirk.
 *
 * Both halves are required. A DualSense on a correctly translating engine is untouched, and any
 * other pad on WebKitGTK/Linux is untouched.
 */
export function gamepadLayoutQuirk(
  snapshot: GamepadSnapshot,
  runtime: InputRuntime,
): GamepadLayoutQuirk {
  if (runtime !== 'webkitgtk-linux') return null;
  if (!TRANSPOSED_FACE_BUTTON_DEVICE.test(snapshot.id)) return null;
  return 'transposed-face-buttons';
}

/**
 * Returns the snapshot in the canonical Standard Gamepad layout.
 *
 * The same object is returned when no quirk applies, so the canonical path allocates nothing. Index,
 * id, mapping, connected, and axes are always preserved exactly: this corrects a layout, it does not
 * disguise a device. A pad reporting fewer buttons than the transposition names keeps its own button
 * at that position rather than losing it.
 *
 * Every field is read and assigned **explicitly**, never spread from the source. A browser `Gamepad`
 * exposes its properties as prototype getters rather than own properties, so `{ ...gamepad }` copies
 * *nothing*: it would hand back a snapshot whose `mapping` and `connected` are `undefined`, which the
 * adapter would correctly reject as uninterpretable, and the pad would stop working altogether
 * instead of being corrected. Reading through the getters is the whole point of doing it this way.
 */
export function normalizeGamepadSnapshot(
  snapshot: GamepadSnapshot,
  runtime: InputRuntime,
): GamepadSnapshot {
  if (gamepadLayoutQuirk(snapshot, runtime) === null) return snapshot;
  const buttons = snapshot.buttons;
  return {
    index: snapshot.index,
    id: snapshot.id,
    mapping: snapshot.mapping,
    connected: snapshot.connected,
    axes: snapshot.axes,
    buttons: Array.from(buttons, (button, index) => {
      const source = TRANSPOSED_FACE_BUTTONS.get(index);
      if (source === undefined) return button;
      return buttons[source] ?? button;
    }),
  };
}

/** Normalizes a whole `navigator.getGamepads()` result, preserving its slots and empty entries. */
export function normalizeGamepads(
  pads: readonly (GamepadSnapshot | null)[],
  runtime: InputRuntime,
): (GamepadSnapshot | null)[] {
  return pads.map((snapshot) =>
    snapshot === null ? null : normalizeGamepadSnapshot(snapshot, runtime),
  );
}

/**
 * The `Gamepad.id` of the one controller RetroFrontier currently accepts, or `null` when none is
 * connected or supported.
 *
 * A one-shot read for the moment a launch or a save-state load is issued — it is deliberately not
 * a subscription: nothing here holds ownership state across calls, unlike `useControllerInput`.
 * The identity is sent to the backend so save-state hotkey derivation can require proof of the
 * actual controller before ever trusting the qualified profile database alone (MEDIUM-2); nothing
 * else reads it.
 */
export function activeControllerIdentity(): string | null {
  const source =
    typeof navigator === 'undefined' ? undefined : navigator.getGamepads?.bind(navigator);
  if (source === undefined) return null;
  const pads = normalizeGamepads(
    Array.from(source()) as (GamepadSnapshot | null)[],
    currentInputRuntime(),
  );
  return selectActiveGamepad(pads, null)?.id ?? null;
}
