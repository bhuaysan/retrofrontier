import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ScanIssuePage, ScanStatus, ScanSummary } from '../platform/ipc';
import { useScanState } from './useScanState';

const mocks = vi.hoisted(() => ({
  getScanIssuePage: vi.fn(),
  getScanStatus: vi.fn(),
  onLibraryScanCompleted: vi.fn(),
  onLibraryScanProgress: vi.fn(),
  rescanLibrary: vi.fn(),
}));

vi.mock('../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../platform/ipc')>();
  return {
    ...actual,
    getScanIssuePage: mocks.getScanIssuePage,
    getScanStatus: mocks.getScanStatus,
    onLibraryScanCompleted: mocks.onLibraryScanCompleted,
    onLibraryScanProgress: mocks.onLibraryScanProgress,
    rescanLibrary: mocks.rescanLibrary,
  };
});

const idleStatus: ScanStatus = {
  running: false,
  progress: null,
  lastResult: null,
};

const emptyIssues: ScanIssuePage = {
  issues: [],
  scanRunId: null,
  total: 0,
  offset: 0,
  limit: 50,
};

const runningSummary: ScanSummary = {
  runId: 12,
  state: 'running',
  counters: {
    rootsDiscovered: 0,
    rootsCompleted: 0,
    filesDiscovered: 0,
    filesProcessed: 0,
    filesHashed: 0,
    bytesHashed: 0,
    issuesFound: 0,
  },
  durationMs: 0,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

describe('useScanState', () => {
  beforeEach(() => {
    mocks.getScanIssuePage.mockReset().mockResolvedValue(emptyIssues);
    mocks.getScanStatus.mockReset().mockResolvedValue(idleStatus);
    mocks.onLibraryScanCompleted.mockReset().mockResolvedValue(vi.fn());
    mocks.onLibraryScanProgress.mockReset().mockResolvedValue(vi.fn());
    mocks.rescanLibrary.mockReset().mockResolvedValue(runningSummary);
  });

  it('coalesces repeated scan starts while the native request is in flight', async () => {
    const request = deferred<ScanSummary>();
    mocks.rescanLibrary.mockReturnValue(request.promise);
    const { result } = renderHook(() => useScanState());

    await waitFor(() => expect(mocks.getScanStatus).toHaveBeenCalled());

    let first: Promise<ScanSummary | null> | undefined;
    let second: Promise<ScanSummary | null> | undefined;
    act(() => {
      first = result.current.startScan();
      second = result.current.startScan();
    });

    expect(mocks.rescanLibrary).toHaveBeenCalledTimes(1);
    expect(result.current.scanStartPending).toBe(true);
    await act(async () => {
      request.resolve(runningSummary);
      await expect(first).resolves.toEqual(runningSummary);
      await expect(second).resolves.toBeNull();
    });

    expect(result.current.scanStartPending).toBe(false);
  });
});
