import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { GameMetadataState, LibraryGameDetail } from '../platform/ipc';
import { useGameDetail } from './useGameDetail';

const mocks = vi.hoisted(() => ({
  getLibraryGameDetail: vi.fn(),
  getGameMetadata: vi.fn(),
  setGameFavorite: vi.fn(),
  requestGameMetadata: vi.fn(),
  refreshGameMetadata: vi.fn(),
  selectGameMetadataCandidate: vi.fn(),
  clearGameMetadataCandidate: vi.fn(),
  onMetadataStateChanged: vi.fn(),
}));

vi.mock('../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../platform/ipc')>();
  return {
    ...actual,
    getLibraryGameDetail: mocks.getLibraryGameDetail,
    getGameMetadata: mocks.getGameMetadata,
    setGameFavorite: mocks.setGameFavorite,
    requestGameMetadata: mocks.requestGameMetadata,
    refreshGameMetadata: mocks.refreshGameMetadata,
    selectGameMetadataCandidate: mocks.selectGameMetadataCandidate,
    clearGameMetadataCandidate: mocks.clearGameMetadataCandidate,
    onMetadataStateChanged: mocks.onMetadataStateChanged,
  };
});

const localDetail: LibraryGameDetail = {
  gameId: 7,
  systemId: 'playstation',
  localTitle: 'Ridge Racer',
  availability: 'available',
  favorite: false,
  contentUnits: [
    {
      unitId: 11,
      rootId: 2,
      kind: 'singleFile',
      localTitle: 'Ridge Racer',
      primaryRelativePath: 'PlayStation/Ridge Racer.chd',
      fileCount: 1,
      availability: 'available',
    },
  ],
};

