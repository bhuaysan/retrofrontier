import { act, render } from '@testing-library/react';
import { useEffect } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { InputAction } from '../input/actions';
import { resetActiveControllerOwner } from '../input/activeController';
import { GAMEPAD_BUTTON_INDEX, GAMEPAD_TUNING } from '../input/gamepadAdapter';
import { activeControllerIdentity } from '../input/gamepadQuirks';
import { useControllerInput } from './useControllerInput';

interface FakePad {
  index: number;
  id: string;
  mapping: string;
  connected: boolean;
  buttons: { pressed: boolean }[];
  axes: number[];
}

function fakePad(index = 0, overrides: Partial<FakePad> = {}): FakePad {
  return {
    index,
    id: `Pad ${index}`,
    mapping: 'standard',
    connected: true,
    buttons: Array.from({ length: 17 }, () => ({ pressed: false })),
    axes: [0, 0, 0, 0],
    ...overrides,
  };
}

function withButton(index: number, pad: FakePad = fakePad()): FakePad {
  return { ...pad, buttons: pad.buttons.map((_button, i) => ({ pressed: i === index })) };
}

let pads: (FakePad | null)[] = [];

function Harness({ enabled, onAction }: { enabled: boolean; onAction: (a: InputAction) => void }) {
  useControllerInput({ enabled, onAction });
  return null;
}

/** Advances one animation frame. jsdom drives `requestAnimationFrame` from a timer. */
function frame(ms = 20) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

beforeEach(() => {
  pads = [];
  vi.useFakeTimers();
  vi.stubGlobal('navigator', {
    ...window.navigator,
    getGamepads: () => pads,
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/**
 * Ownership revocation must be visible to the animation-frame poller no later than the commit that
 * revoked it.
 *
 * These tests are deliberately ordering-sensitive rather than settled-state assertions. The probe
 * sits **before** the controller host in the tree, so its passive effect runs before the host's own
 * passive effects. Firing a polled frame from there reproduces exactly the window a passive
 * ownership gate leaves open: React has already committed `ownsInput === false`, but a poller that
 * learns about it from a passive effect still sees the old `true` and gets one more semantic frame.
 */
function OwnershipFrameProbe({ armed, pump }: { armed: boolean; pump: () => void }) {
  useEffect(() => {
    if (armed) pump();
  }, [armed, pump]);
  return null;
}

/**
 * The probe sits **before** the controller host, so its passive effect runs before the host's own
 * passive effects. Firing a polled frame from there reproduces exactly the window a passive
 * ownership gate leaves open: React has already committed `ownsInput === false`, but a poller that
 * learns about it from a passive effect still sees the old `true` and gets one more semantic frame.
 */
function OwnershipHarness({
  enabled,
  onAction,
  pump,
}: {
  enabled: boolean;
  onAction: (action: InputAction) => void;
  pump: () => void;
}) {
  return (
    <>
      <OwnershipFrameProbe armed={!enabled} pump={pump} />
      <Harness enabled={enabled} onAction={onAction} />
    </>
  );
}

describe('useControllerInput ownership revocation ordering', () => {
  let frames: FrameRequestCallback[] = [];

  /** Runs every frame the hook has queued, the way the browser would on the next repaint. */
  function pump() {
    const queued = frames;
    frames = [];
    for (const callback of queued) callback(performance.now());
  }

  beforeEach(() => {
    frames = [];
    let handle = 0;
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      frames.push(callback);
      handle += 1;
      return handle;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined);
  });

  it('cannot dispatch a held activation on a frame that runs inside the revoking commit', () => {
    const onAction = vi.fn();
    const { rerender } = render(<OwnershipHarness enabled onAction={onAction} pump={pump} />);
    pads = [fakePad()];
    act(() => pump());

    // Confirm is physically held at the exact moment RetroFrontier loses ownership.
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm)];
    rerender(<OwnershipHarness enabled={false} onAction={onAction} pump={pump} />);
    expect(onAction).not.toHaveBeenCalled();

    // And it still may not fire on any later frame while ownership is elsewhere.
    act(() => pump());
    act(() => pump());
    expect(onAction).not.toHaveBeenCalled();
  });

  it('cannot dispatch or repeat a held direction on a frame inside the revoking commit', () => {
    const onAction = vi.fn();
    const { rerender } = render(<OwnershipHarness enabled onAction={onAction} pump={pump} />);
    pads = [fakePad()];
    act(() => pump());

    pads = [withButton(GAMEPAD_BUTTON_INDEX.dpadDown)];
    rerender(<OwnershipHarness enabled={false} onAction={onAction} pump={pump} />);
    expect(onAction).not.toHaveBeenCalled();
    act(() => pump());
    act(() => pump());
    expect(onAction).not.toHaveBeenCalled();
  });

  it('adopts the physically held input when ownership returns, then honours a real press', () => {
    const onAction = vi.fn();
    const { rerender } = render(<OwnershipHarness enabled onAction={onAction} pump={pump} />);
    pads = [fakePad()];
    act(() => pump());
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm)];
    rerender(<OwnershipHarness enabled={false} onAction={onAction} pump={pump} />);
    act(() => pump());

    // Ownership returns with Confirm still held: adopted, never replayed.
    rerender(<OwnershipHarness enabled onAction={onAction} pump={pump} />);
    act(() => pump());
    act(() => pump());
    expect(onAction).not.toHaveBeenCalled();

    // Released and pressed again: a genuine press is delivered immediately.
    pads = [fakePad()];
    act(() => pump());
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm)];
    act(() => pump());
    expect(onAction.mock.calls).toEqual([['confirm']]);
  });
});

