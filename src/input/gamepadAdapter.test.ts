import { describe, expect, it } from 'vitest';

import {
  GAMEPAD_BUTTON_INDEX,
  GAMEPAD_TUNING,
  createGamepadState,
  hasUnsupportedGamepad,
  releaseGamepadOwnership,
  selectActiveGamepad,
  stepGamepad,
  type GamepadSnapshot,
} from './gamepadAdapter';

function pad(overrides: Partial<GamepadSnapshot> = {}): GamepadSnapshot {
  return {
    index: 0,
    id: 'Test Pad (STANDARD GAMEPAD)',
    mapping: 'standard',
    connected: true,
    buttons: Array.from({ length: 17 }, () => ({ pressed: false })),
    axes: [0, 0, 0, 0],
    ...overrides,
  };
}

function withButton(index: number, snapshot: GamepadSnapshot = pad()): GamepadSnapshot {
  const buttons = snapshot.buttons.map((button, i) => ({ pressed: i === index || button.pressed }));
  return { ...snapshot, buttons };
}

function withAxes(x: number, y: number, snapshot: GamepadSnapshot = pad()): GamepadSnapshot {
  return { ...snapshot, axes: [x, y, 0, 0] };
}

/** Drives the machine from a fresh state, discarding the adoption step's (empty) output. */
function run(steps: readonly { snapshot: GamepadSnapshot | null; now: number }[]) {
  let state = createGamepadState();
  const emitted: string[][] = [];
  for (const step of steps) {
    const result = stepGamepad(state, step.snapshot, step.now);
    state = result.state;
    emitted.push([...result.actions]);
  }
  return { state, emitted };
}

describe('selectActiveGamepad', () => {
  it('selects the lowest connected index when nothing is active yet', () => {
    const pads = [null, pad({ index: 1 }), pad({ index: 2 })];
    expect(selectActiveGamepad(pads, null)?.index).toBe(1);
  });

  it('keeps the active controller while it stays connected', () => {
    const pads = [pad({ index: 0 }), pad({ index: 1 })];
    expect(selectActiveGamepad(pads, 1)?.index).toBe(1);
  });

  it('falls back deterministically when the active controller disappears', () => {
    expect(selectActiveGamepad([pad({ index: 0 }), null], 1)?.index).toBe(0);
    expect(selectActiveGamepad([null, null], 1)).toBeNull();
    expect(selectActiveGamepad([pad({ index: 0, connected: false })], null)).toBeNull();
  });

  it('never selects a pad whose mapping contract cannot be interpreted', () => {
    expect(selectActiveGamepad([pad({ index: 0, mapping: '' })], null)).toBeNull();
    expect(selectActiveGamepad([pad({ index: 0, mapping: 'xr-standard' })], null)).toBeNull();
  });

  it('does not let a non-standard low-index pad block a usable standard pad', () => {
    const pads = [pad({ index: 0, mapping: '' }), pad({ index: 1 })];
    expect(selectActiveGamepad(pads, null)?.index).toBe(1);
  });

  it('drops an active pad that stops reporting a standard mapping', () => {
    expect(selectActiveGamepad([pad({ index: 0, mapping: '' }), pad({ index: 1 })], 0)?.index).toBe(
      1,
    );
  });

  it('reports connected-but-unsupported pads honestly', () => {
    expect(hasUnsupportedGamepad([pad({ index: 0, mapping: '' })])).toBe(true);
    expect(hasUnsupportedGamepad([pad({ index: 0 })])).toBe(false);
    expect(hasUnsupportedGamepad([pad({ index: 0, mapping: '', connected: false })])).toBe(false);
    expect(hasUnsupportedGamepad([])).toBe(false);
  });
});

describe('stepGamepad mapping policy', () => {
  it('dispatches nothing for a pad that does not use the Standard Gamepad mapping', () => {
    const nonStandard = withButton(GAMEPAD_BUTTON_INDEX.confirm, pad({ mapping: '' }));
    const { emitted } = run([
      { snapshot: pad({ mapping: '' }), now: 0 },
      { snapshot: nonStandard, now: 16 },
      { snapshot: withAxes(0.9, 0, pad({ mapping: '' })), now: 32 },
      { snapshot: withButton(GAMEPAD_BUTTON_INDEX.dpadDown, pad({ mapping: '' })), now: 48 },
    ]);
    expect(emitted).toEqual([[], [], [], []]);
  });
});

