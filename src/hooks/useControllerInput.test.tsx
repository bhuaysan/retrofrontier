import { act, render } from '@testing-library/react';
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
