import { render, screen, waitFor } from '@testing-library/react';
import { act } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAppWindowFocus } from './useAppWindowFocus';

const mocks = vi.hoisted(() => ({
  isAppWindowFocused: vi.fn(),
  onAppWindowFocusChanged: vi.fn(),
  handlers: new Set<(focused: boolean) => void>(),
}));

vi.mock('../platform/appWindow', () => ({
  isAppWindowFocused: mocks.isAppWindowFocused,
  onAppWindowFocusChanged: mocks.onAppWindowFocusChanged,
  requestAppWindowFocus: vi.fn(),
}));

function Harness() {
  const focused = useAppWindowFocus();
  return <span data-testid="focus">{focused ? 'focused' : 'unfocused'}</span>;
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.isAppWindowFocused.mockReset();
  mocks.onAppWindowFocusChanged.mockReset();
  mocks.isAppWindowFocused.mockResolvedValue(true);
  mocks.onAppWindowFocusChanged.mockImplementation(async (handler: (focused: boolean) => void) => {
    mocks.handlers.add(handler);
    return () => mocks.handlers.delete(handler);
  });
});

describe('useAppWindowFocus', () => {
  it('follows the native window focus state', async () => {
    render(<Harness />);
    await waitFor(() => expect(screen.getByTestId('focus')).toHaveTextContent('focused'));

    act(() => mocks.handlers.forEach((handler) => handler(false)));
    expect(screen.getByTestId('focus')).toHaveTextContent('unfocused');

    act(() => mocks.handlers.forEach((handler) => handler(true)));
    expect(screen.getByTestId('focus')).toHaveTextContent('focused');
  });

  it('adopts the initial native state when the window starts unfocused', async () => {
    mocks.isAppWindowFocused.mockResolvedValue(false);
    render(<Harness />);
    await waitFor(() => expect(screen.getByTestId('focus')).toHaveTextContent('unfocused'));
  });

  it('stays usable when no native window state is available', async () => {
    mocks.isAppWindowFocused.mockResolvedValue(null);
    render(<Harness />);
    await waitFor(() => expect(mocks.isAppWindowFocused).toHaveBeenCalled());
    expect(screen.getByTestId('focus')).toHaveTextContent('focused');
  });
});
