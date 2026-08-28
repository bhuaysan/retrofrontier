import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { SystemsResponse } from '../platform/ipc';
import { useSystemCatalog } from './useSystemCatalog';

const mocks = vi.hoisted(() => ({
  getSystems: vi.fn(),
  normalizeIpcError: (reason: unknown) => reason,
}));

vi.mock('../platform/ipc', () => mocks);

const response: SystemsResponse = {
  runtime: {
    state: 'ready',
    installationId: 'install-1',
    releaseId: 'release-1',
    canRollback: false,
    repairRequired: false,
  },
  biosRoot: '/documents/RetroFrontier/BIOS',
  biosRootStatus: 'ready',
  systems: [
    {
      id: 'playstation',
      displayName: 'PlayStation',
      manufacturer: 'Sony',
      aliases: ['PS1'],
      supportedExtensions: ['.cue', '.chd'],
      core: {
        policy: {
          defaultCoreId: 'synthetic-core',
          approvedCoreIds: ['synthetic-core'],
          decision: { kind: 'resolved' },
        },
        availability: {
          runtimeState: 'ready',
          availableCoreIds: ['synthetic-core'],
          defaultCoreAvailable: true,
        },
      },
      bios: {
        policy: 'required',
        ready: true,
        requirements: [
          {
            requirementId: 'playstation-bios',
            systemId: 'playstation',
            required: true,
            state: 'presentValid',
            expectedFilenames: ['synthetic-bios.bin'],
            expectedSizeBytes: 1,
            description: 'Synthetic BIOS',
            matchedFilename: 'synthetic-bios.bin',
            fileSizeBytes: 1,
            sha256: null,
          },
        ],
      },
      readiness: { ready: true, reasons: [] },
    },
  ],
};

function setupDefaults() {
  mocks.getSystems.mockReset();
  mocks.getSystems.mockResolvedValue(response);
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

describe('useSystemCatalog', () => {
  beforeEach(setupDefaults);

  it('retains the full authoritative system statuses alongside sidebar labels', async () => {
    const { result } = renderHook(() => useSystemCatalog());

    await waitFor(() => expect(result.current.statuses).toHaveLength(1));

    expect(result.current.systems).toEqual([{ id: 'playstation', displayName: 'PlayStation' }]);
    expect(result.current.statuses[0]).toBe(response.systems[0]);
    expect(result.current.statuses[0].readiness.ready).toBe(true);
    expect(result.current.statuses[0].core.policy.defaultCoreId).toBe('synthetic-core');
    expect(result.current.statuses[0].bios.requirements[0].state).toBe('presentValid');
  });

  it('clears stale catalog projections during a failed refresh and can recover', async () => {
    let rejectRefresh: ((reason: unknown) => void) | undefined;
    const pending = new Promise<SystemsResponse>((_, reject) => {
      rejectRefresh = reject;
    });
    mocks.getSystems.mockResolvedValueOnce(response).mockReturnValueOnce(pending);
    const { result } = renderHook(() => useSystemCatalog());

    await waitFor(() => expect(result.current.statuses).toHaveLength(1));

    let refreshPromise: Promise<unknown> | undefined;
    act(() => {
      refreshPromise = result.current.refresh();
    });

    expect(result.current.loading).toBe(true);
    expect(result.current.systems).toEqual([]);
    expect(result.current.statuses).toEqual([]);
    expect(result.current.error).toBeNull();
    await act(async () => {
      rejectRefresh?.({ code: 'catalog_invalid', message: 'synthetic catalog failure' });
      await refreshPromise;
    });

    expect(result.current.error).toEqual({
      code: 'catalog_invalid',
      message: 'synthetic catalog failure',
    });
    expect(result.current.systems).toEqual([]);
    expect(result.current.statuses).toEqual([]);

    mocks.getSystems.mockResolvedValueOnce(response);
    await act(async () => {
      await result.current.refresh();
    });

    await waitFor(() => expect(result.current.statuses).toHaveLength(1));
    expect(result.current.error).toBeNull();
  });

  it('keeps the newest overlapping catalog refresh authoritative', async () => {
    const older = deferred<SystemsResponse>();
    const newer = deferred<SystemsResponse>();
    mocks.getSystems
      .mockResolvedValueOnce(response)
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise);
    const { result } = renderHook(() => useSystemCatalog());

    await waitFor(() => expect(result.current.statuses).toHaveLength(1));
    let olderRefresh: Promise<unknown> | undefined;
    let newerRefresh: Promise<unknown> | undefined;
    act(() => {
      olderRefresh = result.current.refresh();
      newerRefresh = result.current.refresh();
    });

    await act(async () => {
      newer.resolve({ ...response, systems: [] });
      await newerRefresh!;
    });
    expect(result.current.systems).toEqual([]);
    expect(result.current.error).toBeNull();

    await act(async () => {
      older.resolve(response);
      await olderRefresh!;
    });
    expect(result.current.systems).toEqual([]);
    expect(result.current.statuses).toEqual([]);
    expect(result.current.error).toBeNull();
  });
});
