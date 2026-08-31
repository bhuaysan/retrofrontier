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
  isDesktopRuntime: vi.fn(() => true),
}));

const session: RunningGameSession = {
  sessionId: 5,
  gameId: 1,
  contentUnitId: 101,
  coreId: 'nestopia',
  startedAt: 1,
};

const DETAIL_ROUTE = 'game:1';
const LIBRARY_ROUTE = 'library';

interface HarnessProps {
  running: RunningGameSession | null;
  blocked: boolean;
  windowFocused: boolean;
  routeKey?: string;
}

/**
 * The Detail route: Play, Favorite, and a programmatic heading fallback. The Library route renders
 * a card and its own heading fallback, so a route change really changes which nodes exist.
 */
function Body({ running, blocked, windowFocused, routeKey = DETAIL_ROUTE }: HarnessProps) {
  const onDetail = routeKey === DETAIL_ROUTE;
  const playRef = useFocusNode({ id: focusNodes.detail('play') });
  const favoriteRef = useFocusNode({ id: focusNodes.detail('favorite') });
  const headingRef = useFocusNode({ id: focusNodes.libraryHeading });
  const cardRef = useFocusNode({ id: focusNodes.libraryGame(1) });
  const { captureLaunchOrigin } = useLaunchFocusReturn({
    running,
    blocked,
    windowFocused,
    routeKey,
    fallbackNodeId: onDetail ? focusNodes.detail('play') : focusNodes.libraryHeading,
  });
  return (
    <>
      <button data-testid="capture" onClick={captureLaunchOrigin} type="button">
        CAPTURE
      </button>
      {onDetail ? (
        <>
          <button ref={playRef} type="button">
            PLAY
          </button>
          <button ref={favoriteRef} type="button">
            FAVORITE
          </button>
        </>
      ) : (
        <>
          <h1 ref={headingRef} tabIndex={-1}>
            LIBRARY
          </h1>
          <button ref={cardRef} type="button">
            GAME 1
          </button>
        </>
      )}
    </>
  );
}

function Harness(props: HarnessProps) {
  return (
    <FocusProvider>
      <Body {...props} />
    </FocusProvider>
  );
}

function capture() {
  act(() => {
    screen.getByTestId('capture').click();
  });
}

beforeEach(() => {
  mocks.requestAppWindowFocus.mockReset();
  mocks.requestAppWindowFocus.mockResolvedValue(true);
});

describe('useLaunchFocusReturn launch origin', () => {
  it('records the origin when the UI initiates the launch, not when running arrives', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();

    // Focus moves before the backend reports `running`: the recorded origin must not follow it.
    act(() => screen.getByText('FAVORITE').focus());
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());

    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());
  });

  it('asks for the application window exactly once per ended session', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();

    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    expect(mocks.requestAppWindowFocus).not.toHaveBeenCalled();

    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));

    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });

  it('restores DOM focus only after the application window is focused again', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();

    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
    expect(screen.getByText('PLAY')).not.toHaveFocus();

    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());
  });

  it('never requests the window while launch state is still uncertain', async () => {
    const { rerender } = render(
      <Harness blocked={false} running={session} windowFocused={false} />,
    );
    rerender(<Harness blocked running={null} windowFocused={false} />);
    await act(async () => undefined);
    expect(mocks.requestAppWindowFocus).not.toHaveBeenCalled();

    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
  });

  it('does not restore focus repeatedly once it completed', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());

    act(() => screen.getByText('FAVORITE').focus());
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    rerender(<Harness blocked={false} running={null} windowFocused />);
    expect(screen.getByText('FAVORITE')).toHaveFocus();
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });
});

describe('useLaunchFocusReturn route awareness', () => {
  it('does not drag the user back to the route the launch started from', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);

    // The user navigated to the Library while the game was running.
    rerender(
      <Harness blocked={false} running={session} routeKey={LIBRARY_ROUTE} windowFocused={false} />,
    );
    rerender(
      <Harness blocked={false} running={null} routeKey={LIBRARY_ROUTE} windowFocused={false} />,
    );
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));

    rerender(<Harness blocked={false} running={null} routeKey={LIBRARY_ROUTE} windowFocused />);
    // The current route's deterministic target, not the obsolete Detail action.
    await waitFor(() => expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus());
  });

  it('leaves no obsolete request that steals focus when the old route returns', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    rerender(
      <Harness blocked={false} running={null} routeKey={LIBRARY_ROUTE} windowFocused={false} />,
    );
    rerender(<Harness blocked={false} running={null} routeKey={LIBRARY_ROUTE} windowFocused />);
    await waitFor(() => expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus());

    act(() => screen.getByText('GAME 1').focus());
    // Going back to Game Detail later must not resurrect the old Detail restoration.
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await act(async () => undefined);
    expect(screen.getByText('PLAY')).not.toHaveFocus();
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });

  it('falls back within the current route when nothing recorded the launch origin', async () => {
    // A session that was already running when this frontend mounted has no captured origin.
    const { rerender } = render(
      <Harness blocked={false} running={session} windowFocused={false} />,
    );
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());
  });
});
