import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FocusProvider } from '../../focus/FocusProvider';
import { useFocusApi } from '../../focus/focusContext';
import type { GameDetailModel } from '../../hooks/useGameDetail';
import type { GameLaunchModel } from '../../hooks/useGameLaunch';
import type {
  LaunchContentOption,
  LaunchFailure,
  LibraryGameDetail,
  SystemStatus,
} from '../../platform/ipc';
import { installRectStub, layoutColumn } from '../../test/geometry';
import { GameDetailPage } from './GameDetailPage';

const localDetail: LibraryGameDetail = {
  gameId: 7,
  systemId: 'playstation',
  localTitle: 'Ridge Racer Local',
  availability: 'available',
  favorite: false,
  contentUnits: [
    {
      unitId: 11,
      rootId: 2,
      kind: 'cueBin',
      localTitle: 'Ridge Racer Disc 1',
      primaryRelativePath: 'PlayStation/Ridge Racer (Disc 1).cue',
      fileCount: 4,
      availability: 'available',
    },
  ],
};

const systemStatus: SystemStatus = {
  id: 'playstation',
  displayName: 'PlayStation',
  manufacturer: 'Sony',
  aliases: [],
  supportedExtensions: ['.cue'],
  core: {
    policy: {
      defaultCoreId: 'beetle_psx',
      approvedCoreIds: ['beetle_psx'],
      decision: { kind: 'resolved' },
    },
    availability: {
      runtimeState: 'ready',
      availableCoreIds: ['beetle_psx'],
      defaultCoreAvailable: true,
    },
  },
  bios: { policy: 'required', ready: true, requirements: [] },
  readiness: { ready: true, reasons: [] },
};

const contentOptions: LaunchContentOption[] = [
  {
    contentUnitId: 11,
    localTitle: 'Ridge Racer Disc 1',
    kind: 'cueBin',
    fileCount: 4,
    availability: 'available',
  },
  {
    contentUnitId: 12,
    localTitle: 'Ridge Racer Disc 2',
    kind: 'cueBin',
    fileCount: 4,
    availability: 'available',
  },
];

const launchFailure: LaunchFailure = {
  code: 'runtimeNotReady',
  message: 'The managed RetroArch runtime is not installed.',
  context: {
    systemId: 'playstation',
    coreId: null,
    biosRequirementIds: [],
    runtimeState: null,
    hostPrerequisite: null,
    exitCode: null,
    contentOptions: [],
  },
};

function detailModel(): GameDetailModel {
  return {
    localDetail,
    metadata: null,
    localLoaded: true,
    metadataLoaded: true,
    localLoading: false,
    metadataLoading: false,
    localError: null,
    metadataError: null,
    favoritePending: false,
    favoriteError: null,
    metadataActionPending: false,
    metadataActionKind: null,
    metadataActionTarget: null,
    metadataActionError: null,
    refresh: vi.fn().mockResolvedValue(undefined),
    retryLocal: vi.fn().mockResolvedValue(undefined),
    retryMetadata: vi.fn().mockResolvedValue(undefined),
    requestMetadata: vi.fn().mockResolvedValue(undefined),
    refreshMetadata: vi.fn().mockResolvedValue(undefined),
    selectMetadataCandidate: vi.fn().mockResolvedValue(undefined),
    clearMetadataSelection: vi.fn().mockResolvedValue(undefined),
    toggleFavorite: vi.fn().mockResolvedValue(undefined),
  };
}

function launchModel(overrides: Partial<GameLaunchModel> = {}): GameLaunchModel {
  return {
    phase: 'idle',
    running: null,
    blocked: false,
    pendingGameId: null,
    failure: null,
    contentOptions: null,
    diagnostics: [],
    launch: vi.fn().mockResolvedValue(undefined),
    dismissFailure: vi.fn(),
    cancelContentSelection: vi.fn(),
    ...overrides,
  };
}

