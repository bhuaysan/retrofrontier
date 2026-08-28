import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LibraryPage } from '../platform/ipc';
import { useLibraryQuery } from './useLibraryQuery';

const mocks = vi.hoisted(() => {
  class MockIpcError extends Error {
    readonly code: string;

    constructor(code: string, message: string) {
      super(message);
      this.code = code;
    }
  }

  return {
    queryLibrary: vi.fn(),
    setGameFavorite: vi.fn(),
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
    queryLibrary: mocks.queryLibrary,
    setGameFavorite: mocks.setGameFavorite,
    onMetadataStateChanged: mocks.onMetadataStateChanged,
    normalizeIpcError: mocks.normalizeIpcError,
  };
});

const firstPage: LibraryPage = {
  items: [
    {
      gameId: 1,
      systemId: 'nes',
      localTitle: 'Local title',
      metadataTitle: 'Metadata title',
      displayTitle: 'Metadata title',
      sortTitle: 'metadata title',
      availability: 'available',
      favorite: false,
      metadataMatchState: 'matched',
      releaseDate: '1987-09-11',
      genre: 'Platform',
      region: 'US',
      coverRef: 'rfmedia://localhost/cover/1',
    },
  ],
  total: 1,
  offset: 0,
  limit: 60,
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

function pageWith(title: string, offset = 0, total = 1): LibraryPage {
  return {
    ...firstPage,
    items: [{ ...firstPage.items[0], displayTitle: title, metadataTitle: title }],
    offset,
    total,
  };
}

describe('useLibraryQuery', () => {
  beforeEach(() => {
    vi.useRealTimers();
    mocks.queryLibrary.mockReset().mockResolvedValue(firstPage);
    mocks.setGameFavorite.mockReset().mockResolvedValue({ gameId: 1, favorite: true });
    mocks.onMetadataStateChanged.mockReset().mockResolvedValue(vi.fn());
  });

  it('loads the initial bounded page with the backend default limit', async () => {
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));

    expect(result.current.initialLoading).toBe(true);
    await waitFor(() => expect(result.current.page).toEqual(firstPage));
    expect(mocks.queryLibrary).toHaveBeenCalledWith({ sort: 'titleAsc', offset: 0 });
    expect(result.current.initialLoading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('keeps an initial query error distinct and retries authoritatively', async () => {
    const error = new mocks.IpcError('database_unavailable', 'Library query failed.');
    mocks.queryLibrary.mockRejectedValueOnce(error);
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));

    await waitFor(() => expect(result.current.error?.code).toBe('database_unavailable'));
    expect(result.current.page).toBeNull();
    expect(result.current.initialLoading).toBe(false);

    mocks.queryLibrary.mockResolvedValueOnce(firstPage);
    await act(async () => {
      await result.current.retry();
    });

    expect(result.current.page).toEqual(firstPage);
    expect(result.current.error).toBeNull();
  });

  it('pages forward and backward without requesting beyond the final page', async () => {
    const pageOne = pageWith('Page one', 0, 121);
    const pageTwo = pageWith('Page two', 60, 121);
    const finalPage = pageWith('Final page', 120, 121);
    mocks.queryLibrary
      .mockResolvedValueOnce(pageOne)
      .mockResolvedValueOnce(pageTwo)
      .mockResolvedValueOnce(finalPage)
      .mockResolvedValueOnce(pageTwo);
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page?.items[0].displayTitle).toBe('Page one'));

    act(() => result.current.nextPage());
    await waitFor(() => expect(result.current.page?.offset).toBe(60));
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({ sort: 'titleAsc', offset: 60 });

    act(() => result.current.nextPage());
    await waitFor(() => expect(result.current.page?.offset).toBe(120));
    const callCountAtEnd = mocks.queryLibrary.mock.calls.length;
    act(() => result.current.nextPage());
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(callCountAtEnd);

    act(() => result.current.previousPage());
    await waitFor(() => expect(result.current.page?.offset).toBe(60));
  });

  it('recovers to the last valid page when the total shrinks', async () => {
    mocks.queryLibrary
      .mockResolvedValueOnce(pageWith('Page one', 0, 120))
      .mockResolvedValueOnce(pageWith('Page two', 60, 120))
      .mockResolvedValueOnce({ items: [], total: 1, offset: 60, limit: 60 })
      .mockResolvedValueOnce(pageWith('Only remaining game', 0, 1));
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page?.offset).toBe(0));
    act(() => result.current.nextPage());
    await waitFor(() => expect(result.current.page?.offset).toBe(60));

    await act(async () => result.current.retry());

    await waitFor(() =>
      expect(result.current.page?.items[0].displayTitle).toBe('Only remaining game'),
    );
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({ sort: 'titleAsc', offset: 0 });
  });

  it('resets paging for a backend system ID and rejects the stale old page', async () => {
    const oldPage = deferred<LibraryPage>();
    const newFilterPage = deferred<LibraryPage>();
    mocks.queryLibrary
      .mockResolvedValueOnce(pageWith('First', 0, 120))
      .mockReturnValueOnce(oldPage.promise)
      .mockReturnValueOnce(newFilterPage.promise);
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).not.toBeNull());

    act(() => result.current.nextPage());
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(2));
    act(() => result.current.setSystemId('snes'));
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(3));
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
      systemId: 'snes',
      sort: 'titleAsc',
      offset: 0,
    });

    await act(async () => newFilterPage.resolve(pageWith('SNES result')));
    await act(async () => oldPage.resolve(pageWith('Stale page two', 60, 120)));
    expect(result.current.page?.items[0].displayTitle).toBe('SNES result');
  });

  it('debounces search and passes literal wildcard characters unchanged', async () => {
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).not.toBeNull());
    vi.useFakeTimers();

    act(() => result.current.setSearchInput('A'));
    await act(async () => vi.advanceTimersByTimeAsync(100));
    act(() => result.current.setSearchInput('A%_\\'));
    await act(async () => vi.advanceTimersByTimeAsync(199));
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(1);
    await act(async () => vi.advanceTimersByTimeAsync(1));
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
      search: 'A%_\\',
      sort: 'titleAsc',
      offset: 0,
    });
  });

  it('keeps the newest result, error, and loading ownership across stale requests', async () => {
    const older = deferred<LibraryPage>();
    const newer = deferred<LibraryPage>();
    mocks.queryLibrary
      .mockResolvedValueOnce(firstPage)
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise);
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).not.toBeNull());

    act(() => result.current.setSystemId('snes'));
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(2));
    act(() => result.current.setSystemId('mega_drive'));
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(3));

    await act(async () => older.resolve(pageWith('Stale result')));
    expect(result.current.refreshing).toBe(true);
    expect(result.current.page).toEqual(firstPage);

    await act(async () => newer.resolve(pageWith('Newest result')));
    expect(result.current.page?.items[0].displayTitle).toBe('Newest result');
    expect(result.current.refreshing).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('does not let a stale rejection overwrite a newer successful query', async () => {
    const older = deferred<LibraryPage>();
    const newer = deferred<LibraryPage>();
    mocks.queryLibrary
      .mockResolvedValueOnce(firstPage)
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise);
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).not.toBeNull());
    act(() => result.current.setSystemId('snes'));
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(2));
    act(() => result.current.setSystemId('mega_drive'));
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(3));

    await act(async () => newer.resolve(pageWith('Newest success')));
    await act(async () => older.reject(new mocks.IpcError('database_unavailable', 'stale')));
    expect(result.current.page?.items[0].displayTitle).toBe('Newest success');
    expect(result.current.error).toBeNull();
  });

  it('is safe to unmount while the initial query is active', async () => {
    const pending = deferred<LibraryPage>();
    mocks.queryLibrary.mockReturnValueOnce(pending.promise);
    const { result, unmount } = renderHook(() => useLibraryQuery({ enabled: true }));
    expect(result.current.initialLoading).toBe(true);

    unmount();
    await act(async () => pending.resolve(firstPage));

    expect(result.current.page).toBeNull();
  });

  it('suppresses duplicate favorite writes and refetches authoritative state', async () => {
    const favoriteWrite = deferred<{ gameId: number; favorite: boolean }>();
    const committed = vi.fn();
    mocks.setGameFavorite.mockReturnValueOnce(favoriteWrite.promise);
    mocks.queryLibrary.mockResolvedValueOnce(firstPage).mockResolvedValueOnce({
      ...firstPage,
      items: [{ ...firstPage.items[0], favorite: true }],
    });
    const { result } = renderHook(() =>
      useLibraryQuery({ enabled: true, onFavoriteCommitted: committed }),
    );
    await waitFor(() => expect(result.current.page).not.toBeNull());

    act(() => {
      void result.current.toggleFavorite(firstPage.items[0]);
      void result.current.toggleFavorite(firstPage.items[0]);
    });
    expect(mocks.setGameFavorite).toHaveBeenCalledTimes(1);
    expect(mocks.setGameFavorite).toHaveBeenCalledWith({ gameId: 1, favorite: true });
    expect(result.current.page?.items[0].favorite).toBe(false);

    await act(async () => favoriteWrite.resolve({ gameId: 1, favorite: true }));
    await waitFor(() => expect(result.current.page?.items[0].favorite).toBe(true));
    expect(committed).toHaveBeenCalledTimes(1);
    expect(result.current.favoritePendingIds.has(1)).toBe(false);
  });

  it('refetches the current query identity when a held favorite write finishes', async () => {
    const favoriteWrite = deferred<{ gameId: number; favorite: boolean }>();
    mocks.setGameFavorite.mockReturnValueOnce(favoriteWrite.promise);
    mocks.queryLibrary
      .mockResolvedValueOnce(firstPage)
      .mockResolvedValueOnce(pageWith('SNES result'))
      .mockResolvedValueOnce(pageWith('SNES favorite result'));
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).toEqual(firstPage));

    act(() => void result.current.toggleFavorite(firstPage.items[0]));
    act(() => result.current.setSystemId('snes'));
    await waitFor(() => expect(result.current.page?.items[0].displayTitle).toBe('SNES result'));

    await act(async () => favoriteWrite.resolve({ gameId: 1, favorite: true }));
    await waitFor(() =>
      expect(result.current.page?.items[0].displayTitle).toBe('SNES favorite result'),
    );
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
      systemId: 'snes',
      sort: 'titleAsc',
      offset: 0,
    });
  });

  it('leaves card state truthful and reports favorite mutation failure', async () => {
    mocks.setGameFavorite.mockRejectedValueOnce(
      new mocks.IpcError('database_unavailable', 'Favorite failed.'),
    );
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).not.toBeNull());

    await act(async () => result.current.toggleFavorite(firstPage.items[0]));
    expect(result.current.page).toEqual(firstPage);
    expect(result.current.favoriteError?.code).toBe('database_unavailable');
  });

  it('unfavorites from a later favorites-only page with one reset query', async () => {
    const favoriteItem = { ...firstPage.items[0], favorite: true };
    mocks.queryLibrary
      .mockResolvedValueOnce({ ...firstPage, items: [favoriteItem], total: 61 })
      .mockResolvedValueOnce({ ...firstPage, items: [favoriteItem], total: 61 })
      .mockResolvedValueOnce({
        ...firstPage,
        items: [favoriteItem],
        total: 61,
        offset: 60,
      })
      .mockResolvedValueOnce({ items: [], total: 0, offset: 0, limit: 60 });
    mocks.setGameFavorite.mockResolvedValueOnce({ gameId: 1, favorite: false });
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).not.toBeNull());
    act(() => result.current.setFavoritesOnly(true));
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(2));
    act(() => result.current.nextPage());
    await waitFor(() => expect(result.current.page?.offset).toBe(60));

    await act(async () => result.current.toggleFavorite(favoriteItem));
    await waitFor(() => expect(result.current.page?.items).toEqual([]));

    expect(mocks.queryLibrary).toHaveBeenCalledTimes(4);
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
      favoritesOnly: true,
      sort: 'titleAsc',
      offset: 0,
    });
  });

  it('coalesces visible metadata invalidations and ignores off-page IDs', async () => {
    vi.useFakeTimers();
    let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
    const unlisten = vi.fn();
    mocks.onMetadataStateChanged.mockImplementation(async (nextHandler) => {
      handler = nextHandler;
      return unlisten;
    });
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await act(async () => Promise.resolve());
    await act(async () => Promise.resolve());
    expect(result.current.page).toEqual(firstPage);
    const initialCalls = mocks.queryLibrary.mock.calls.length;

    act(() => {
      handler?.({ gameId: 99, providerId: 'screenScraper' });
      handler?.({ gameId: 1, providerId: 'screenScraper' });
      handler?.({ gameId: 1, providerId: 'screenScraper' });
    });
    await act(async () => vi.advanceTimersByTimeAsync(179));
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(initialCalls);
    await act(async () => vi.advanceTimersByTimeAsync(1));
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(initialCalls + 1);
  });

  it('cleans up metadata listeners and late registration after unmount', async () => {
    const registration = deferred<() => void>();
    const unlisten = vi.fn();
    mocks.onMetadataStateChanged.mockReturnValueOnce(registration.promise);
    const { unmount } = renderHook(() => useLibraryQuery({ enabled: true }));
    unmount();
    await act(async () => registration.resolve(unlisten));
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('clears a pending metadata invalidation timer on unmount', async () => {
    vi.useFakeTimers();
    let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
    const unlisten = vi.fn();
    mocks.onMetadataStateChanged.mockImplementation(async (nextHandler) => {
      handler = nextHandler;
      return unlisten;
    });
    const { unmount } = renderHook(() => useLibraryQuery({ enabled: true }));
    await act(async () => Promise.resolve());
    await act(async () => Promise.resolve());
    const callsBeforeEvent = mocks.queryLibrary.mock.calls.length;
    act(() => handler?.({ gameId: 1, providerId: 'screenScraper' }));

    unmount();
    await act(async () => vi.advanceTimersByTimeAsync(180));

    expect(mocks.queryLibrary).toHaveBeenCalledTimes(callsBeforeEvent);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('refreshes once for each new terminal scan run identity', async () => {
    const { rerender } = renderHook(
      ({ runId }) => useLibraryQuery({ enabled: true, scanCompletionRunId: runId }),
      { initialProps: { runId: null as number | null } },
    );
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(1));
    rerender({ runId: 31 });
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(2));
    rerender({ runId: 31 });
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(2);
    rerender({ runId: 32 });
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(3));
  });

  it('uses one initial query when a completed scan turns an empty library populated', async () => {
    const { rerender } = renderHook(
      ({ enabled, runId }) => useLibraryQuery({ enabled, scanCompletionRunId: runId }),
      { initialProps: { enabled: false, runId: null as number | null } },
    );
    expect(mocks.queryLibrary).not.toHaveBeenCalled();

    rerender({ enabled: false, runId: 44 });
    rerender({ enabled: true, runId: 44 });

    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(1));
  });
});