describe('stepGamepad digital directions', () => {
  it('emits one action for each D-pad direction on press', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withButton(GAMEPAD_BUTTON_INDEX.dpadUp), now: 16 },
      { snapshot: withButton(GAMEPAD_BUTTON_INDEX.dpadDown), now: 32 },
      { snapshot: withButton(GAMEPAD_BUTTON_INDEX.dpadLeft), now: 48 },
      { snapshot: withButton(GAMEPAD_BUTTON_INDEX.dpadRight), now: 64 },
    ]);
    expect(emitted).toEqual([[], ['moveUp'], ['moveDown'], ['moveLeft'], ['moveRight']]);
  });

  it('does not repeat before the initial repeat delay elapses', () => {
    const held = withButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: held, now: 0 },
      { snapshot: held, now: GAMEPAD_TUNING.initialRepeatDelayMs - 1 },
    ]);
    expect(emitted).toEqual([[], ['moveDown'], []]);
  });

  it('repeats once after the initial delay and then on the repeat interval', () => {
    const held = withButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    const { delay, interval } = {
      delay: GAMEPAD_TUNING.initialRepeatDelayMs,
      interval: GAMEPAD_TUNING.repeatIntervalMs,
    };
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: held, now: 0 },
      { snapshot: held, now: delay },
      { snapshot: held, now: delay + interval - 1 },
      { snapshot: held, now: delay + interval },
      { snapshot: held, now: delay + 2 * interval },
    ]);
    expect(emitted).toEqual([[], ['moveDown'], ['moveDown'], [], ['moveDown'], ['moveDown']]);
  });

  it('never emits more than one directional action per polled frame', () => {
    const held = withButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: held, now: 0 },
      { snapshot: held, now: 100_000 },
    ]);
    expect(emitted[2]).toEqual(['moveDown']);
  });

  it('resets the repeat state when the direction changes', () => {
    const down = withButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    const up = withButton(GAMEPAD_BUTTON_INDEX.dpadUp);
    const delay = GAMEPAD_TUNING.initialRepeatDelayMs;
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: down, now: 0 },
      { snapshot: up, now: 10 },
      { snapshot: up, now: 10 + delay - 1 },
      { snapshot: up, now: 10 + delay },
    ]);
    expect(emitted).toEqual([[], ['moveDown'], ['moveUp'], [], ['moveUp']]);
  });

  it('resets the repeat state on release, so a re-press acts immediately', () => {
    const down = withButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    const delay = GAMEPAD_TUNING.initialRepeatDelayMs;
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: down, now: 0 },
      { snapshot: pad(), now: 10 },
      { snapshot: down, now: 20 },
      { snapshot: down, now: 20 + delay - 1 },
    ]);
    expect(emitted).toEqual([[], ['moveDown'], [], ['moveDown'], []]);
  });
});

describe('stepGamepad analogue handling', () => {
  it('ignores stick movement inside the deadzone', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(0.2, 0), now: 16 },
      { snapshot: withAxes(0, -0.3), now: 32 },
    ]);
    expect(emitted).toEqual([[], [], []]);
  });

  it('enters a direction above the enter threshold', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(GAMEPAD_TUNING.enterDeadzone, 0), now: 16 },
      { snapshot: withAxes(0, -GAMEPAD_TUNING.enterDeadzone), now: 32 },
    ]);
    expect(emitted).toEqual([[], ['moveRight'], ['moveUp']]);
  });

  it('holds a direction through jitter between the exit and enter thresholds', () => {
    const { enterDeadzone, exitDeadzone, initialRepeatDelayMs } = GAMEPAD_TUNING;
    const between = (enterDeadzone + exitDeadzone) / 2;
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(enterDeadzone, 0), now: 0 },
      { snapshot: withAxes(between, 0), now: 10 },
      { snapshot: withAxes(enterDeadzone, 0), now: 20 },
      { snapshot: withAxes(between, 0), now: 30 },
      { snapshot: withAxes(between, 0), now: initialRepeatDelayMs },
    ]);
    // One action on entry, nothing while it jitters, then the ordinary held repeat.
    expect(emitted).toEqual([[], ['moveRight'], [], [], [], ['moveRight']]);
  });

  it('releases the direction below the exit threshold', () => {
    const { enterDeadzone, exitDeadzone } = GAMEPAD_TUNING;
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(enterDeadzone, 0), now: 0 },
      { snapshot: withAxes(exitDeadzone - 0.05, 0), now: 10 },
      { snapshot: withAxes(enterDeadzone, 0), now: 20 },
    ]);
    expect(emitted).toEqual([[], ['moveRight'], [], ['moveRight']]);
  });

  it('uses the dominant axis so a diagonal never fires two directions', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(0.9, -0.7), now: 16 },
      { snapshot: withAxes(0, 0), now: 32 },
      { snapshot: withAxes(0.7, -0.9), now: 48 },
    ]);
    expect(emitted).toEqual([[], ['moveRight'], [], ['moveUp']]);
  });

  // Replaces an earlier assertion that a perfect 45° diagonal emitted nothing. A dead direction at
  // exactly equal deflection is not deterministic tie *behaviour*, it is the absence of behaviour,
  // and M8 required a deterministic tie-break.
  it('breaks a perfect diagonal deterministically towards the documented axis priority', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(0.8, 0.8), now: 16 },
    ]);
    expect(emitted).toEqual([[], ['moveRight']]);

    const negative = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(-0.8, -0.8), now: 16 },
    ]);
    expect(negative.emitted).toEqual([[], ['moveLeft']]);
  });

  it('keeps a held horizontal direction when the deflection becomes an exact tie', () => {
    const { delay } = { delay: GAMEPAD_TUNING.initialRepeatDelayMs };
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(0.9, 0.1), now: 0 },
      { snapshot: withAxes(0.8, 0.8), now: 10 },
      { snapshot: withAxes(0.8, 0.8), now: delay },
    ]);
    // Entry, then the tie holds the same direction rather than switching or dying, and the ordinary
    // held repeat continues on the same action.
    expect(emitted).toEqual([[], ['moveRight'], [], ['moveRight']]);
  });

  it('keeps a held vertical direction when the deflection becomes an exact tie', () => {
    const { delay } = { delay: GAMEPAD_TUNING.initialRepeatDelayMs };
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(0.1, -0.9), now: 0 },
      { snapshot: withAxes(-0.8, -0.8), now: 10 },
      { snapshot: withAxes(-0.8, -0.8), now: delay },
    ]);
    expect(emitted).toEqual([[], ['moveUp'], [], ['moveUp']]);
  });

  it('does not storm actions when the two axes cross over by a small amount', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(0.72, 0.7), now: 0 },
      { snapshot: withAxes(0.7, 0.72), now: 10 },
      { snapshot: withAxes(0.72, 0.7), now: 20 },
      { snapshot: withAxes(0.7, 0.73), now: 30 },
      { snapshot: withAxes(0.71, 0.7), now: 40 },
    ]);
    // One entry action, then nothing: a held axis keeps dominance until the other axis wins by a
    // real margin, so noise around the diagonal cannot alternate directions frame by frame.
    expect(emitted).toEqual([[], ['moveRight'], [], [], [], []]);
  });

  it('still switches axis when the other axis genuinely dominates', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withAxes(0.8, 0.6), now: 0 },
      { snapshot: withAxes(0.6, 0.95), now: 10 },
    ]);
    expect(emitted).toEqual([[], ['moveRight'], ['moveDown']]);
  });

  it('lets the D-pad win over a simultaneously deflected stick', () => {
    const snapshot = withAxes(0.9, 0, withButton(GAMEPAD_BUTTON_INDEX.dpadUp));
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot, now: 16 },
    ]);
    expect(emitted).toEqual([[], ['moveUp']]);
  });
});