function Dispatcher() {
  const api = useFocusApi();
  return (
    <>
      <button
        aria-hidden="true"
        data-testid="dispatch-back"
        onClick={() => api.dispatch('back', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-down"
        onClick={() => api.dispatch('moveDown', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-confirm"
        onClick={() => api.dispatch('confirm', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-context"
        onClick={() => api.dispatch('context', 'gamepad')}
        type="button"
      />
    </>
  );
}

function send(action: 'back' | 'down' | 'confirm' | 'context') {
  act(() => {
    fireEvent.click(screen.getByTestId(`dispatch-${action}`));
  });
}

function renderWithSelection(cancelContentSelection = vi.fn()) {
  function Harness() {
    const [open, setOpen] = useState(false);
    return (
      <FocusProvider>
        <Dispatcher />
        <button
          aria-hidden="true"
          data-testid="open-selection"
          onClick={() => setOpen(true)}
          type="button"
        />
        <GameDetailPage
          detail={detailModel()}
          gameId={7}
          launch={launchModel({
            contentOptions: open ? contentOptions : null,
            cancelContentSelection: () => {
              cancelContentSelection();
              setOpen(false);
            },
          })}
          onBackToLibrary={vi.fn()}
          onRetryReadiness={vi.fn()}
          readinessError={null}
          systemStatus={systemStatus}
        />
      </FocusProvider>
    );
  }
  render(<Harness />);
  // The surface only ever appears in response to a launch attempt, never on mount.
  act(() => {
    fireEvent.click(screen.getByTestId('open-selection'));
  });
  return { cancelContentSelection };
}

function selectionSurface() {
  return screen.getByRole('group', { name: 'Choose a version' });
}

beforeEach(() => {
  installRectStub();
});

/**
 * A normalized launch failure is a temporary surface exactly like the content selection: it appears
 * in response to an action, it owns `back` while it is open, and dismissing it must put focus back
 * where the launch started from. Without a scope, focus stayed wherever the pending launch had left
 * it — typically BACK TO LIBRARY — and controller `back` navigated away instead of dismissing.
 *
 * The failure is always raised *after* mount, because that is the only way it can ever occur: it is
 * the answer to a launch the user started on this screen. Rendering it at mount would race the
 * route-entry heading focus and test a state the application cannot reach.
 */
interface FailureHarnessOptions {
  contentSelectionFirst?: boolean;
  blocked?: boolean;
  onBackToLibrary?: () => void;
  detail?: GameDetailModel;
}

function renderWithFailure(options: FailureHarnessOptions = {}) {
  const dismissFailure = vi.fn();
  const detail = options.detail ?? detailModel();
  function Harness() {
    const [failed, setFailed] = useState(false);
    const [selecting, setSelecting] = useState(false);
    const [onDetail, setOnDetail] = useState(true);
    return (
      <FocusProvider>
        <Dispatcher />
        <button
          aria-hidden="true"
          data-testid="open-selection"
          onClick={() => setSelecting(true)}
          type="button"
        />
        <button
          aria-hidden="true"
          data-testid="fail-launch"
          onClick={() => {
            setSelecting(false);
            setFailed(true);
          }}
          type="button"
        />
        <button
          aria-hidden="true"
          data-testid="leave-route"
          onClick={() => setOnDetail(false)}
          type="button"
        />
        {onDetail ? (
          <GameDetailPage
            detail={detail}
            gameId={7}
            launch={launchModel({
              blocked: options.blocked ?? false,
              contentOptions: selecting ? contentOptions : null,
              failure: failed ? launchFailure : null,
              dismissFailure: () => {
                dismissFailure();
                setFailed(false);
              },
            })}
            onBackToLibrary={options.onBackToLibrary ?? vi.fn()}
            onRetryReadiness={vi.fn()}
            readinessError={null}
            systemStatus={systemStatus}
          />
        ) : (
          <button type="button">LIBRARY HEADING</button>
        )}
      </FocusProvider>
    );
  }
  render(<Harness />);
  if (options.contentSelectionFirst === true) {
    act(() => {
      fireEvent.click(screen.getByTestId('open-selection'));
    });
  }
  act(() => {
    fireEvent.click(screen.getByTestId('fail-launch'));
  });
  return { detail, dismissFailure };
}

function failureSurface() {
  return screen.getByRole('group', { name: 'Launch failed' });
}

function dismissAction() {
  return within(failureSurface()).getByRole('button', { name: 'DISMISS' });
}

function playAction() {
  return screen.getByRole('button', { name: 'Play Ridge Racer Local' });
}

function backAction() {
  return screen.getByRole('link', { name: /BACK TO LIBRARY/ });
}

function failureGone() {
  return screen.queryByRole('group', { name: 'Launch failed' });
}

describe('Game Detail launch failure scope', () => {
  it('moves focus to DISMISS when a launch failure appears', async () => {
    renderWithFailure();
    await waitFor(() => expect(dismissAction()).toHaveFocus());
  });

  it('dismisses with confirm and restores the Play action', async () => {
    const { dismissFailure } = renderWithFailure();
    await waitFor(() => expect(dismissAction()).toHaveFocus());
    send('confirm');
    expect(dismissFailure).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(failureGone()).not.toBeInTheDocument());
    expect(playAction()).toHaveFocus();
  });

  it('dismisses with back instead of navigating to the Library', async () => {
    const onBackToLibrary = vi.fn();
    const { dismissFailure } = renderWithFailure({ onBackToLibrary });
    await waitFor(() => expect(dismissAction()).toHaveFocus());

    // Focus is deliberately moved outside first: `back` must still reach the innermost surface.
    act(() => backAction().focus());
    send('back');
    expect(onBackToLibrary).not.toHaveBeenCalled();
    expect(dismissFailure).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(failureGone()).not.toBeInTheDocument());
    expect(playAction()).toHaveFocus();
  });

  it('keeps directional movement inside the failure surface', async () => {
    renderWithFailure();
    await waitFor(() => expect(dismissAction()).toHaveFocus());
    const dismiss = dismissAction();
    const outside = screen.getByRole('button', { name: 'Add Ridge Racer Local to favorites' });
    layoutColumn([dismiss, outside]);
    act(() => dismiss.focus());
    send('down');
    expect(dismiss).toHaveFocus();
  });

  it('refuses to activate an underlying control reached with the pointer', async () => {
    const { detail } = renderWithFailure();
    await waitFor(() => expect(dismissAction()).toHaveFocus());

    // Tab or a click can legitimately move focus out of a non-modal surface; acting out there with
    // a controller may not.
    act(() => screen.getByRole('button', { name: 'Add Ridge Racer Local to favorites' }).focus());
    send('confirm');
    send('context');
    expect(detail.toggleFavorite).not.toHaveBeenCalled();
    expect(failureGone()).toBeInTheDocument();
  });

  it('takes focus after a content-selected launch fails, then restores Play', async () => {
    const { dismissFailure } = renderWithFailure({ contentSelectionFirst: true });
    await waitFor(() => expect(dismissAction()).toHaveFocus());
    expect(screen.queryByRole('group', { name: 'Choose a version' })).not.toBeInTheDocument();
    send('confirm');
    expect(dismissFailure).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(playAction()).toHaveFocus());
  });

  it('falls back to the Back action when Play cannot take focus', async () => {
    // Launch state is blocked, so Play is disabled and is not a truthful restoration target.
    renderWithFailure({ blocked: true });
    await waitFor(() => expect(dismissAction()).toHaveFocus());
    send('confirm');
    await waitFor(() => expect(backAction()).toHaveFocus());
  });

  it('does not force focus back when the surface unmounts with the route', async () => {
    renderWithFailure();
    await waitFor(() => expect(dismissAction()).toHaveFocus());
    act(() => {
      fireEvent.click(screen.getByTestId('leave-route'));
    });
    act(() => screen.getByText('LIBRARY HEADING').focus());
    await act(async () => undefined);
    // The old route is gone; nothing may drag the user back to it.
    expect(screen.getByText('LIBRARY HEADING')).toHaveFocus();
  });
});

describe('Game Detail launch content selection scope', () => {
  it('moves focus into the selection surface when it opens', async () => {
    renderWithSelection();
    await waitFor(() =>
      expect(
        within(selectionSurface()).getByRole('button', { name: /Ridge Racer Disc 1/ }),
      ).toHaveFocus(),
    );
  });

  it('keeps directional movement inside the selection surface', () => {
    renderWithSelection();
    const surface = selectionSurface();
    const first = within(surface).getByRole('button', { name: /Ridge Racer Disc 1/ });
    const second = within(surface).getByRole('button', { name: /Ridge Racer Disc 2/ });
    const cancel = within(surface).getByRole('button', { name: 'CANCEL' });
    const outside = screen.getByRole('button', { name: 'Add Ridge Racer Local to favorites' });
    layoutColumn([first, second, cancel, outside]);

    act(() => first.focus());
    send('down');
    expect(second).toHaveFocus();
    send('down');
    expect(cancel).toHaveFocus();
    send('down');
    expect(cancel).toHaveFocus();
  });

  it('cancels the selection with back and restores the Play action', async () => {
    const { cancelContentSelection } = renderWithSelection();
    send('back');

    expect(cancelContentSelection).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: 'Choose a version' })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Play Ridge Racer Local' })).toHaveFocus();
  });

  it('launches the focused version with confirm', () => {
    const launch = vi.fn().mockResolvedValue(undefined);
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <FocusProvider>
          <Dispatcher />
          <button
            aria-hidden="true"
            data-testid="open-selection"
            onClick={() => setOpen(true)}
            type="button"
          />
          <GameDetailPage
            detail={detailModel()}
            gameId={7}
            launch={launchModel({ contentOptions: open ? contentOptions : null, launch })}
            onBackToLibrary={vi.fn()}
            onRetryReadiness={vi.fn()}
            readinessError={null}
            systemStatus={systemStatus}
          />
        </FocusProvider>
      );
    }
    render(<Harness />);
    act(() => {
      fireEvent.click(screen.getByTestId('open-selection'));
    });

    act(() =>
      within(selectionSurface())
        .getByRole('button', { name: /Ridge Racer Disc 2/ })
        .focus(),
    );
    send('confirm');
    expect(launch).toHaveBeenCalledWith(7, 12);
  });
});

