import type { DirectionalAction, InputAction } from './actions';

/**
 * The one place physical gamepad button indices exist.
 *
 * Indices follow the W3C Standard Gamepad mapping, and M8 accepts **only** pads the browser
 * normalizes to that mapping. Reading these indices from a pad whose layout the browser could not
 * normalize would produce arbitrary actions from arbitrary buttons; remapping is out of M8 scope,
 * so such a pad stays connected-but-unsupported instead.
 */
export const GAMEPAD_BUTTON_INDEX = {
  confirm: 0,
  back: 1,
  context: 2,
  dpadUp: 12,
  dpadDown: 13,
  dpadLeft: 14,
  dpadRight: 15,
} as const;

/** Left-stick axes in the Standard Gamepad mapping. */
export const GAMEPAD_AXIS_INDEX = { horizontal: 0, vertical: 1 } as const;

/**
 * Directional policy.
 *
 * `enterDeadzone` and the lower `exitDeadzone` form the hysteresis band: once a direction is held,
 * the stick has to fall well back towards centre before the direction is released, so jitter around
 * one threshold cannot repeatedly enter and leave a direction. Repeat timing is UI-paced, not
 * frame-paced: one action on press, then a pause, then a bounded interval.
 */
export const GAMEPAD_TUNING = {
  enterDeadzone: 0.55,
  exitDeadzone: 0.35,
  /**
   * How far the other axis must exceed the currently held axis before the direction switches.
   *
   * This is the dominance counterpart to the deadzone hysteresis. Without it a stick resting near
   * the diagonal alternates direction whenever `|x|` and `|y|` cross, which emits a burst of
   * conflicting movement; with it, a held axis keeps dominance until the other axis wins clearly.
   * It also gives the exact-tie case a deterministic answer: the held axis stays.
   */
  axisDominanceMargin: 0.15,
  initialRepeatDelayMs: 400,
  repeatIntervalMs: 110,
} as const;

/**
 * The mapping contract M8 can interpret.
 *
 * `standard` is the W3C Standard Gamepad mapping the browser has already normalized to a known
 * button and axis layout. Anything else exposes a device-specific layout that `GAMEPAD_BUTTON_INDEX`
 * would misread.
 */
export function isSupportedGamepad(snapshot: GamepadSnapshot | null): snapshot is GamepadSnapshot {
  return snapshot !== null && snapshot.connected && snapshot.mapping === 'standard';
}

/** A pad is attached that RetroFrontier cannot interpret. Reported honestly, never as usable. */
export function hasUnsupportedGamepad(pads: readonly (GamepadSnapshot | null)[]): boolean {
  return pads.some(
    (snapshot) => snapshot !== null && snapshot.connected && snapshot.mapping !== 'standard',
  );
}

export interface GamepadButtonLike {
  pressed: boolean;
}

/** The structural subset of the browser `Gamepad` object this adapter reads. */
export interface GamepadSnapshot {
  index: number;
  id: string;
  mapping: string;
  connected: boolean;
  buttons: readonly GamepadButtonLike[];
  axes: readonly number[];
}

interface ActivationState {
  confirm: boolean;
  back: boolean;
  context: boolean;
}

export interface GamepadState {
  /** The controller whose input is currently accepted, or `null` when none is active. */
  activeIndex: number | null;
  /**
   * The next processed frame only records what is physically held, without emitting. It is set
   * after every ownership change so an input held across the change cannot fire a stale action.
   */
  adopting: boolean;
  direction: DirectionalAction | null;
  /** A direction adopted while already held is not armed, so it never repeats until re-pressed. */
  directionArmed: boolean;
  nextRepeatAt: number;
  buttons: ActivationState;
}

const NO_BUTTONS: ActivationState = { confirm: false, back: false, context: false };

export function createGamepadState(): GamepadState {
  return {
    activeIndex: null,
    adopting: true,
    direction: null,
    directionArmed: false,
    nextRepeatAt: 0,
    buttons: { ...NO_BUTTONS },
  };
}

/**
 * Drops every held/repeat state and re-adopts on the next frame.
 *
 * Used when RetroFrontier stops owning controller input — the window lost focus, or a managed game
 * became authoritative — so returning ownership cannot replay whatever was held meanwhile.
 */
export function releaseGamepadOwnership(state: GamepadState): GamepadState {
  return {
    ...state,
    adopting: true,
    direction: null,
    directionArmed: false,
    nextRepeatAt: 0,
    buttons: { ...NO_BUTTONS },
  };
}

/**
 * Picks the one controller whose input is accepted.
 *
 * Only pads whose mapping RetroFrontier can interpret are eligible, so a non-standard pad at a low
 * index can never take ownership from a usable Standard-mapped pad at a higher index. Among those,
 * the currently active controller keeps ownership while it stays connected, so plugging in a second
 * pad never moves control or duplicates actions. Otherwise the lowest connected index wins, which
 * makes the choice deterministic rather than dependent on enumeration order.
 */
export function selectActiveGamepad(
  pads: readonly (GamepadSnapshot | null)[],
  activeIndex: number | null,
): GamepadSnapshot | null {
  const connected = pads.filter(isSupportedGamepad);
  if (connected.length === 0) return null;
  const active = connected.find((padSnapshot) => padSnapshot.index === activeIndex);
  if (active !== undefined) return active;
  return connected.reduce((lowest, candidate) =>
    candidate.index < lowest.index ? candidate : lowest,
  );
}

function pressed(snapshot: GamepadSnapshot, index: number): boolean {
  return snapshot.buttons[index]?.pressed === true;
}