describe('useControllerInput', () => {
  it('dispatches semantic actions from the active controller', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    pads = [fakePad()];
    frame();
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm)];
    frame();
    expect(onAction).toHaveBeenCalledWith('confirm');
    expect(onAction).toHaveBeenCalledTimes(1);
  });

  it('does not dispatch while RetroFrontier does not own controller input', () => {
    const onAction = vi.fn();
    const { rerender } = render(<Harness enabled={false} onAction={onAction} />);
    pads = [fakePad()];
    frame();
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm)];
    frame();
    frame();
    expect(onAction).not.toHaveBeenCalled();

    // Ownership returns while the same button is still physically held: nothing may replay.
    rerender(<Harness enabled onAction={onAction} />);
    frame();
    frame(GAMEPAD_TUNING.initialRepeatDelayMs + GAMEPAD_TUNING.repeatIntervalMs);
    expect(onAction).not.toHaveBeenCalled();

    pads = [fakePad()];
    frame();
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm)];
    frame();
    expect(onAction).toHaveBeenCalledTimes(1);
  });

  it('keeps tracking connection state while it is not dispatching', () => {
    const onAction = vi.fn();
    const { rerender } = render(<Harness enabled={false} onAction={onAction} />);
    expect(document.documentElement.dataset.controller).toBe('disconnected');
    pads = [fakePad()];
    frame();
    expect(document.documentElement.dataset.controller).toBe('connected');
    pads = [];
    frame();
    expect(document.documentElement.dataset.controller).toBe('disconnected');
    rerender(<Harness enabled onAction={onAction} />);
    frame();
    expect(onAction).not.toHaveBeenCalled();
  });

  it('reads only the deterministically selected controller', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    pads = [fakePad(0), fakePad(1)];
    frame();
    pads = [fakePad(0), withButton(GAMEPAD_BUTTON_INDEX.confirm, fakePad(1))];
    frame();
    expect(onAction).not.toHaveBeenCalled();

    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm, fakePad(0)), fakePad(1)];
    frame();
    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onAction).toHaveBeenCalledWith('confirm');
  });

  it('does not dispatch mapped actions for a non-standard pad', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    pads = [fakePad(0, { mapping: '' })];
    frame();
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm, fakePad(0, { mapping: '' }))];
    frame();
    frame();
    expect(onAction).not.toHaveBeenCalled();
    expect(document.documentElement.dataset.controller).toBe('unsupported');
  });

  it('lets a standard pad at a higher index win over a non-standard pad at index 0', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    pads = [fakePad(0, { mapping: '' }), fakePad(1)];
    frame();
    expect(document.documentElement.dataset.controller).toBe('connected');
    pads = [fakePad(0, { mapping: '' }), withButton(GAMEPAD_BUTTON_INDEX.confirm, fakePad(1))];
    frame();
    expect(onAction).toHaveBeenCalledTimes(1);
    expect(onAction).toHaveBeenCalledWith('confirm');
  });

  it('stops polling when it unmounts', () => {
    const onAction = vi.fn();
    const { unmount } = render(<Harness enabled onAction={onAction} />);
    pads = [fakePad()];
    frame();
    unmount();
    pads = [withButton(GAMEPAD_BUTTON_INDEX.confirm)];
    frame();
    frame();
    expect(onAction).not.toHaveBeenCalled();
  });
});