describe('Game Detail scope activation boundary', () => {
  it('refuses controller confirm on a control outside the open selection surface', () => {
    renderWithSelection();
    const outside = screen.getByRole('button', { name: 'Add Ridge Racer Local to favorites' });
    const detail = detailModel();
    void detail;

    // Tab or a pointer can still leave a non-modal scope; the controller may not act out there.
    act(() => outside.focus());
    send('confirm');
    expect(screen.getByRole('group', { name: 'Choose a version' })).toBeInTheDocument();
    expect(outside).toHaveFocus();
  });

  it('refuses controller context on a control outside the open selection surface', () => {
    const cancelContentSelection = vi.fn();
    renderWithSelection(cancelContentSelection);
    const back = screen.getByRole('link', { name: /BACK TO LIBRARY/ });
    act(() => back.focus());
    send('context');
    send('confirm');
    expect(screen.getByRole('group', { name: 'Choose a version' })).toBeInTheDocument();
    expect(cancelContentSelection).not.toHaveBeenCalled();
  });

  it('re-enters the selection surface on the next directional action', () => {
    renderWithSelection();
    const surface = selectionSurface();
    const first = within(surface).getByRole('button', { name: /Ridge Racer Disc 1/ });
    const second = within(surface).getByRole('button', { name: /Ridge Racer Disc 2/ });
    const cancel = within(surface).getByRole('button', { name: 'CANCEL' });
    const outside = screen.getByRole('button', { name: 'Add Ridge Racer Local to favorites' });
    layoutColumn([first, second, cancel, outside]);

    act(() => outside.focus());
    send('down');
    expect(first).toHaveFocus();
  });

  it('activates ordinary Game Detail actions again once the scope is dismissed', async () => {
    const { cancelContentSelection } = renderWithSelection();
    send('back');
    expect(cancelContentSelection).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: 'Choose a version' })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'Play Ridge Racer Local' })).toHaveFocus();
    send('confirm');
  });
});

