import { afterEach, describe, expect, it, vi } from 'vitest';

import { publishActiveControllerOwner, resetActiveControllerOwner } from './activeController';
import { GAMEPAD_BUTTON_INDEX, type GamepadSnapshot } from './gamepadAdapter';
import {
  activeControllerIdentity,
  detectInputRuntime,
  gamepadLayoutQuirk,
  normalizeGamepadSnapshot,
  normalizeGamepads,
} from './gamepadQuirks';

/** The qualification target's own user agent: WebKitGTK 2.52.5 in the Linux Tauri WebView. */
const WEBKITGTK_LINUX_UA =
  'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/60.5 Safari/605.1.15';

/** The Linux kernel device name WebKitGTK reports verbatim as `Gamepad.id` over USB. */
const DUALSENSE_USB_ID = 'Sony Interactive Entertainment DualSense Wireless Controller';

function pad(overrides: Partial<GamepadSnapshot> = {}): GamepadSnapshot {
  return {
    index: 0,
    id: DUALSENSE_USB_ID,
    mapping: 'standard',
    connected: true,
    buttons: Array.from({ length: 17 }, () => ({ pressed: false })),
    axes: [0, 0, 0, 0],
    ...overrides,
  };
}

/** A snapshot with exactly one raw button held, as the browser would report it. */
function withRawButton(
  rawIndex: number,
  overrides: Partial<GamepadSnapshot> = {},
): GamepadSnapshot {
  const base = pad(overrides);
  return {
    ...base,
    buttons: base.buttons.map((_button, index) => ({ pressed: index === rawIndex })),
  };
}

/** The canonical indices reported as pressed, so a mapping assertion reads as a mapping. */
function pressedCanonicalIndices(snapshot: GamepadSnapshot): number[] {
  return snapshot.buttons.flatMap((button, index) => (button.pressed ? [index] : []));
}

describe('detectInputRuntime', () => {
  it('recognizes WebKitGTK on Linux, the qualified affected engine', () => {
    expect(detectInputRuntime(WEBKITGTK_LINUX_UA)).toBe('webkitgtk-linux');
  });

  it('does not recognize Chromium on Linux, which advertises AppleWebKit too', () => {
    expect(
      detectInputRuntime(
        'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36',
      ),
    ).toBe('other');
  });

  it('does not recognize WebKit on macOS, a different Gamepad backend entirely', () => {
    expect(
      detectInputRuntime(
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15',
      ),
    ).toBe('other');
  });

  it('does not recognize Firefox on Linux', () => {
    expect(
      detectInputRuntime('Mozilla/5.0 (X11; Linux x86_64; rv:135.0) Gecko/20100101 Firefox/135.0'),
    ).toBe('other');
  });

  it('treats a missing or empty user agent as unaffected rather than guessing', () => {
    expect(detectInputRuntime(undefined)).toBe('other');
    expect(detectInputRuntime('')).toBe('other');
  });
});

describe('gamepadLayoutQuirk', () => {
  it('requires both the affected engine and the affected device', () => {
    expect(gamepadLayoutQuirk(pad(), 'webkitgtk-linux')).toBe('transposed-face-buttons');
    expect(gamepadLayoutQuirk(pad(), 'other')).toBeNull();
    expect(
      gamepadLayoutQuirk(pad({ id: 'Xbox Wireless Controller' }), 'webkitgtk-linux'),
    ).toBeNull();
  });

  it('recognizes the DualSense under its Bluetooth naming as well as its USB naming', () => {
    expect(
      gamepadLayoutQuirk(pad({ id: 'DualSense Wireless Controller' }), 'webkitgtk-linux'),
    ).toBe('transposed-face-buttons');
  });
});

/**
 * A pad the browser mapped correctly must be handed on byte for byte.
 *
 * This is the guard that keeps the fix narrow: the canonical Standard Gamepad layout is the contract
 * the whole application above this boundary is written against, and normalization may never be the
 * reason it changes.
 */
describe('a correctly mapped Standard Gamepad', () => {
  const canonical = pad({ id: 'Xbox Wireless Controller' });

  it.each([
    ['confirm', GAMEPAD_BUTTON_INDEX.confirm],
    ['back', GAMEPAD_BUTTON_INDEX.back],
    ['context', GAMEPAD_BUTTON_INDEX.context],
    ['search', GAMEPAD_BUTTON_INDEX.search],
  ])('keeps %s on canonical index %i on the affected engine', (_action, index) => {
    const raw = withRawButton(index, { id: canonical.id });
    expect(pressedCanonicalIndices(normalizeGamepadSnapshot(raw, 'webkitgtk-linux'))).toEqual([
      index,
    ]);
  });

  it('is returned as the very same object, so the canonical path allocates nothing', () => {
    expect(normalizeGamepadSnapshot(canonical, 'webkitgtk-linux')).toBe(canonical);
    // Including the affected device itself, once the engine is not the affected one.
    const dualsense = pad();
    expect(normalizeGamepadSnapshot(dualsense, 'other')).toBe(dualsense);
  });
});