function digitalDirection(snapshot: GamepadSnapshot): DirectionalAction | null {
  if (pressed(snapshot, GAMEPAD_BUTTON_INDEX.dpadUp)) return 'moveUp';
  if (pressed(snapshot, GAMEPAD_BUTTON_INDEX.dpadDown)) return 'moveDown';
  if (pressed(snapshot, GAMEPAD_BUTTON_INDEX.dpadLeft)) return 'moveLeft';
  if (pressed(snapshot, GAMEPAD_BUTTON_INDEX.dpadRight)) return 'moveRight';
  return null;
}

function analogueDirection(
  snapshot: GamepadSnapshot,
  held: DirectionalAction | null,
): DirectionalAction | null {
  const x = snapshot.axes[GAMEPAD_AXIS_INDEX.horizontal] ?? 0;
  const y = snapshot.axes[GAMEPAD_AXIS_INDEX.vertical] ?? 0;
  const horizontalHeld = held === 'moveLeft' || held === 'moveRight';
  const verticalHeld = held === 'moveUp' || held === 'moveDown';
  const { enterDeadzone, exitDeadzone } = GAMEPAD_TUNING;

  const magnitudeX = Math.abs(x);
  const magnitudeY = Math.abs(y);
  const activeX = magnitudeX >= (horizontalHeld ? exitDeadzone : enterDeadzone);
  const activeY = magnitudeY >= (verticalHeld ? exitDeadzone : enterDeadzone);

  if (!activeX && !activeY) return null;
  if (!activeY) return x > 0 ? 'moveRight' : 'moveLeft';
  if (!activeX) return y > 0 ? 'moveDown' : 'moveUp';

  // Both axes are deflected. Exactly one direction is produced, chosen deterministically:
  //  - a held axis keeps dominance until the other axis exceeds it by the dominance margin, which
  //    covers the exact 45° tie and stops noise around the diagonal alternating directions;
  //  - with nothing held, the larger deflection wins and an exact tie resolves to the horizontal
  //    axis, the documented fixed priority.
  const { axisDominanceMargin } = GAMEPAD_TUNING;
  const horizontal = () => (x > 0 ? 'moveRight' : 'moveLeft');
  const vertical = () => (y > 0 ? 'moveDown' : 'moveUp');
  if (horizontalHeld) {
    return magnitudeY > magnitudeX + axisDominanceMargin ? vertical() : horizontal();
  }
  if (verticalHeld) {
    return magnitudeX > magnitudeY + axisDominanceMargin ? horizontal() : vertical();
  }
  return magnitudeX >= magnitudeY ? horizontal() : vertical();
}

export interface GamepadStepResult {
  state: GamepadState;
  actions: readonly InputAction[];
}

/**
 * Advances the deterministic gamepad state machine by one polled frame.
 *
 * `snapshot` is the active controller, or `null` when none is connected. `now` is a monotonic
 * millisecond clock. At most one directional action is produced per frame, so a stalled frame
 * cannot emit a burst of movement.
 */
export function stepGamepad(
  state: GamepadState,
  snapshot: GamepadSnapshot | null,
  now: number,
): GamepadStepResult {
  // A pad whose mapping was not normalized to the Standard Gamepad layout is treated exactly like
  // no pad at all: its button indices and axes mean something else, so nothing may be derived from
  // them. `selectActiveGamepad` already filters these out; this is the same policy enforced at the
  // one place that reads indices, so no caller can bypass it.
  if (!isSupportedGamepad(snapshot)) {
    return { state: { ...releaseGamepadOwnership(state), activeIndex: null }, actions: [] };
  }

  const ownershipChanged = state.activeIndex !== snapshot.index;
  const base = ownershipChanged
    ? { ...releaseGamepadOwnership(state), activeIndex: snapshot.index }
    : state;
  const adopting = base.adopting;

  const actions: InputAction[] = [];
  const heldDirection = digitalDirection(snapshot) ?? analogueDirection(snapshot, base.direction);

  let direction = base.direction;
  let directionArmed = base.directionArmed;
  let nextRepeatAt = base.nextRepeatAt;

  if (adopting) {
    direction = heldDirection;
    directionArmed = false;
    nextRepeatAt = 0;
  } else if (heldDirection === null) {
    direction = null;
    directionArmed = false;
    nextRepeatAt = 0;
  } else if (heldDirection !== base.direction) {
    actions.push(heldDirection);
    direction = heldDirection;
    directionArmed = true;
    nextRepeatAt = now + GAMEPAD_TUNING.initialRepeatDelayMs;
  } else if (directionArmed && now >= nextRepeatAt) {
    actions.push(heldDirection);
    nextRepeatAt = now + GAMEPAD_TUNING.repeatIntervalMs;
  }

  const buttons: ActivationState = {
    confirm: pressed(snapshot, GAMEPAD_BUTTON_INDEX.confirm),
    back: pressed(snapshot, GAMEPAD_BUTTON_INDEX.back),
    context: pressed(snapshot, GAMEPAD_BUTTON_INDEX.context),
  };
  if (!adopting) {
    if (buttons.confirm && !base.buttons.confirm) actions.push('confirm');
    if (buttons.back && !base.buttons.back) actions.push('back');
    if (buttons.context && !base.buttons.context) actions.push('context');
  }

  return {
    state: {
      activeIndex: snapshot.index,
      adopting: false,
      direction,
      directionArmed,
      nextRepeatAt,
      buttons,
    },
    actions,
  };
}
