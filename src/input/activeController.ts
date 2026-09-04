import type { GamepadSnapshot } from './gamepadAdapter';

/**
 * The one semantic source of truth for *which controller RetroFrontier currently accepts input
 * from* (MEDIUM-2).
 *
 * There is exactly one ownership decision in the application, and `useControllerInput` makes it:
 * `selectActiveGamepad` is called there with the ownership index the hook has been carrying, so a
 * pad that already owns input keeps it when another pad is plugged in at a lower index. That
 * retained ownership is real state, and it lives only inside the hook.
 *
 * Anything else that needs to know *who* the active controller is must read it from here rather
 * than selecting again. A second, independent `selectActiveGamepad(pads, null)` call — which is
 * what launch and save-state loads used to do — deliberately ignores retained ownership and
 * re-picks the lowest connected index, so it can name a completely different pad:
 *
 * ```text
 * index 1  Xbox pad, plugged in first, currently owns RetroFrontier input
 * index 0  DualSense, plugged in later
 *
 * useControllerInput ownership   → Xbox (index 1)
 * selectActiveGamepad(pads,null) → DualSense (index 0)   ← a different controller
 * ```
 *
 * The backend then wrote DualSense raw hotkey button numbers for a launch the player is driving
 * with an Xbox pad. Publishing the hook's own answer here makes both readers the same reader.
 */
export interface ActiveControllerOwner {
  /** The `Gamepad.index` that currently owns RetroFrontier input. */
  index: number;
  /** That exact pad's `Gamepad.id`. */
  id: string;
}

/**
 * Module state on purpose, not React state.
 *
 * The owner changes on every polled animation frame and nothing renders from it, so putting it in
 * React state would re-render the whole shell at 60Hz to serve a value only read at the instant a
 * launch is issued. It is written from exactly one place — `useControllerInput`'s poll — and read
 * from exactly one place, `activeControllerIdentity`.
 */
let owner: ActiveControllerOwner | null = null;

/**
 * Publish the controller that currently owns input, or `null` when none does.
 *
 * Called by `useControllerInput` with the very snapshot its ownership selection produced, so the
 * published identity can never describe a different pad than the one driving navigation. It is
 * also called with `null` when the hook unmounts: an unmounted acquisition boundary owns nothing,
 * and a stale identity must never outlive the loop that proved it.
 */
export function publishActiveControllerOwner(snapshot: GamepadSnapshot | null): void {
  owner = snapshot === null ? null : { index: snapshot.index, id: snapshot.id };
}

/** The controller that currently owns RetroFrontier input, or `null` when none does. */
export function activeControllerOwner(): ActiveControllerOwner | null {
  return owner;
}

/** Test-only reset, so one test's published owner cannot leak into the next. */
export function resetActiveControllerOwner(): void {
  owner = null;
}