/**
 * The physically measured WebKitGTK/DualSense quirk.
 *
 * Raw indices from the operator's probe of the real `Gamepad.buttons` array on the qualification
 * hardware: Cross 0, Circle 1, Triangle 2, Square 3. Canonical Standard Gamepad wants Square — the
 * left face button — at 2 and Triangle — the upper face button — at 3, so exactly those two are
 * transposed and nothing else is.
 */
describe('the qualified WebKitGTK DualSense quirk', () => {
  it('maps raw Cross 0 to canonical confirm', () => {
    const normalized = normalizeGamepadSnapshot(withRawButton(0), 'webkitgtk-linux');
    expect(pressedCanonicalIndices(normalized)).toEqual([GAMEPAD_BUTTON_INDEX.confirm]);
  });

  it('maps raw Circle 1 to canonical back', () => {
    const normalized = normalizeGamepadSnapshot(withRawButton(1), 'webkitgtk-linux');
    expect(pressedCanonicalIndices(normalized)).toEqual([GAMEPAD_BUTTON_INDEX.back]);
  });

  it('maps raw Square 3 to canonical context', () => {
    const normalized = normalizeGamepadSnapshot(withRawButton(3), 'webkitgtk-linux');
    expect(pressedCanonicalIndices(normalized)).toEqual([GAMEPAD_BUTTON_INDEX.context]);
  });

  it('maps raw Triangle 2 to canonical search', () => {
    const normalized = normalizeGamepadSnapshot(withRawButton(2), 'webkitgtk-linux');
    expect(pressedCanonicalIndices(normalized)).toEqual([GAMEPAD_BUTTON_INDEX.search]);
  });

  it('leaves the D-pad, the shoulders, and every axis untouched', () => {
    for (const index of [
      4,
      5,
      6,
      7,
      8,
      9,
      10,
      11,
      GAMEPAD_BUTTON_INDEX.dpadUp,
      GAMEPAD_BUTTON_INDEX.dpadDown,
      GAMEPAD_BUTTON_INDEX.dpadLeft,
      GAMEPAD_BUTTON_INDEX.dpadRight,
      16,
    ]) {
      const normalized = normalizeGamepadSnapshot(withRawButton(index), 'webkitgtk-linux');
      expect(pressedCanonicalIndices(normalized)).toEqual([index]);
    }

    const deflected = pad({ axes: [-0.9, 0.8, 0.2, -0.4] });
    expect(normalizeGamepadSnapshot(deflected, 'webkitgtk-linux').axes).toEqual([
      -0.9, 0.8, 0.2, -0.4,
    ]);
  });

  it('corrects a layout without disguising the device', () => {
    const normalized = normalizeGamepadSnapshot(pad({ index: 2 }), 'webkitgtk-linux');
    expect(normalized.index).toBe(2);
    expect(normalized.id).toBe(DUALSENSE_USB_ID);
    expect(normalized.mapping).toBe('standard');
    expect(normalized.connected).toBe(true);
    expect(normalized.buttons).toHaveLength(17);
  });

  it('keeps a pad that reports fewer buttons than the transposition names', () => {
    const short = pad({ buttons: [{ pressed: false }, { pressed: false }, { pressed: true }] });
    const normalized = normalizeGamepadSnapshot(short, 'webkitgtk-linux');
    expect(normalized.buttons).toHaveLength(3);
    expect(pressedCanonicalIndices(normalized)).toEqual([2]);
  });
});

/**
 * A pad shaped like a **real** browser `Gamepad`, not like a test literal.
 *
 * This is the shape that matters: `Gamepad` exposes `index`, `id`, `mapping`, `connected`, `buttons`,
 * and `axes` as prototype getters, so it has no own enumerable properties at all. A normalization
 * that copied the snapshot by spreading it would produce `mapping: undefined` and
 * `connected: undefined` here, the adapter would reject the pad as uninterpretable, and the
 * controller would stop working entirely rather than being corrected — which is exactly what
 * happened on the qualification hardware while every plain-object test kept passing.
 */
class BrowserLikeGamepad {
  constructor(
    private readonly source: GamepadSnapshot,
    private readonly rawPressed: number,
  ) {}