/**
 * The qualified WebKitGTK/DualSense face-button quirk, driven through the real acquisition path.
 *
 * These tests press the **raw** browser indices the operator physically measured, not canonical
 * ones, and assert the semantic actions RetroFrontier receives. That is the only way to prove the
 * normalization boundary is actually in the path: a canonical-index test would pass either way.
 */
describe('useControllerInput WebKitGTK DualSense face-button normalization', () => {
  /** WebKitGTK 2.52.5 in the Linux Tauri WebView, the qualification target. */
  const WEBKITGTK_LINUX_UA =
    'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/60.5 Safari/605.1.15';
  const CHROMIUM_LINUX_UA =
    'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36';
  /** WebKitGTK reports the Linux kernel device name verbatim as `Gamepad.id`. */
  const DUALSENSE_ID = 'Sony Interactive Entertainment DualSense Wireless Controller';

  /** Raw physical indices as measured on the qualification hardware. */
  const RAW = { cross: 0, circle: 1, triangle: 2, square: 3 } as const;

  function runtime(userAgent: string) {
    vi.stubGlobal('navigator', {
      ...window.navigator,
      userAgent,
      getGamepads: () => pads,
    });
  }

  function dualsense(index = 0): FakePad {
    return fakePad(index, { id: DUALSENSE_ID });
  }

  /** Presses one raw browser button index and lets the poller observe press and release. */
  function pressRaw(rawIndex: number, pad: FakePad = dualsense()) {
    pads = [withButton(rawIndex, pad)];
    frame();
    pads = [pad];
    frame();
  }

  function actionsFrom(userAgent: string, rawIndex: number, pad: FakePad = dualsense()) {
    const onAction = vi.fn();
    runtime(userAgent);
    render(<Harness enabled onAction={onAction} />);
    pads = [pad];
    frame();
    pressRaw(rawIndex, pad);
    return onAction.mock.calls.flat();
  }

  it('maps raw Cross 0 to confirm', () => {
    expect(actionsFrom(WEBKITGTK_LINUX_UA, RAW.cross)).toEqual(['confirm']);
  });

  it('maps raw Circle 1 to back', () => {
    expect(actionsFrom(WEBKITGTK_LINUX_UA, RAW.circle)).toEqual(['back']);
  });

  it('maps raw Square 3 to context', () => {
    expect(actionsFrom(WEBKITGTK_LINUX_UA, RAW.square)).toEqual(['context']);
  });

  it('maps raw Triangle 2 to search', () => {
    expect(actionsFrom(WEBKITGTK_LINUX_UA, RAW.triangle)).toEqual(['search']);
  });

  it('reports the applied quirk as a diagnostic while the affected pad is active', () => {
    const onAction = vi.fn();
    runtime(WEBKITGTK_LINUX_UA);
    render(<Harness enabled onAction={onAction} />);
    pads = [dualsense()];
    frame();
    expect(document.documentElement.dataset.controllerLayout).toBe('transposed-face-buttons');

    pads = [];
    frame();
    expect(document.documentElement.dataset.controllerLayout).toBeUndefined();
  });

  /** The guard against a global swap: an unaffected pad on the same engine keeps canonical indices. */
  it('leaves a correctly mapped pad on the same engine canonical', () => {
    const xbox = fakePad(0, { id: 'Xbox Wireless Controller' });
    expect(actionsFrom(WEBKITGTK_LINUX_UA, 2, xbox)).toEqual(['context']);
    expect(document.documentElement.dataset.controllerLayout).toBeUndefined();
  });

  it('leaves a correctly mapped pad on the same engine canonical for search', () => {
    const xbox = fakePad(0, { id: 'Xbox Wireless Controller' });
    expect(actionsFrom(WEBKITGTK_LINUX_UA, 3, xbox)).toEqual(['search']);
  });

  it('leaves the same DualSense canonical on an engine that maps it correctly', () => {
    expect(actionsFrom(CHROMIUM_LINUX_UA, 2)).toEqual(['context']);
    expect(actionsFrom(CHROMIUM_LINUX_UA, 3)).toEqual(['search']);
  });

  /**
   * Normalization corrects a layout; it may not change the ownership discipline around it. A
   * transposed button held across a loss and return of ownership stays as silent as a canonical one.
   */
  it('keeps held-button adoption across an ownership change', () => {
    const onAction = vi.fn();
    runtime(WEBKITGTK_LINUX_UA);
    const { rerender } = render(<Harness enabled onAction={onAction} />);
    pads = [dualsense()];
    frame();

    // Raw Triangle — canonical search — is held while ownership is elsewhere, and stays held when it
    // returns. Adoption must swallow it entirely.
    pads = [withButton(RAW.triangle, dualsense())];
    rerender(<Harness enabled={false} onAction={onAction} />);
    frame();
    rerender(<Harness enabled onAction={onAction} />);
    frame();
    frame();
    expect(onAction).not.toHaveBeenCalled();

    // Released and pressed again, it is a genuine press and arrives as the canonical action.
    pads = [dualsense()];
    frame();
    pressRaw(RAW.triangle);
    expect(onAction.mock.calls.flat()).toEqual(['search']);
  });

  it('keeps deterministic ownership selection and disconnect handling', () => {
    const onAction = vi.fn();
    runtime(WEBKITGTK_LINUX_UA);
    render(<Harness enabled onAction={onAction} />);

    // The active pad keeps ownership when a second, unaffected pad is plugged in.
    pads = [dualsense(0)];
    frame();
    pads = [dualsense(0), fakePad(1, { id: 'Xbox Wireless Controller' })];
    frame();
    pressRaw(RAW.square, dualsense(0));
    expect(onAction.mock.calls.flat()).toEqual(['context']);

    // A disconnect releases, and the replacement adopts rather than replaying.
    onAction.mockClear();
    pads = [null, withButton(2, fakePad(1, { id: 'Xbox Wireless Controller' }))];
    frame();
    frame();
    expect(onAction).not.toHaveBeenCalled();
  });
});

