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

/**
 * A subscription whose promise resolution is controlled by the test, while the handler is registered
 * synchronously — which is how the real boundary behaves: Tauri attaches the listener on the Rust
 * side and only then resolves with the unlisten function.
 */
function deferredSubscription() {
  let resolveSubscription: ((release: (() => void) | null) => void) | undefined;
  let rejectSubscription: ((reason: unknown) => void) | undefined;
  mocks.onAppWindowFocusChanged.mockImplementation((handler: (focused: boolean) => void) => {
    mocks.handlers.add(handler);
    return new Promise<(() => void) | null>((resolve, reject) => {
      resolveSubscription = resolve;
      rejectSubscription = reject;
    });
  });
  return {
    succeed: async () =>
      act(async () => {
        resolveSubscription?.(() => mocks.handlers.clear());
      }),
    fail: async (reason: unknown) =>
      act(async () => {
        rejectSubscription?.(reason);
      }),
  };
}

function deferredRead() {
  let resolveRead: ((value: boolean | null) => void) | undefined;
  mocks.isAppWindowFocused.mockImplementation(
    () =>
      new Promise<boolean | null>((resolve) => {
        resolveRead = resolve;
      }),
  );
  return {
    resolve: async (value: boolean | null) =>
      act(async () => {
        resolveRead?.(value);
      }),
  };
}

function emit(focused: boolean) {
  act(() => mocks.handlers.forEach((handler) => handler(focused)));
}

describe('useAppWindowFocus bootstrap ordering', () => {
  it('does not read the native focus state before the subscription is established', async () => {
    // Reading first is what loses a focus change that happens while the listener is not attached
    // yet: the read would then be the only observation, and it would be stale.
    const subscription = deferredSubscription();
    render(<Harness />);
    await act(async () => undefined);
    expect(mocks.isAppWindowFocused).not.toHaveBeenCalled();
    expect(state()).toBe('unfocused');

    await subscription.succeed();
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(state()).toBe('focused'));
  });

  it('observes a focus change that happened before the subscription attached', async () => {
    // The window really is unfocused by the time RetroFrontier can observe anything. A read taken
    // at mount would have said `true` and no event would ever correct it.
    let nativeFocus = true;
    mocks.isAppWindowFocused.mockImplementation(async () => nativeFocus);
    const subscription = deferredSubscription();
    render(<Harness />);
    await act(async () => undefined);

    nativeFocus = false;
    await subscription.succeed();
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    expect(state()).toBe('unfocused');
  });

  it('never lets a stale initial read override a newer focus event', async () => {
    const read = deferredRead();
    render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());

    // An authoritative event arrives while the read is still in flight.
    emit(false);
    expect(state()).toBe('unfocused');

    // The older observation resolves afterwards. It must not resurrect ownership.
    await read.resolve(true);
    await act(async () => undefined);
    expect(state()).toBe('unfocused');

    // Events stay authoritative afterwards.
    emit(true);
    expect(state()).toBe('focused');
  });

  it('fails closed when the post-subscription read fails and no event has spoken', async () => {
    mocks.isAppWindowFocused.mockRejectedValue(new Error('no window'));
    render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    await act(async () => undefined);
    expect(state()).toBe('unfocused');

    // A later event is still authoritative: an unreadable state is not a permanent refusal.
    emit(true);
    expect(state()).toBe('focused');
  });

  it('keeps an event-established focus state when the read fails afterwards', async () => {
    const read = deferredRead();
    render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    emit(true);
    expect(state()).toBe('focused');
    await read.resolve(null);
    await act(async () => undefined);
    expect(state()).toBe('focused');
  });

  it('releases a subscription that resolves after unmount', async () => {
    const subscription = deferredSubscription();
    const { unmount } = render(<Harness />);
    unmount();
    await subscription.succeed();
    await act(async () => undefined);
    // Nothing was read for a hook that no longer exists.
    expect(mocks.isAppWindowFocused).not.toHaveBeenCalled();
  });

  it('ignores a read that resolves after unmount', async () => {
    const read = deferredRead();
    const { unmount } = render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    unmount();
    await read.resolve(true);
    await act(async () => undefined);
    expect(screen.queryByTestId('focus')).not.toBeInTheDocument();
  });

  it('follows several rapid focus changes in order', async () => {
    render(<Harness />);
    await waitFor(() => expect(state()).toBe('focused'));
    emit(false);
    emit(true);
    emit(false);
    expect(state()).toBe('unfocused');
    emit(true);
    expect(state()).toBe('focused');
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
