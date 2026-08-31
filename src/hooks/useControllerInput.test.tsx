import { act, render } from '@testing-library/react';
import { useEffect } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { InputAction } from '../input/actions';
import { GAMEPAD_BUTTON_INDEX, GAMEPAD_TUNING } from '../input/gamepadAdapter';
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
