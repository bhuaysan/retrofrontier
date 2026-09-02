import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LibraryShelves, LibraryShelvesRequest } from '../platform/ipc';
import { useLibraryShelves } from './useLibraryShelves';

const mocks = vi.hoisted(() => {
  class MockIpcError extends Error {
    readonly code: string;

    constructor(code: string, message: string) {
      super(message);
      this.code = code;
    }
  }

  return {
    queryLibraryShelves: vi.fn(),
    onMetadataStateChanged: vi.fn(),
    normalizeIpcError: (reason: unknown) =>
      reason instanceof MockIpcError
        ? reason
        : new MockIpcError('ipc_unavailable', 'Native library unavailable.'),
    IpcError: MockIpcError,
  };
});

vi.mock('../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../platform/ipc')>();
  return {
    ...actual,
    queryLibraryShelves: mocks.queryLibraryShelves,
    onMetadataStateChanged: mocks.onMetadataStateChanged,
    normalizeIpcError: mocks.normalizeIpcError,
  };
});

function item(gameId: number, systemId: 'nes' | 'snes' = 'nes') {
  return {
    gameId,
    systemId,
    localTitle: `Local ${gameId}`,
    metadataTitle: `Game ${gameId}`,
    displayTitle: `Game ${gameId}`,
    sortTitle: `game ${gameId}`,
    availability: 'available' as const,
    favorite: false,
    metadataMatchState: 'matched' as const,
    releaseDate: null,
    genre: null,
    region: null,
    coverRef: null,
  };
}