/**
 * MEDIUM-2: the identity the backend receives must be the controller that actually owns
 * RetroFrontier input.
 *
 * `useControllerInput` is the only place ownership is decided, and it deliberately *keeps*
 * ownership with the pad that has it rather than re-picking the lowest connected index. These tests
 * assert that `activeControllerIdentity()` — the value threaded into every launch and save-state
 * load request — reports that same decision. Before the fix it called `selectActiveGamepad` a
 * second time with no ownership state, so plugging a pad in at a lower index made the backend
 * receive one controller's identity while the player drove the UI with another, and the backend
 * then wrote that other pad's raw hotkey button numbers.
 */
describe('useControllerInput publishes the controller that owns input', () => {
  const DUALSENSE_ID = 'Sony Interactive Entertainment DualSense Wireless Controller';

  afterEach(() => {
    resetActiveControllerOwner();
  });

  it('keeps the established owner when a different pad appears at a lower index', () => {
    render(<Harness enabled onAction={vi.fn()} />);

    // Controller B takes ownership first, at index 1.
    pads = [null, fakePad(1, { id: 'Xbox Wireless Controller' })];
    frame();
    expect(activeControllerIdentity()).toBe('Xbox Wireless Controller');

    // A DualSense is plugged in later, at the lower index 0. Navigation ownership does not move,
    // so neither does the identity sent to the backend — and no DualSense hotkeys are emitted.
    pads = [fakePad(0, { id: DUALSENSE_ID }), fakePad(1, { id: 'Xbox Wireless Controller' })];
    frame();
    frame();
    expect(activeControllerIdentity()).toBe('Xbox Wireless Controller');
    expect(activeControllerIdentity()).not.toBe(DUALSENSE_ID);
  });

  it('reports the qualified pad when it is the actual owner', () => {
    render(<Harness enabled onAction={vi.fn()} />);
    pads = [fakePad(0, { id: DUALSENSE_ID })];
    frame();
    expect(activeControllerIdentity()).toBe(DUALSENSE_ID);
  });

  it('reports no owner when nothing usable is connected, and after unmount', () => {
    const view = render(<Harness enabled onAction={vi.fn()} />);
    pads = [];
    frame();
    expect(activeControllerIdentity()).toBeNull();

    // A pad the browser could not map to the Standard layout owns nothing either.
    pads = [fakePad(0, { id: DUALSENSE_ID, mapping: 'xinput' })];
    frame();
    expect(activeControllerIdentity()).toBeNull();

    // A real owner, then an unmount: a stale identity must not outlive the loop that proved it.
    pads = [fakePad(0, { id: DUALSENSE_ID })];
    frame();
    expect(activeControllerIdentity()).toBe(DUALSENSE_ID);
    act(() => {
      view.unmount();
    });
    expect(activeControllerIdentity()).toBeNull();
  });
});
