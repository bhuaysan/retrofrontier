import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  MetadataScrapeProgress,
  MetadataScrapeRunStatus,
  MetadataScrapeStatus,
} from '../platform/ipc';
import { SCRAPE_POLL_INTERVAL_MS, useMetadataScrape } from './useMetadataScrape';

const mocks = vi.hoisted(() => ({
  getMetadataScrapeStatus: vi.fn(),
  previewMetadataScrape: vi.fn(),
  startMetadataScrape: vi.fn(),
  stopMetadataScrape: vi.fn(),
}));

vi.mock('../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../platform/ipc')>();
  return {
    ...actual,
    getMetadataScrapeStatus: mocks.getMetadataScrapeStatus,
    previewMetadataScrape: mocks.previewMetadataScrape,
    startMetadataScrape: mocks.startMetadataScrape,
    stopMetadataScrape: mocks.stopMetadataScrape,
  };
});

function progress(overrides: Partial<MetadataScrapeProgress> = {}): MetadataScrapeProgress {
  return {
    totalGames: 148,
    matched: 0,
    needsReview: 0,
    noMatch: 0,
    unsupported: 0,
    failed: 0,
    running: 0,
    waiting: 148,
    ...overrides,
  };
}

function status(
  runStatus: MetadataScrapeRunStatus | null,
  overrides: Partial<MetadataScrapeProgress> = {},
): MetadataScrapeStatus {
  if (runStatus === null) {
    return { providerId: 'screenScraper', run: null, active: false };
  }
  return {
    providerId: 'screenScraper',
    active: runStatus === 'preparing' || runStatus === 'running' || runStatus === 'stopping',
    run: {
      id: 1,
      providerId: 'screenScraper',
      mode: 'missingMetadata',
      status: runStatus,
      progress: progress(overrides),
      createdAt: 10,
      updatedAt: 10,
      finishedAt: runStatus === 'completed' || runStatus === 'stopped' ? 20 : null,
    },
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

describe('useMetadataScrape', () => {
  beforeEach(() => {
    mocks.getMetadataScrapeStatus.mockReset().mockResolvedValue(status(null));
    mocks.previewMetadataScrape
      .mockReset()
      .mockImplementation(async ({ mode }: { mode: string }) => ({
        mode,
        eligibleGames: mode === 'missingMetadata' ? 148 : 7,
      }));
    mocks.startMetadataScrape.mockReset().mockResolvedValue(status('running'));
    mocks.stopMetadataScrape.mockReset().mockResolvedValue(status('stopped'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('previews the selected mode and re-counts when the mode changes', async () => {
    const { result } = renderHook(() => useMetadataScrape());

    await waitFor(() => expect(result.current.eligibleGames).toBe(148));
    expect(result.current.mode).toBe('missingMetadata');

    act(() => result.current.setMode('refreshMatched'));

    await waitFor(() => expect(result.current.eligibleGames).toBe(7));
    expect(mocks.previewMetadataScrape).toHaveBeenLastCalledWith({ mode: 'refreshMatched' });
  });

  it('never lets a slower earlier preview overwrite the current mode', async () => {
    const slow = deferred<{ mode: string; eligibleGames: number }>();
    mocks.previewMetadataScrape.mockImplementationOnce(() => slow.promise);
    mocks.previewMetadataScrape.mockImplementationOnce(async () => ({
      mode: 'refreshMatched',
      eligibleGames: 7,
    }));

    const { result } = renderHook(() => useMetadataScrape());
    // Let the first count genuinely start before switching, so both requests are really in flight.
    // Without this the mode change simply cancels the first one and the ordering guard is untested.
    await act(async () => {
      await Promise.resolve();
    });
    expect(mocks.previewMetadataScrape).toHaveBeenCalledTimes(1);

    act(() => result.current.setMode('refreshMatched'));
    await waitFor(() => expect(result.current.eligibleGames).toBe(7));

    await act(async () => {
      slow.resolve({ mode: 'missingMetadata', eligibleGames: 148 });
      await slow.promise;
    });

    expect(result.current.eligibleGames).toBe(7);
  });

  it('starts the selected mode and adopts the returned run', async () => {
    const { result } = renderHook(() => useMetadataScrape());
    await waitFor(() => expect(result.current.statusLoading).toBe(false));

    let started = false;
    await act(async () => {
      started = await result.current.start();
    });

    expect(started).toBe(true);
    expect(mocks.startMetadataScrape).toHaveBeenCalledWith({ mode: 'missingMetadata' });
    expect(result.current.active).toBe(true);
    expect(result.current.status?.run?.status).toBe('running');
  });

  it('polls only while a run owns the provider', async () => {
    vi.useFakeTimers();
    mocks.getMetadataScrapeStatus.mockResolvedValue(status('running'));

    // `waitFor` schedules on real timers, so this test settles the hook by advancing the fake clock
    // rather than waiting on wall-clock time that never passes.
    const { result } = renderHook(() => useMetadataScrape());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(result.current.active).toBe(true);

    const beforePolling = mocks.getMetadataScrapeStatus.mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCRAPE_POLL_INTERVAL_MS * 3);
    });
    expect(mocks.getMetadataScrapeStatus.mock.calls.length).toBeGreaterThan(beforePolling);

    // The run finishes; the summary is static, so polling must stop.
    mocks.getMetadataScrapeStatus.mockResolvedValue(
      status('completed', { matched: 148, waiting: 0 }),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCRAPE_POLL_INTERVAL_MS);
    });
    expect(result.current.active).toBe(false);

    const afterCompletion = mocks.getMetadataScrapeStatus.mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SCRAPE_POLL_INTERVAL_MS * 5);
    });
    expect(mocks.getMetadataScrapeStatus.mock.calls.length).toBe(afterCompletion);
  });

  it('stops an active run and reports the stopped summary', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(status('running'));
    const { result } = renderHook(() => useMetadataScrape());
    await waitFor(() => expect(result.current.active).toBe(true));

    await act(async () => {
      await result.current.stop();
    });

    expect(mocks.stopMetadataScrape).toHaveBeenCalledTimes(1);
    expect(result.current.status?.run?.status).toBe('stopped');
    expect(result.current.active).toBe(false);
  });

  it('restores an active run that was already in progress before the screen opened', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(
      status('running', { matched: 31, needsReview: 6, running: 2, waiting: 109 }),
    );

    const { result } = renderHook(() => useMetadataScrape());

    await waitFor(() => expect(result.current.active).toBe(true));
    expect(result.current.status?.run?.progress.matched).toBe(31);
    expect(mocks.startMetadataScrape).not.toHaveBeenCalled();
  });

  it('surfaces an action failure without discarding the known run state', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(status('running'));
    mocks.stopMetadataScrape.mockRejectedValue(new Error('provider unavailable'));

    const { result } = renderHook(() => useMetadataScrape());
    await waitFor(() => expect(result.current.active).toBe(true));

    let stopped = true;
    await act(async () => {
      stopped = await result.current.stop();
    });

    expect(stopped).toBe(false);
    expect(result.current.actionError).not.toBeNull();
    expect(result.current.status?.run?.status).toBe('running');
  });

  it('ignores a second action while one is already in flight', async () => {
    const pending = deferred<MetadataScrapeStatus>();
    mocks.startMetadataScrape.mockImplementationOnce(() => pending.promise);

    const { result } = renderHook(() => useMetadataScrape());
    await waitFor(() => expect(result.current.statusLoading).toBe(false));

    let first!: Promise<boolean>;
    act(() => {
      first = result.current.start();
    });
    await act(async () => {
      expect(await result.current.start()).toBe(false);
    });

    await act(async () => {
      pending.resolve(status('running'));
      await first;
    });

    expect(mocks.startMetadataScrape).toHaveBeenCalledTimes(1);
  });
});
