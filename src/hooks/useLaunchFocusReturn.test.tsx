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

const CONTENT_UNIT_ID = 101;

interface HarnessProps {
  running: RunningGameSession | null;
  blocked: boolean;
  windowFocused: boolean;
  routeKey?: string;
  /** A launch request this frontend issued has not resolved yet. */
  pendingGameId?: number | null;
  /** The launch is waiting for the user to choose a content unit. */
  contentSelectionOpen?: boolean;
}

/**
 * The Detail route: Play, Favorite, the temporary content-selection option, and a programmatic
 * heading fallback. The Library route renders a card and its own heading fallback, so a route change
 * really changes which nodes exist.
 */
function Body({
  running,
  blocked,
  windowFocused,
  routeKey = DETAIL_ROUTE,
  pendingGameId = null,
  contentSelectionOpen = false,
}: HarnessProps) {
  const onDetail = routeKey === DETAIL_ROUTE;
  const playRef = useFocusNode({ id: focusNodes.detail('play') });
  const favoriteRef = useFocusNode({ id: focusNodes.detail('favorite') });
  const headingRef = useFocusNode({ id: focusNodes.libraryHeading });
  const cardRef = useFocusNode({ id: focusNodes.libraryGame(1) });
  const optionRef = useFocusNode({ id: focusNodes.launchContent(CONTENT_UNIT_ID) });
  const { beginLaunchInteraction } = useLaunchFocusReturn({
    running,
    blocked,
    pendingGameId,
    contentSelectionOpen,
    windowFocused,
    routeKey,
    fallbackNodeId: onDetail ? focusNodes.detail('play') : focusNodes.libraryHeading,
  });
  return (
    <>
      <button data-testid="capture" onClick={beginLaunchInteraction} type="button">
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
          {contentSelectionOpen ? (
            <button ref={optionRef} type="button">
              DISC 1
            </button>
          ) : null}
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

  it('restores DOM focus when the window was already focused as the process ended', async () => {
    // The transition that creates a return must itself make the restore state observable. If the
    // user came back to RetroFrontier while the game was still running, no further focus event
    // arrives after the exit, so a restore path that only reacts to `windowFocused` never runs and
    // the logical DOM focus stays lost for the rest of the session.
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();

    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());

    // The user manually returns to RetroFrontier while RetroArch is still running.
    rerender(<Harness blocked={false} running={session} windowFocused />);
    await act(async () => undefined);
    expect(screen.getByText('PLAY')).not.toHaveFocus();

    // RetroArch exits with RetroFrontier already focused.
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());

    // Exactly one native request, and no further focus event was needed to complete the return.
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });

  it('does not restore again when idle state rerenders after a completed return', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused />);
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());

    // The user moves focus, then unrelated rerenders happen: route state, then plain rerenders.
    act(() => screen.getByText('FAVORITE').focus());
    rerender(<Harness blocked={false} running={null} windowFocused />);
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await act(async () => undefined);
    expect(screen.getByText('FAVORITE')).toHaveFocus();
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
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

describe('useLaunchFocusReturn launch interaction lifetime', () => {
  /**
   * A multi-step content selection is ONE launch interaction. The temporary content-option node the
   * user confirms from does not exist any more when RetroArch exits, so recording it as the launch
   * origin makes the return either late (via the safety fallback) or wrong (an obsolete node that
   * re-registers takes focus).
   */
  it('keeps the original PLAY origin across a content-selection continuation', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());

    // Step 1: PLAY.
    capture();
    rerender(<Harness blocked={false} pendingGameId={1} running={null} windowFocused />);

    // The backend answers `contentSelectionRequired`.
    rerender(<Harness blocked={false} contentSelectionOpen running={null} windowFocused />);
    await act(async () => undefined);
    act(() => screen.getByText('DISC 1').focus());

    // Step 2: the user confirms a version. Same interaction, so the origin must not move.
    capture();
    rerender(<Harness blocked={false} pendingGameId={1} running={null} windowFocused />);
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());

    // RetroArch exits and the window comes back.
    rerender(<Harness blocked={false} running={null} windowFocused={false} />);
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
    rerender(<Harness blocked={false} running={null} windowFocused />);

    // Immediate, not after the bounded safety fallback: the target was never the obsolete node.
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });

  it('leaves no request that an obsolete content option could satisfy later', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} contentSelectionOpen running={null} windowFocused />);
    act(() => screen.getByText('DISC 1').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());

    // The user starts another launch that needs a version again. A stale `resolveOnRegister`
    // request for the old content option would steal focus the moment the surface remounts.
    act(() => screen.getByText('FAVORITE').focus());
    rerender(<Harness blocked={false} contentSelectionOpen running={null} windowFocused />);
    await act(async () => undefined);
    expect(screen.getByText('DISC 1')).not.toHaveFocus();
  });

  it('clears the origin when the content selection is cancelled', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} contentSelectionOpen running={null} windowFocused />);
    await act(async () => undefined);

    // Cancelled: no process was ever started, so the interaction is over.
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await act(async () => undefined);

    // A later, independent launch captures a fresh origin — FAVORITE, not the stale PLAY.
    act(() => screen.getByText('FAVORITE').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('FAVORITE')).toHaveFocus());
  });

  it('clears the origin when a launch fails without ever running', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} pendingGameId={1} running={null} windowFocused />);
    // A normalized failure clears the pending id without starting a process.
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await act(async () => undefined);

    act(() => screen.getByText('FAVORITE').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('FAVORITE')).toHaveFocus());
  });

  it('clears the origin when the second, content-selected launch fails', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    rerender(<Harness blocked={false} contentSelectionOpen running={null} windowFocused />);
    act(() => screen.getByText('DISC 1').focus());
    capture();
    rerender(<Harness blocked={false} pendingGameId={1} running={null} windowFocused />);
    // The second launch fails: no surface, no process, nothing left to return to.
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await act(async () => undefined);

    act(() => screen.getByText('FAVORITE').focus());
    capture();
    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('FAVORITE')).toHaveFocus());
  });

  it('holds the open interaction while launch state is uncertain', async () => {
    const { rerender } = render(<Harness blocked={false} running={null} windowFocused />);
    act(() => screen.getByText('PLAY').focus());
    capture();
    // Blocked: nothing may be concluded, so the origin must neither be consumed nor discarded.
    rerender(<Harness blocked pendingGameId={null} running={null} windowFocused />);
    await act(async () => undefined);

    rerender(<Harness blocked={false} running={session} windowFocused={false} />);
    act(() => (document.activeElement as HTMLElement).blur());
    rerender(<Harness blocked={false} running={null} windowFocused />);
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());
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
