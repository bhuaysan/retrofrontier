import { act, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FocusProvider } from '../focus/FocusProvider';
import { useFocusNode } from '../focus/focusContext';
import { focusNodes } from '../focus/focusNodes';
import type { RunningGameSession } from '../platform/ipc';
import { useLaunchFocusReturn } from './useLaunchFocusReturn';

const mocks = vi.hoisted(() => ({
  requestAppWindowFocus: vi.fn(),
}));

vi.mock('../platform/appWindow', () => ({
  requestAppWindowFocus: mocks.requestAppWindowFocus,
  isAppWindowFocused: vi.fn(),
  onAppWindowFocusChanged: vi.fn(),
}));

const session: RunningGameSession = {
  sessionId: 5,
  gameId: 1,
  contentUnitId: 101,
  coreId: 'nestopia',
  startedAt: 1,
};

function Body({
  running,
  blocked,
  windowFocused,
}: {
  running: RunningGameSession | null;
  blocked: boolean;
  windowFocused: boolean;
}) {
  const playRef = useFocusNode({ id: focusNodes.detail('play'), confirm: { label: 'PLAY' } });
  useLaunchFocusReturn({
    running,
    blocked,
    windowFocused,
    fallbackNodeId: focusNodes.detail('play'),
  });
  return (
    <>
      <button ref={playRef} type="button">
        PLAY
      </button>
      <button ref={useFocusNode({ id: focusNodes.detail('favorite') })} type="button">
        FAVORITE
      </button>
    </>
  );
}

function Harness(props: {
  running: RunningGameSession | null;
  blocked: boolean;
  windowFocused: boolean;
}) {
  return (
    <FocusProvider>
      <Body {...props} />
    </FocusProvider>
  );
}

beforeEach(() => {
  mocks.requestAppWindowFocus.mockReset();
  mocks.requestAppWindowFocus.mockResolvedValue(true);
});

describe('useLaunchFocusReturn', () => {
  it('asks for the application window exactly once when the managed game ends', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());

    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    expect(mocks.requestAppWindowFocus).not.toHaveBeenCalled();

    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));

    // Re-rendering while the request is outstanding must not ask again.
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });

  it('restores DOM focus only after the application window is focused again', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());

    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
    expect(screen.getByText('PLAY')).not.toHaveFocus();

    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());
  });

  it('restores the target the launch was started from', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('FAVORITE').focus());

    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('FAVORITE')).toHaveFocus());
  });

  it('does not restore focus repeatedly once it completed', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());

    act(() => screen.getByText('FAVORITE').focus());
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    rerender(<Harness blocked={false} running={null} windowFocused />);
    expect(screen.getByText('FAVORITE')).toHaveFocus();
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });

  it('never requests the window while launch state is still uncertain', async () => {
    const { rerender } = render(
      <Harness blocked={false} running={session} windowFocused={false} />,
    );
    rerender(<Harness blocked running={null} windowFocused={false} />);
    await Promise.resolve();
    expect(mocks.requestAppWindowFocus).not.toHaveBeenCalled();

    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
  });
});