describe('stepGamepad activation buttons', () => {
  it('edge-triggers confirm, back, and context exactly once per press', () => {
    const confirm = withButton(GAMEPAD_BUTTON_INDEX.confirm);
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: confirm, now: 16 },
      { snapshot: confirm, now: 32 },
      { snapshot: confirm, now: 5_000 },
      { snapshot: pad(), now: 5_016 },
      { snapshot: confirm, now: 5_032 },
    ]);
    expect(emitted).toEqual([[], ['confirm'], [], [], [], ['confirm']]);
  });

  it('maps the standard back and context buttons', () => {
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: withButton(GAMEPAD_BUTTON_INDEX.back), now: 16 },
      { snapshot: pad(), now: 32 },
      { snapshot: withButton(GAMEPAD_BUTTON_INDEX.context), now: 48 },
    ]);
    expect(emitted).toEqual([[], ['back'], [], ['context']]);
  });
});

describe('stepGamepad ownership changes', () => {
  it('clears held and repeat state when the controller disconnects', () => {
    const held = withButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: held, now: 0 },
      { snapshot: null, now: 10 },
      // The same physical hold is still present when the controller comes back.
      { snapshot: held, now: 20 },
      { snapshot: held, now: 20 + GAMEPAD_TUNING.initialRepeatDelayMs },
    ]);
    expect(emitted).toEqual([[], ['moveDown'], [], [], []]);
  });

  it('clears state when the active controller is replaced', () => {
    const first = withButton(GAMEPAD_BUTTON_INDEX.confirm, pad({ index: 0 }));
    const second = withButton(GAMEPAD_BUTTON_INDEX.confirm, pad({ index: 1 }));
    const { emitted } = run([
      { snapshot: pad(), now: 0 },
      { snapshot: first, now: 16 },
      { snapshot: second, now: 32 },
      { snapshot: pad({ index: 1 }), now: 48 },
      { snapshot: second, now: 64 },
    ]);
    expect(emitted).toEqual([[], ['confirm'], [], [], ['confirm']]);
  });

  it('does not replay a held input when ownership returns after a focus loss', () => {
    const held = withButton(
      GAMEPAD_BUTTON_INDEX.confirm,
      withButton(GAMEPAD_BUTTON_INDEX.dpadDown),
    );
    let state = createGamepadState();
    state = stepGamepad(state, pad(), 0).state;
    state = stepGamepad(state, held, 16).state;

    state = releaseGamepadOwnership(state);

    const returning = stepGamepad(state, held, 5_000);
    expect(returning.actions).toEqual([]);
    const stillHeld = stepGamepad(
      returning.state,
      held,
      5_000 + GAMEPAD_TUNING.initialRepeatDelayMs + GAMEPAD_TUNING.repeatIntervalMs,
    );
    expect(stillHeld.actions).toEqual([]);

    const released = stepGamepad(stillHeld.state, pad(), 6_000);
    expect(released.actions).toEqual([]);
    const pressedAgain = stepGamepad(released.state, held, 6_016);
    expect(pressedAgain.actions).toEqual(['moveDown', 'confirm']);
  });
});