  get index(): number {
    return this.source.index;
  }
  get id(): string {
    return this.source.id;
  }
  get mapping(): string {
    return this.source.mapping;
  }
  get connected(): boolean {
    return this.source.connected;
  }
  get axes(): readonly number[] {
    return this.source.axes;
  }
  get buttons(): readonly { pressed: boolean }[] {
    return this.source.buttons.map((_button, index) => ({ pressed: index === this.rawPressed }));
  }
}

describe('a pad shaped like a real browser Gamepad', () => {
  function browserLikePad(rawPressed: number): GamepadSnapshot {
    return new BrowserLikeGamepad(pad(), rawPressed) as unknown as GamepadSnapshot;
  }

  it('has no own enumerable properties, so it cannot be copied by spreading', () => {
    expect(Object.keys({ ...browserLikePad(2) })).not.toContain('mapping');
  });

  it('keeps the identity the adapter gates on, so the pad stays interpretable', () => {
    const normalized = normalizeGamepadSnapshot(browserLikePad(2), 'webkitgtk-linux');
    expect(normalized.mapping).toBe('standard');
    expect(normalized.connected).toBe(true);
    expect(normalized.id).toBe(DUALSENSE_USB_ID);
    expect(normalized.index).toBe(0);
    expect(normalized.axes).toEqual([0, 0, 0, 0]);
  });

  it('still transposes the two face buttons', () => {
    expect(
      pressedCanonicalIndices(normalizeGamepadSnapshot(browserLikePad(2), 'webkitgtk-linux')),
    ).toEqual([GAMEPAD_BUTTON_INDEX.search]);
    expect(
      pressedCanonicalIndices(normalizeGamepadSnapshot(browserLikePad(3), 'webkitgtk-linux')),
    ).toEqual([GAMEPAD_BUTTON_INDEX.context]);
  });
});

describe('normalizeGamepads', () => {
  it('preserves slots and empty entries', () => {
    const pads = [null, withRawButton(2, { index: 1 }), null];
    const normalized = normalizeGamepads(pads, 'webkitgtk-linux');
    expect(normalized).toHaveLength(3);
    expect(normalized[0]).toBeNull();
    expect(normalized[2]).toBeNull();
    expect(pressedCanonicalIndices(normalized[1] as GamepadSnapshot)).toEqual([
      GAMEPAD_BUTTON_INDEX.search,
    ]);
  });

  it('normalizes only the affected pad when an unaffected pad is attached alongside', () => {
    const pads = [
      withRawButton(2, { index: 0, id: 'Xbox Wireless Controller' }),
      withRawButton(2, { index: 1 }),
    ];
    const [xbox, dualsense] = normalizeGamepads(pads, 'webkitgtk-linux');
    expect(pressedCanonicalIndices(xbox as GamepadSnapshot)).toEqual([
      GAMEPAD_BUTTON_INDEX.context,
    ]);
    expect(pressedCanonicalIndices(dualsense as GamepadSnapshot)).toEqual([
      GAMEPAD_BUTTON_INDEX.search,
    ]);
  });
});

describe('activeControllerIdentity', () => {
  afterEach(() => {
    resetActiveControllerOwner();
  });

  it('returns the id of the controller that actually owns input', () => {
    publishActiveControllerOwner(pad({ id: DUALSENSE_USB_ID }));
    expect(activeControllerIdentity()).toBe(DUALSENSE_USB_ID);
  });

  it('returns null when no owner has been proven', () => {
    expect(activeControllerIdentity()).toBeNull();
  });

  it('returns null once the acquisition boundary releases ownership', () => {
    publishActiveControllerOwner(pad({ id: DUALSENSE_USB_ID }));
    publishActiveControllerOwner(null);
    expect(activeControllerIdentity()).toBeNull();
  });

  /**
   * MEDIUM-2 regression. It does **not** re-select a controller of its own: it reports whoever the
   * input layer says currently owns input, even when that is not the lowest connected index. The
   * previous implementation called `selectActiveGamepad(pads, null)` here and would have answered
   * `'DualSense at index 0'` while the UI was being driven by the pad at index 1.
   */
  it('never re-selects: a lower-index pad does not take the reported identity', () => {
    // The input layer's own ownership decision: the pad at index 1 got there first and keeps it.
    publishActiveControllerOwner(pad({ index: 1, id: 'Xbox Wireless Controller' }));
    // A second pad appears at a lower index. Nothing about ownership changed.
    vi.stubGlobal('navigator', {
      ...window.navigator,
      getGamepads: () => [
        pad({ index: 0, id: DUALSENSE_USB_ID }),
        pad({ index: 1, id: 'Xbox Wireless Controller' }),
      ],
    });
    expect(activeControllerIdentity()).toBe('Xbox Wireless Controller');
    vi.unstubAllGlobals();
  });
});
