import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { LibraryPage, LibraryQueryRequest } from '../platform/ipc';
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

  it('keeps a requested page authoritative when metadata invalidation overlaps navigation', async () => {
    vi.useFakeTimers();
    const navigation = deferred<LibraryPage>();
    const invalidationRefresh = deferred<LibraryPage>();
    let metadataHandler:
      ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
    let page60Requests = 0;
    const unlisten = vi.fn();
    const pageOne = pageWith('Page one', 0, 61);

    mocks.queryLibrary.mockImplementation((request: LibraryQueryRequest) => {
      if (request.offset === 60) {
        page60Requests += 1;
        return page60Requests === 1 ? navigation.promise : invalidationRefresh.promise;
      }
      return Promise.resolve(pageOne);
    });
    mocks.onMetadataStateChanged.mockImplementation(async (handler) => {
      metadataHandler = handler;
      return unlisten;
    });

    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await act(async () => Promise.resolve());
    await act(async () => Promise.resolve());
    expect(result.current.page).toEqual(pageOne);

    act(() => result.current.nextPage());
    await act(async () => Promise.resolve());
    expect(mocks.queryLibrary).toHaveBeenNthCalledWith(2, {
      sort: 'titleAsc',
      offset: 60,
    });

    act(() => metadataHandler?.({ gameId: 1, providerId: 'screenScraper' }));
    await act(async () => vi.advanceTimersByTimeAsync(180));
    expect(mocks.queryLibrary).toHaveBeenNthCalledWith(3, {
      sort: 'titleAsc',
      offset: 60,
    });

    await act(async () => invalidationRefresh.resolve(pageWith('Metadata-refreshed page', 60, 61)));
    await act(async () => navigation.resolve(pageWith('Stale navigation page', 60, 61)));

    expect(
      result.current.page?.items.map(({ gameId, displayTitle }) => ({ gameId, displayTitle })),
    ).toEqual([{ gameId: 1, displayTitle: 'Metadata-refreshed page' }]);
    expect(result.current.page?.offset).toBe(60);
    expect(result.current.pageLoading).toBe(false);
    expect(result.current.refreshing).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('does not revive the previous query when a filter changes during invalidation debounce', async () => {
    vi.useFakeTimers();
    const filteredPage = deferred<LibraryPage>();
    let metadataHandler:
      ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
    const unlisten = vi.fn();
    mocks.queryLibrary.mockResolvedValueOnce(firstPage).mockReturnValueOnce(filteredPage.promise);
    mocks.onMetadataStateChanged.mockImplementation(async (handler) => {
      metadataHandler = handler;
      return unlisten;
    });

    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await act(async () => Promise.resolve());
    await act(async () => Promise.resolve());
    expect(result.current.page).toEqual(firstPage);

    act(() => metadataHandler?.({ gameId: 1, providerId: 'screenScraper' }));
    act(() => result.current.setSystemId('snes'));
    await act(async () => Promise.resolve());
    expect(mocks.queryLibrary).toHaveBeenNthCalledWith(2, {
      systemId: 'snes',
      sort: 'titleAsc',
      offset: 0,
    });

    await act(async () => vi.advanceTimersByTimeAsync(180));
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(2);

    await act(async () => filteredPage.resolve(pageWith('SNES result')));
    expect(result.current.page?.items[0].displayTitle).toBe('SNES result');
  });

  it('keeps the committed page and allows ordinary Next after page-forward failure', async () => {
    const error = new mocks.IpcError('database_unavailable', 'Page query failed.');
    const pageOne = pageWith('Page one', 0, 61);
    const pageTwo = pageWith('Page two', 60, 61);
    mocks.queryLibrary
      .mockResolvedValueOnce(pageOne)
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce(pageTwo)
      .mockResolvedValueOnce(pageOne);
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).toEqual(pageOne));

    act(() => result.current.nextPage());
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(result.current.error?.code).toBe('database_unavailable'));

    expect(
      result.current.page?.items.map(({ gameId, displayTitle }) => ({ gameId, displayTitle })),
    ).toEqual([{ gameId: 1, displayTitle: 'Page one' }]);
    expect(result.current.page?.offset).toBe(0);
    expect(result.current.pageLoading).toBe(false);
    expect(mocks.queryLibrary).toHaveBeenNthCalledWith(2, {
      sort: 'titleAsc',
      offset: 60,
    });

    act(() => result.current.nextPage());
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(3));
    expect(mocks.queryLibrary).toHaveBeenNthCalledWith(3, {
      sort: 'titleAsc',
      offset: 60,
    });
    await waitFor(() => expect(result.current.page).toEqual(pageTwo));
    expect(
      result.current.page?.items.map(({ gameId, displayTitle }) => ({ gameId, displayTitle })),
    ).toEqual([{ gameId: 1, displayTitle: 'Page two' }]);

    act(() => result.current.previousPage());
    await waitFor(() => expect(result.current.page).toEqual(pageOne));
    expect(
      result.current.page?.items.map(({ gameId, displayTitle }) => ({ gameId, displayTitle })),
    ).toEqual([{ gameId: 1, displayTitle: 'Page one' }]);
    expect(mocks.queryLibrary).toHaveBeenNthCalledWith(4, {
      sort: 'titleAsc',
      offset: 0,
    });
  });

  it('retries a failed page-forward at its failed target', async () => {
    const error = new mocks.IpcError('database_unavailable', 'Page query failed.');
    const pageOne = pageWith('Page one', 0, 61);
    const pageTwo = pageWith('Page two', 60, 61);
    mocks.queryLibrary
      .mockResolvedValueOnce(pageOne)
      .mockRejectedValueOnce(error)
      .mockResolvedValueOnce(pageTwo);
    const { result } = renderHook(() => useLibraryQuery({ enabled: true }));
    await waitFor(() => expect(result.current.page).toEqual(pageOne));

    act(() => result.current.nextPage());
    await waitFor(() => expect(result.current.error?.code).toBe('database_unavailable'));

    await act(async () => result.current.retry());
    expect(mocks.queryLibrary).toHaveBeenNthCalledWith(3, {
      sort: 'titleAsc',
      offset: 60,
    });
    expect(result.current.page).toEqual(pageTwo);
    expect(result.current.error).toBeNull();
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
    expect(result.current.debouncedSearch).toBe('A%_\\');
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

  it('flushes a continuous visible metadata stream at its maximum wait', async () => {
    vi.useFakeTimers();
    let handler: ((event: { gameId: number; providerId: 'screenScraper' }) => void) | undefined;
    mocks.onMetadataStateChanged.mockImplementation(async (nextHandler) => {
      handler = nextHandler;
      return vi.fn();
    });
    renderHook(() => useLibraryQuery({ enabled: true }));
    await act(async () => Promise.resolve());
    await act(async () => Promise.resolve());
    const initialCalls = mocks.queryLibrary.mock.calls.length;

    for (let index = 0; index < 6; index += 1) {
      act(() => handler?.({ gameId: 1, providerId: 'screenScraper' }));
      await act(async () => vi.advanceTimersByTimeAsync(100));
    }
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(initialCalls);

    await act(async () => vi.advanceTimersByTimeAsync(400));
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

  it('uses one bounded query when returning to the library after a terminal scan', async () => {
    const { rerender } = renderHook(
      ({ enabled, runId }) => useLibraryQuery({ enabled, scanCompletionRunId: runId }),
      { initialProps: { enabled: true, runId: null as number | null } },
    );
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(1));

    rerender({ enabled: false, runId: 44 });
    rerender({ enabled: true, runId: 44 });

    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(2));
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

  it('preserves the committed page when the library route is temporarily hidden', async () => {
    const pageOne = pageWith('Page one', 0, 121);
    const pageTwo = pageWith('Page two', 60, 121);
    mocks.queryLibrary.mockResolvedValueOnce(pageOne).mockResolvedValue(pageTwo);
    const { result, rerender } = renderHook(({ enabled }) => useLibraryQuery({ enabled }), {
      initialProps: { enabled: true },
    });
    await waitFor(() => expect(result.current.page?.offset).toBe(0));

    act(() => result.current.nextPage());
    await waitFor(() => expect(result.current.page?.offset).toBe(60));
    const callsBeforeRouteChange = mocks.queryLibrary.mock.calls.length;

    rerender({ enabled: false });
    rerender({ enabled: true });
    await waitFor(() =>
      expect(mocks.queryLibrary).toHaveBeenCalledTimes(callsBeforeRouteChange + 1),
    );

    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({ sort: 'titleAsc', offset: 60 });
    expect(result.current.page?.offset).toBe(60);
  });
});
