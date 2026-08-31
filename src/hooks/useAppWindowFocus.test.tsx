import { render, screen, waitFor } from '@testing-library/react';
import { act } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAppWindowFocus } from './useAppWindowFocus';

const mocks = vi.hoisted(() => ({
  isAppWindowFocused: vi.fn(),
  onAppWindowFocusChanged: vi.fn(),
  isDesktopRuntime: vi.fn(),
  handlers: new Set<(focused: boolean) => void>(),
}));

vi.mock('../platform/appWindow', () => ({
  isAppWindowFocused: mocks.isAppWindowFocused,
  onAppWindowFocusChanged: mocks.onAppWindowFocusChanged,
  isDesktopRuntime: mocks.isDesktopRuntime,
  requestAppWindowFocus: vi.fn(),
}));

function Harness() {
  const focused = useAppWindowFocus();
  return <span data-testid="focus">{focused ? 'focused' : 'unfocused'}</span>;
}

function state() {
  return screen.getByTestId('focus').textContent;
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.isAppWindowFocused.mockReset();
  mocks.onAppWindowFocusChanged.mockReset();
  mocks.isDesktopRuntime.mockReset();
  mocks.isDesktopRuntime.mockReturnValue(true);
  mocks.isAppWindowFocused.mockResolvedValue(true);
  mocks.onAppWindowFocusChanged.mockImplementation(async (handler: (focused: boolean) => void) => {
    mocks.handlers.add(handler);
    return () => mocks.handlers.delete(handler);
  });
});

describe('useAppWindowFocus in the desktop runtime', () => {
  it('follows the native window focus state', async () => {
    render(<Harness />);
    await waitFor(() => expect(state()).toBe('focused'));

    act(() => mocks.handlers.forEach((handler) => handler(false)));
    expect(state()).toBe('unfocused');

    act(() => mocks.handlers.forEach((handler) => handler(true)));
    expect(state()).toBe('focused');
  });

  it('adopts the initial native state when the window starts unfocused', async () => {
    mocks.isAppWindowFocused.mockResolvedValue(false);
    render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    expect(state()).toBe('unfocused');
  });

  it('never claims ownership before the native state has been read', async () => {
    let resolveFocus: ((value: boolean | null) => void) | undefined;
    mocks.isAppWindowFocused.mockReturnValue(
      new Promise<boolean | null>((resolve) => {
        resolveFocus = resolve;
      }),
    );
    render(<Harness />);
    expect(state()).toBe('unfocused');
    await act(async () => {
      resolveFocus?.(true);
    });
    await waitFor(() => expect(state()).toBe('focused'));
  });

  it('fails closed when the native focus state cannot be read', async () => {
    // RetroFrontier cannot truthfully assert that it owns the controller, so it must not.
    mocks.isAppWindowFocused.mockResolvedValue(null);
    render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    expect(state()).toBe('unfocused');
  });

  it('fails closed when the native focus read rejects', async () => {
    mocks.isAppWindowFocused.mockRejectedValue(new Error('no window'));
    render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    expect(state()).toBe('unfocused');
  });

  it('does not turn a failed focus subscription into a permanent ownership grant', async () => {
    // Without a subscription the state could never become false again, so a one-off "focused" read
    // would grant ownership for the rest of the session.
    mocks.onAppWindowFocusChanged.mockResolvedValue(null);
    render(<Harness />);
    await waitFor(() => expect(mocks.onAppWindowFocusChanged).toHaveBeenCalled());
    expect(state()).toBe('unfocused');
  });

  it('does not grant ownership when subscribing rejects', async () => {
    mocks.onAppWindowFocusChanged.mockRejectedValue(new Error('no listener'));
    render(<Harness />);
    await waitFor(() => expect(mocks.onAppWindowFocusChanged).toHaveBeenCalled());
    expect(state()).toBe('unfocused');
  });
});

describe('useAppWindowFocus outside the desktop runtime', () => {
  it('keeps a plain browser dev session usable', async () => {
    mocks.isDesktopRuntime.mockReturnValue(false);
    mocks.isAppWindowFocused.mockResolvedValue(null);
    mocks.onAppWindowFocusChanged.mockResolvedValue(null);
    render(<Harness />);
    expect(state()).toBe('focused');
    // There is no native window to interrogate, so the boundary is not called at all and the
    // unknown native state cannot make development unusable.
    await act(async () => undefined);
    expect(mocks.isAppWindowFocused).not.toHaveBeenCalled();
    expect(state()).toBe('focused');
  });
});