const shelves: LibraryShelves = {
  shelves: [
    { systemId: 'snes', total: 84, items: [item(1, 'snes'), item(2, 'snes')] },
    { systemId: 'nes', total: 3, items: [item(3)] },
  ],
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

const BASE = {
  enabled: true,
  search: '',
  favoritesOnly: false,
  needsMetadataReview: false,
};

describe('useLibraryShelves', () => {
  beforeEach(() => {
    vi.useRealTimers();
    mocks.queryLibraryShelves.mockReset().mockResolvedValue(shelves);
    mocks.onMetadataStateChanged.mockReset().mockResolvedValue(vi.fn());
  });

  it('loads the bounded shelf projection once and reports it settled', async () => {
    const { result } = renderHook(() => useLibraryShelves(BASE));

    expect(result.current.initialLoading).toBe(true);
    await waitFor(() => expect(result.current.shelves).toEqual(shelves));
    expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(1);
    expect(mocks.queryLibraryShelves).toHaveBeenCalledWith({});
    expect(result.current.initialLoading).toBe(false);
    expect(result.current.error).toBeNull();
    expect(result.current.resultVersion).toBe(1);
  });

  it('does not query at all while it is disabled', async () => {
    renderHook(() => useLibraryShelves({ ...BASE, enabled: false }));
    await act(async () => Promise.resolve());

    expect(mocks.queryLibraryShelves).not.toHaveBeenCalled();
  });

  it('sends only the filters the user actually set', async () => {
    const { rerender } = renderHook(
      (props: Parameters<typeof useLibraryShelves>[0]) => useLibraryShelves(props),
      { initialProps: BASE },
    );
    await waitFor(() => expect(mocks.queryLibraryShelves).toHaveBeenCalledWith({}));

    rerender({ ...BASE, search: 'mario', favoritesOnly: true, needsMetadataReview: true });

    await waitFor(() =>
      expect(mocks.queryLibraryShelves).toHaveBeenLastCalledWith({
        search: 'mario',
        favoritesOnly: true,
        needsMetadataReview: true,
      }),
    );
  });

  it('uses the committed debounced search it is given and never debounces again', async () => {
    // Search debouncing is owned once, by the Library's own query state. A second debounce here
    // would make the shelves lag the grid by an unpredictable amount.
    const { rerender } = renderHook(
      (props: Parameters<typeof useLibraryShelves>[0]) => useLibraryShelves(props),
      { initialProps: BASE },
    );
    await waitFor(() => expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(1));

    rerender({ ...BASE, search: 'ma' });
    await waitFor(() =>
      expect(mocks.queryLibraryShelves).toHaveBeenLastCalledWith({ search: 'ma' }),
    );
    expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(2);
  });

  it('keeps the previous shelves visible when a refresh fails, and offers a retry', async () => {
    const { result, rerender } = renderHook(
      (props: Parameters<typeof useLibraryShelves>[0]) => useLibraryShelves(props),
      { initialProps: BASE },
    );
    await waitFor(() => expect(result.current.shelves).toEqual(shelves));

    const failure = new mocks.IpcError('database_unavailable', 'Shelf query failed.');
    mocks.queryLibraryShelves.mockRejectedValueOnce(failure);
    rerender({ ...BASE, favoritesOnly: true });

    await waitFor(() => expect(result.current.error?.code).toBe('database_unavailable'));
    expect(result.current.shelves, 'the whole Library must not be blanked').toEqual(shelves);

    mocks.queryLibraryShelves.mockResolvedValueOnce({ shelves: [] });
    await act(async () => {
      await result.current.retry();
    });
    expect(result.current.error).toBeNull();
    expect(result.current.shelves).toEqual({ shelves: [] });
  });

  it('reports an initial failure without any shelves rather than a false empty library', async () => {
    mocks.queryLibraryShelves.mockRejectedValueOnce(
      new mocks.IpcError('database_unavailable', 'Shelf query failed.'),
    );
    const { result } = renderHook(() => useLibraryShelves(BASE));

    await waitFor(() => expect(result.current.error?.code).toBe('database_unavailable'));
    expect(result.current.shelves).toBeNull();
    expect(result.current.initialLoading).toBe(false);
    expect(result.current.resultVersion).toBe(1);
  });

  it('ignores a stale response that resolves after a newer request', async () => {
    const stale = deferred<LibraryShelves>();
    const fresh = deferred<LibraryShelves>();
    mocks.queryLibraryShelves.mockReset();
    mocks.queryLibraryShelves.mockImplementationOnce(() => stale.promise);
    mocks.queryLibraryShelves.mockImplementationOnce(() => fresh.promise);

    const { result, rerender } = renderHook(
      (props: Parameters<typeof useLibraryShelves>[0]) => useLibraryShelves(props),
      { initialProps: BASE },
    );
    rerender({ ...BASE, favoritesOnly: true });

    const freshResult: LibraryShelves = { shelves: [{ systemId: 'nes', total: 1, items: [] }] };
    await act(async () => {
      fresh.resolve(freshResult);
      await Promise.resolve();
    });
    await waitFor(() => expect(result.current.shelves).toEqual(freshResult));

    await act(async () => {
      stale.resolve(shelves);
      await Promise.resolve();
    });
    expect(result.current.shelves, 'a superseded response must not be committed').toEqual(
      freshResult,
    );
  });

  it('refreshes once after a scan completes, without blanking the visible shelves', async () => {
    const { result, rerender } = renderHook(
      (props: Parameters<typeof useLibraryShelves>[0]) => useLibraryShelves(props),
      { initialProps: { ...BASE, scanCompletionRunId: null as number | null } },
    );
    await waitFor(() => expect(result.current.shelves).toEqual(shelves));
    expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(1);

    rerender({ ...BASE, scanCompletionRunId: 7 });
    await waitFor(() => expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(2));
    expect(result.current.initialLoading).toBe(false);
    expect(result.current.shelves).not.toBeNull();

    // The same terminal run must not keep re-querying on every unrelated rerender.
    rerender({ ...BASE, scanCompletionRunId: 7 });
    await act(async () => Promise.resolve());
    expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(2);
  });

  describe('metadata invalidation', () => {
    it('coalesces events for visible preview games into one bounded refresh', async () => {
      vi.useFakeTimers();
      let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
      mocks.onMetadataStateChanged.mockImplementation(async (next) => {
        handler = next;
        return vi.fn();
      });

      renderHook(() => useLibraryShelves(BASE));
      await act(async () => Promise.resolve());
      await act(async () => Promise.resolve());
      expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(1);

      act(() => {
        handler?.({ gameId: 1, providerId: 'screenScraper' });
        handler?.({ gameId: 2, providerId: 'screenScraper' });
        handler?.({ gameId: 3, providerId: 'screenScraper' });
      });
      expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(1);

      await act(async () => vi.advanceTimersByTimeAsync(180));
      expect(
        mocks.queryLibraryShelves,
        'three visible invalidations are one bounded refresh',
      ).toHaveBeenCalledTimes(2);
    });

    it('ignores events for games no visible shelf preview contains', async () => {
      vi.useFakeTimers();
      let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
      mocks.onMetadataStateChanged.mockImplementation(async (next) => {
        handler = next;
        return vi.fn();
      });

      renderHook(() => useLibraryShelves(BASE));
      await act(async () => Promise.resolve());
      await act(async () => Promise.resolve());

      // A whole-library scrape walks thousands of games. Only the handful on screen may cost a
      // request; everything else must cost nothing at all.
      act(() => {
        for (let gameId = 1000; gameId < 3000; gameId += 1) {
          handler?.({ gameId, providerId: 'screenScraper' });
        }
      });
      await act(async () => vi.advanceTimersByTimeAsync(2000));

      expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(1);
    });

    it('flushes a long stream of visible invalidations at the max wait', async () => {
      vi.useFakeTimers();
      let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
      mocks.onMetadataStateChanged.mockImplementation(async (next) => {
        handler = next;
        return vi.fn();
      });

      renderHook(() => useLibraryShelves(BASE));
      await act(async () => Promise.resolve());
      await act(async () => Promise.resolve());

      // Events arriving faster than the debounce would otherwise postpone the refresh forever.
      for (let tick = 0; tick < 12; tick += 1) {
        act(() => handler?.({ gameId: 1, providerId: 'screenScraper' }));
        await act(async () => vi.advanceTimersByTimeAsync(100));
      }

      expect(mocks.queryLibraryShelves.mock.calls.length).toBeGreaterThan(1);
      expect(
        mocks.queryLibraryShelves.mock.calls.length,
        'a scrape storm must not become a request storm',
      ).toBeLessThanOrEqual(3);
    });

    it('drops a pending invalidation whose filters no longer apply', async () => {
      vi.useFakeTimers();
      let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
      mocks.onMetadataStateChanged.mockImplementation(async (next) => {
        handler = next;
        return vi.fn();
      });

      const { rerender } = renderHook(
        (props: Parameters<typeof useLibraryShelves>[0]) => useLibraryShelves(props),
        { initialProps: BASE },
      );
      await act(async () => Promise.resolve());
      await act(async () => Promise.resolve());
      expect(mocks.queryLibraryShelves).toHaveBeenCalledTimes(1);

      act(() => handler?.({ gameId: 1, providerId: 'screenScraper' }));
      rerender({ ...BASE, favoritesOnly: true });
      await act(async () => Promise.resolve());
      const afterFilterChange = mocks.queryLibraryShelves.mock.calls.length;

      await act(async () => vi.advanceTimersByTimeAsync(1200));
      expect(
        mocks.queryLibraryShelves,
        'the filter change already refetched; the stale invalidation must not refetch again',
      ).toHaveBeenCalledTimes(afterFilterChange);
    });

    it('releases its event subscription on unmount', async () => {
      const unlisten = vi.fn();
      mocks.onMetadataStateChanged.mockResolvedValue(unlisten);
      const { unmount } = renderHook(() => useLibraryShelves(BASE));
      await act(async () => Promise.resolve());
      await act(async () => Promise.resolve());

      unmount();
      expect(unlisten).toHaveBeenCalled();
    });
  });

  it('never asks the backend for more than one shelf request per settled state', async () => {
    const seen: LibraryShelvesRequest[] = [];
    mocks.queryLibraryShelves.mockImplementation(async (request: LibraryShelvesRequest) => {
      seen.push(request);
      return shelves;
    });

    const { result } = renderHook(() => useLibraryShelves(BASE));
    await waitFor(() => expect(result.current.shelves).toEqual(shelves));

    expect(seen).toHaveLength(1);
    expect(seen[0], 'one request covers every system').toEqual({});
  });
});