const metadata: GameMetadataState = {
  gameId: 7,
  providerId: 'screenScraper',
  status: 'matched',
  matchType: 'deterministicSha1',
  deterministic: true,
  providerGameId: '1234',
  providerRomId: '5678',
  unsupportedReason: null,
  lastFailure: null,
  lastCheckedAt: 1,
  metadata: {
    metadata: {
      title: 'Ridge Racer',
      sortTitle: 'ridge racer',
      synopsis: 'A racing game.',
      releaseDate: '1994-12-03',
      developer: 'Namco',
      publisher: 'Namco',
      genre: 'Racing',
      players: '1-2',
      region: 'US',
    },
    provenance: {
      providerId: 'screenScraper',
      providerGameId: '1234',
      sourceCredit: 'ScreenScraper',
      fetchedAt: 1,
    },
  },
  cover: null,
  candidates: [],
  userSelection: null,
  jobs: [],
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

describe('useGameDetail', () => {
  beforeEach(() => {
    vi.useRealTimers();
    mocks.getLibraryGameDetail.mockReset().mockResolvedValue(localDetail);
    mocks.getGameMetadata.mockReset().mockResolvedValue(metadata);
    mocks.setGameFavorite.mockReset().mockResolvedValue({ gameId: 7, favorite: true });
    mocks.requestGameMetadata.mockReset().mockResolvedValue(undefined);
    mocks.refreshGameMetadata.mockReset().mockResolvedValue(undefined);
    mocks.selectGameMetadataCandidate.mockReset().mockResolvedValue(undefined);
    mocks.clearGameMetadataCandidate.mockReset().mockResolvedValue(undefined);
    mocks.onMetadataStateChanged.mockReset().mockResolvedValue(vi.fn());
  });

  it('loads bounded local detail and authoritative metadata independently', async () => {
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    expect(result.current.localLoading).toBe(true);
    expect(result.current.metadataLoading).toBe(true);
    await waitFor(() => {
      expect(result.current.localDetail).toEqual(localDetail);
      expect(result.current.metadata).toEqual(metadata);
    });

    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(1);
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledWith({ gameId: 7 });
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(1);
    expect(mocks.getGameMetadata).toHaveBeenCalledWith({ gameId: 7 });
    expect(result.current.localError).toBeNull();
    expect(result.current.metadataError).toBeNull();
    expect(result.current.localLoading).toBe(false);
    expect(result.current.metadataLoading).toBe(false);
  });

  it('does not expose the previous game while a new route detail is loading', async () => {
    const nextLocal = deferred<LibraryGameDetail>();
    const nextMetadata = deferred<GameMetadataState>();
    mocks.getLibraryGameDetail.mockImplementation(({ gameId }: { gameId: number }) =>
      gameId === 7 ? Promise.resolve(localDetail) : nextLocal.promise,
    );
    mocks.getGameMetadata.mockImplementation(({ gameId }: { gameId: number }) =>
      gameId === 7 ? Promise.resolve(metadata) : nextMetadata.promise,
    );
    const { result, rerender } = renderHook(
      ({ gameId }) => useGameDetail({ enabled: true, gameId }),
      { initialProps: { gameId: 7 } },
    );

    await waitFor(() => expect(result.current.localDetail).toEqual(localDetail));
    rerender({ gameId: 8 });

    expect(result.current.localDetail).toBeNull();
    expect(result.current.metadata).toBeNull();
    expect(result.current.localLoading).toBe(true);
    expect(result.current.metadataLoading).toBe(true);

    await act(async () => {
      nextLocal.resolve({ ...localDetail, gameId: 8, localTitle: 'Next Game' });
      nextMetadata.resolve({ ...metadata, gameId: 8 });
    });
    await waitFor(() => expect(result.current.localDetail?.gameId).toBe(8));
    expect(result.current.metadata?.gameId).toBe(8);
  });

  it('does not duplicate the initial bounded load when a prior scan is already terminal', async () => {
    const { result } = renderHook(() =>
      useGameDetail({ enabled: true, gameId: 7, scanCompletionRunId: 31 }),
    );

    await waitFor(() => expect(result.current.localDetail).toEqual(localDetail));

    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(1);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(1);
  });

  it('keeps metadata usable when local detail fails and retries only local detail', async () => {
    mocks.getLibraryGameDetail.mockRejectedValueOnce({
      code: 'library_unavailable',
      message: 'Library unavailable.',
    });
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.localError?.code).toBe('library_unavailable'));
    expect(result.current.localDetail).toBeNull();
    expect(result.current.metadata).toEqual(metadata);

    await act(async () => result.current.retryLocal());

    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(2);
    expect(result.current.localDetail).toEqual(localDetail);
    expect(result.current.localError).toBeNull();
  });

  it('keeps local detail usable when metadata fails and retries only metadata', async () => {
    mocks.getGameMetadata.mockRejectedValueOnce({
      code: 'metadata_unavailable',
      message: 'Metadata unavailable.',
    });
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.metadataError?.code).toBe('metadata_unavailable'));
    expect(result.current.localDetail).toEqual(localDetail);
    expect(result.current.metadata).toBeNull();

    await act(async () => result.current.retryMetadata());

    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(2);
    expect(result.current.metadata).toEqual(metadata);
    expect(result.current.metadataError).toBeNull();
  });

  it('refreshes only the current game metadata after one coalesced invalidation', async () => {
    vi.useFakeTimers();
    let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
    const unlisten = vi.fn();
    mocks.onMetadataStateChanged.mockImplementation(async (nextHandler) => {
      handler = nextHandler;
      return unlisten;
    });
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));
    await act(async () => Promise.resolve());
    await act(async () => Promise.resolve());
    expect(result.current.metadata).toEqual(metadata);

    act(() => {
      handler?.({ gameId: 99, providerId: 'screenScraper' });
      handler?.({ gameId: 7, providerId: 'screenScraper' });
      handler?.({ gameId: 7, providerId: 'screenScraper' });
    });
    await act(async () => vi.advanceTimersByTimeAsync(179));
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(1);
    await act(async () => vi.advanceTimersByTimeAsync(1));

    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(2);
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(1);
  });

  it('refreshes local detail and metadata once per new terminal scan run', async () => {
    const { result, rerender } = renderHook(
      ({ runId }) => useGameDetail({ enabled: true, gameId: 7, scanCompletionRunId: runId }),
      { initialProps: { runId: null as number | null } },
    );
    await waitFor(() => expect(result.current.localDetail).toEqual(localDetail));

    rerender({ runId: 31 });
    await waitFor(() => expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(2));
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(2);

    rerender({ runId: 31 });
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(2);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(2);

    rerender({ runId: 32 });
    await waitFor(() => expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(3));
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(3);
  });

  it('sets the authoritative favorite result without optimistic state and reports failures separately', async () => {
    const committed = vi.fn();
    const { result } = renderHook(() =>
      useGameDetail({ enabled: true, gameId: 7, onFavoriteCommitted: committed }),
    );
    await waitFor(() => expect(result.current.localDetail).toEqual(localDetail));

    await act(async () => result.current.toggleFavorite());

    expect(mocks.setGameFavorite).toHaveBeenCalledWith({ gameId: 7, favorite: true });
    expect(result.current.localDetail?.favorite).toBe(true);
    expect(committed).toHaveBeenCalledTimes(1);
    expect(result.current.favoritePending).toBe(false);

    mocks.setGameFavorite.mockRejectedValueOnce({
      code: 'library_unavailable',
      message: 'Favorite unavailable.',
    });
    await act(async () => result.current.toggleFavorite());
    expect(result.current.localDetail?.favorite).toBe(true);
    expect(result.current.favoriteError?.code).toBe('library_unavailable');
  });

  it('requests metadata through the native command and rereads authoritative state', async () => {
    const pending = { ...metadata, status: 'pending' as const };
    mocks.getGameMetadata.mockResolvedValueOnce(pending).mockResolvedValueOnce(metadata);
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.metadata).toEqual(pending));
    await act(async () => result.current.requestMetadata());

    expect(mocks.requestGameMetadata).toHaveBeenCalledWith({ gameId: 7 });
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(2);
    expect(result.current.metadata).toEqual(metadata);
    expect(result.current.metadataActionPending).toBe(false);
  });

  it('preserves cached metadata while refresh is in flight and keeps the authoritative result', async () => {
    const refresh = deferred<void>();
    const refreshed = { ...metadata, status: 'matched' as const, lastCheckedAt: 2 };
    mocks.refreshGameMetadata.mockReturnValue(refresh.promise);
    mocks.getGameMetadata.mockResolvedValueOnce(metadata).mockResolvedValueOnce(refreshed);
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.metadata).toEqual(metadata));
    let operation: Promise<void>;
    act(() => {
      operation = result.current.refreshMetadata();
    });

    expect(result.current.metadata).toEqual(metadata);
    expect(result.current.metadataActionPending).toBe(true);
    expect(result.current.metadataActionKind).toBe('refresh');
    expect(mocks.refreshGameMetadata).toHaveBeenCalledWith({ gameId: 7 });

    await act(async () => {
      refresh.resolve();
      await operation!;
    });

    expect(result.current.metadata).toEqual(refreshed);
    expect(result.current.metadataActionPending).toBe(false);
  });

  it.each(['ambiguous', 'deferred', 'failed'] as const)(
    'uses the existing selection command and authoritative reread from %s state',
    async (status) => {
      const selection = deferred<void>();
      const candidateState = {
        ...metadata,
        status,
        providerGameId: null,
        providerRomId: null,
        metadata: null,
        unsupportedReason: status === 'deferred' ? ('chdRepresentationUndefined' as const) : null,
        lastFailure: status === 'failed' ? ('transientServer' as const) : null,
        candidates: [
          { providerGameId: 'candidate-a', title: 'Candidate A', releaseDate: '1994-01-01' },
          { providerGameId: 'candidate-b', title: 'Candidate B', releaseDate: null },
        ],
      };
      const selected = {
        ...metadata,
        userSelection: {
          gameId: 7,
          providerId: 'screenScraper' as const,
          providerGameId: 'candidate-b',
          updatedAt: 3,
        },
      };
      mocks.getGameMetadata.mockResolvedValueOnce(candidateState).mockResolvedValueOnce(selected);
      mocks.selectGameMetadataCandidate.mockReturnValue(selection.promise);
      const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

      await waitFor(() => expect(result.current.metadata).toEqual(candidateState));
      let first: Promise<void>;
      let second: Promise<void>;
      act(() => {
        first = result.current.selectMetadataCandidate('candidate-b');
        second = result.current.selectMetadataCandidate('candidate-b');
      });

      expect(mocks.selectGameMetadataCandidate).toHaveBeenCalledTimes(1);
      expect(result.current.metadataActionPending).toBe(true);
      expect(result.current.metadataActionKind).toBe('select');

      await act(async () => {
        selection.resolve();
        await Promise.all([first!, second!]);
      });

      expect(mocks.selectGameMetadataCandidate).toHaveBeenCalledWith({
        gameId: 7,
        providerGameId: 'candidate-b',
      });
      expect(mocks.getGameMetadata).toHaveBeenCalledTimes(2);
      expect(result.current.metadata).toEqual(selected);
      expect(result.current.metadataActionKind).toBeNull();
    },
  );

  it('clears only the user selection and leaves the resulting state to the backend', async () => {
    const selected = {
      ...metadata,
      userSelection: {
        gameId: 7,
        providerId: 'screenScraper' as const,
        providerGameId: '1234',
        updatedAt: 3,
      },
    };
    const cleared = { ...selected, userSelection: null, status: 'ambiguous' as const };
    mocks.getGameMetadata.mockResolvedValueOnce(selected).mockResolvedValueOnce(cleared);
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.metadata).toEqual(selected));
    await act(async () => result.current.clearMetadataSelection());

    expect(mocks.clearGameMetadataCandidate).toHaveBeenCalledWith({ gameId: 7 });
    expect(result.current.metadata).toEqual(cleared);
  });

  it('keeps the ambiguous state intact when candidate selection fails', async () => {
    const ambiguous = {
      ...metadata,
      status: 'ambiguous' as const,
      metadata: null,
      candidates: [{ providerGameId: 'candidate-a', title: 'Candidate A', releaseDate: null }],
    };
    mocks.getGameMetadata.mockResolvedValue(ambiguous);
    mocks.selectGameMetadataCandidate.mockRejectedValueOnce({
      code: 'metadata_unavailable',
      message: 'provider details must stay out of the UI',
    });
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.metadata).toEqual(ambiguous));
    await act(async () => result.current.selectMetadataCandidate('candidate-a'));

    expect(result.current.metadata).toEqual(ambiguous);
    expect(result.current.metadataActionError?.code).toBe('metadata_unavailable');
  });

  it('keeps the user selection intact when clearing it fails', async () => {
    const selected = {
      ...metadata,
      userSelection: {
        gameId: 7,
        providerId: 'screenScraper' as const,
        providerGameId: '1234',
        updatedAt: 3,
      },
    };
    mocks.getGameMetadata.mockResolvedValue(selected);
    mocks.clearGameMetadataCandidate.mockRejectedValueOnce({
      code: 'metadata_unavailable',
      message: 'provider details must stay out of the UI',
    });
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.metadata).toEqual(selected));
    await act(async () => result.current.clearMetadataSelection());

    expect(result.current.metadata).toEqual(selected);
    expect(result.current.metadataActionError?.code).toBe('metadata_unavailable');
  });

  it('keeps last-known-good metadata when a metadata mutation fails', async () => {
    mocks.refreshGameMetadata.mockRejectedValueOnce({
      code: 'metadata_unavailable',
      message: 'provider secret must not reach the UI',
    });
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));

    await waitFor(() => expect(result.current.metadata).toEqual(metadata));
    await act(async () => result.current.refreshMetadata());

    expect(result.current.metadata).toEqual(metadata);
    expect(result.current.metadataActionError?.code).toBe('metadata_unavailable');
    expect(result.current.metadataActionPending).toBe(false);
  });

  it('does not let a completed mutation for the previous route reread the current game', async () => {
    const refresh = deferred<void>();
    const nextLocal = { ...localDetail, gameId: 8, localTitle: 'Next Game' };
    const nextMetadata = { ...metadata, gameId: 8, metadata: null };
    mocks.refreshGameMetadata.mockReturnValue(refresh.promise);
    mocks.getLibraryGameDetail.mockImplementation(({ gameId }: { gameId: number }) =>
      gameId === 7 ? Promise.resolve(localDetail) : Promise.resolve(nextLocal),
    );
    mocks.getGameMetadata.mockImplementation(({ gameId }: { gameId: number }) =>
      gameId === 7 ? Promise.resolve(metadata) : Promise.resolve(nextMetadata),
    );
    const { result, rerender } = renderHook(
      ({ gameId }) => useGameDetail({ enabled: true, gameId }),
      { initialProps: { gameId: 7 } },
    );

    await waitFor(() => expect(result.current.metadata?.gameId).toBe(7));
    let operation: Promise<void>;
    act(() => {
      operation = result.current.refreshMetadata();
    });
    rerender({ gameId: 8 });
    await waitFor(() => expect(result.current.metadata?.gameId).toBe(8));

    await act(async () => {
      refresh.resolve();
      await operation!;
    });

    expect(mocks.refreshGameMetadata).toHaveBeenCalledTimes(1);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(2);
    expect(result.current.metadata?.gameId).toBe(8);
  });

  it('shows a stable not-found state when a terminal scan removes the game', async () => {
    mocks.getLibraryGameDetail.mockResolvedValueOnce(localDetail).mockResolvedValueOnce(null);
    const { result, rerender } = renderHook(
      ({ runId }) => useGameDetail({ enabled: true, gameId: 7, scanCompletionRunId: runId }),
      { initialProps: { runId: null as number | null } },
    );
    await waitFor(() => expect(result.current.localDetail).toEqual(localDetail));

    rerender({ runId: 44 });
    await waitFor(() => {
      expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(2);
      expect(result.current.localLoaded).toBe(true);
      expect(result.current.localDetail).toBeNull();
    });
    expect(result.current.localDetail).toBeNull();
    expect(result.current.metadata).toEqual(metadata);
  });

  it('cleans up metadata listeners, timers, and late registrations', async () => {
    vi.useFakeTimers();
    const registration = deferred<() => void>();
    const unlisten = vi.fn();
    let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
    mocks.onMetadataStateChanged.mockImplementation((nextHandler) => {
      handler = nextHandler;
      return registration.promise;
    });
    const { unmount } = renderHook(() => useGameDetail({ enabled: true, gameId: 7 }));
    await act(async () => Promise.resolve());
    act(() => handler?.({ gameId: 7, providerId: 'screenScraper' }));
    unmount();
    await act(async () => registration.resolve(unlisten));
    await act(async () => vi.advanceTimersByTimeAsync(180));

    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(1);
  });

  it('does not load or mutate when the route has no valid game ID', async () => {
    const { result } = renderHook(() => useGameDetail({ enabled: true, gameId: null }));

    await act(async () => Promise.resolve());

    expect(result.current.localDetail).toBeNull();
    expect(result.current.metadata).toBeNull();
    expect(mocks.getLibraryGameDetail).not.toHaveBeenCalled();
    expect(mocks.getGameMetadata).not.toHaveBeenCalled();
    expect(mocks.onMetadataStateChanged).not.toHaveBeenCalled();
  });
});