describe('Game Detail launch from a content option', () => {
  /**
   * The real flow: choosing a version calls `launch()`, which sets `pendingGameId` in the same
   * commit. Play therefore becomes disabled at the exact moment the selection surface unmounts and
   * its scope tries to restore Play.
   */
  function LaunchingHarness({ onLaunch }: { onLaunch: (contentUnitId?: number) => void }) {
    const [open, setOpen] = useState(false);
    const [pending, setPending] = useState(false);
    return (
      <FocusProvider>
        <Dispatcher />
        <button
          aria-hidden="true"
          data-testid="open-selection"
          onClick={() => setOpen(true)}
          type="button"
        />
        <GameDetailPage
          detail={detailModel()}
          gameId={7}
          launch={launchModel({
            contentOptions: open ? contentOptions : null,
            pendingGameId: pending ? 7 : null,
            phase: pending ? 'launching' : 'idle',
            launch: async (_gameId: number, contentUnitId?: number) => {
              onLaunch(contentUnitId);
              setOpen(false);
              setPending(true);
            },
            cancelContentSelection: () => setOpen(false),
          })}
          onBackToLibrary={vi.fn()}
          onRetryReadiness={vi.fn()}
          readinessError={null}
          systemStatus={systemStatus}
        />
      </FocusProvider>
    );
  }

  it('does not leave focus on nothing when Play is disabled by the launch it started', async () => {
    const onLaunch = vi.fn();
    render(<LaunchingHarness onLaunch={onLaunch} />);
    act(() => {
      fireEvent.click(screen.getByTestId('open-selection'));
    });

    const option = within(selectionSurface()).getByRole('button', {
      name: /Ridge Racer Disc 2/,
    });
    act(() => option.focus());
    send('confirm');
    expect(onLaunch).toHaveBeenCalledWith(12);

    await waitFor(() =>
      expect(screen.queryByRole('group', { name: 'Choose a version' })).not.toBeInTheDocument(),
    );
    const play = screen.getByRole('button', { name: 'Play Ridge Racer Local' });
    expect(play).toBeDisabled();
    // The disabled Play cannot satisfy the restore, so the scope's fallback is used instead of the
    // request being consumed as a false success.
    expect(play).not.toHaveFocus();
    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'BACK TO LIBRARY' })).toHaveFocus(),
    );
  });
});
