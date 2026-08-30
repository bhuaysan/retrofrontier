import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FocusProvider } from '../../focus/FocusProvider';
import { useFocusApi } from '../../focus/focusContext';
import type { GameDetailModel } from '../../hooks/useGameDetail';
import type { GameLaunchModel } from '../../hooks/useGameLaunch';
import type { LaunchContentOption, LibraryGameDetail, SystemStatus } from '../../platform/ipc';
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
    </>
  );
}

function send(action: 'back' | 'down' | 'confirm') {
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
