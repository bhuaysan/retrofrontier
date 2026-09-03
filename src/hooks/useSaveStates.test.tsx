import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  DeleteSaveStateResponse,
  LoadSaveStateResponse,
  SaveStateView,
} from '../platform/ipc';
import { useSaveStates } from './useSaveStates';

const mocks = vi.hoisted(() => ({
  listSaveStates: vi.fn(),
  loadSaveState: vi.fn(),
  deleteSaveState: vi.fn(),
}));

vi.mock('../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../platform/ipc')>();
  return {
    ...actual,
    listSaveStates: mocks.listSaveStates,
    loadSaveState: mocks.loadSaveState,
    deleteSaveState: mocks.deleteSaveState,
  };
});

function view(overrides: Partial<SaveStateView> = {}): SaveStateView {
  return {
    id: 1,
    gameId: 7,
    contentUnitId: 11,
    slot: 1,
    coreId: 'beetle_psx',
    coreDisplayVersion: '0.9.44',
    coreSourceRevision: null,
    contentUnitLabel: null,
    createdAt: 1_700_000_000_000,
    updatedAt: 1_700_000_000_000,
    thumbnailRef: null,
    capabilities: { loadability: 'ready', deletable: true },
    ...overrides,
  };
}

const started: LoadSaveStateResponse = {
  status: 'started',
  session: { sessionId: 3, gameId: 7, contentUnitId: 11, coreId: 'beetle_psx', startedAt: 5 },
  diagnostics: [],
};

const deleted: DeleteSaveStateResponse = { status: 'deleted', saveStateId: 1 };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
}

function options(overrides: Partial<Parameters<typeof useSaveStates>[0]> = {}) {
  return {
    enabled: true,
    gameId: 7 as number | null,
    launchBlocked: false,
    launchRunning: false,
    launchPending: false,
    ...overrides,
  };
}

describe('useSaveStates', () => {
  beforeEach(() => {
    mocks.listSaveStates.mockReset().mockResolvedValue([view()]);
    mocks.loadSaveState.mockReset().mockResolvedValue(started);
    mocks.deleteSaveState.mockReset().mockResolvedValue(deleted);
  });

  it('loads the save states of a game with the semantic request only', async () => {
    const { result } = renderHook(() => useSaveStates(options()));

    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(mocks.listSaveStates).toHaveBeenCalledWith({ gameId: 7 });
    expect(result.current.states).toHaveLength(1);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('never loads without a valid game', async () => {
    const { result } = renderHook(() => useSaveStates(options({ gameId: null })));
    await act(async () => Promise.resolve());
    expect(mocks.listSaveStates).not.toHaveBeenCalled();

    const disabled = renderHook(() => useSaveStates(options({ enabled: false })));
    await act(async () => Promise.resolve());
    expect(mocks.listSaveStates).not.toHaveBeenCalled();
    expect(result.current.states).toEqual([]);
    expect(disabled.result.current.states).toEqual([]);
  });

  it('exposes the backend order without re-sorting it', async () => {
    // Deliberately not `updatedAt DESC`: the backend owns the order, and a frontend sort would
    // silently disagree with it the moment that ordering rule changes.
    const backendOrder = [
      view({ id: 5, slot: 2, updatedAt: 1_700_000_000_000 }),
      view({ id: 9, slot: 3, updatedAt: 1_900_000_000_000 }),
      view({ id: 2, slot: 1, updatedAt: 1_800_000_000_000 }),
    ];
    mocks.listSaveStates.mockResolvedValue(backendOrder);
    const { result } = renderHook(() => useSaveStates(options()));

    await waitFor(() => expect(result.current.states).toHaveLength(3));
    expect(result.current.states.map((state) => state.id)).toEqual([5, 9, 2]);
  });

  it('surfaces a normalized delete failure code without parsing its message', async () => {
    mocks.deleteSaveState.mockResolvedValue({
      status: 'failed',
      error: { code: 'integrityMismatch', message: 'The registered identity no longer matches.' },
    } satisfies DeleteSaveStateResponse);
    const { result } = renderHook(() => useSaveStates(options()));
    await waitFor(() => expect(result.current.loaded).toBe(true));

    await act(async () => result.current.delete(1));

    expect(result.current.actionFailure).toEqual({
      code: 'integrityMismatch',
      message: 'The registered identity no longer matches.',
    });
    expect(result.current.deletePendingId).toBeNull();
  });

  it('leaves the list unchanged after a failed delete and reloads after a successful one', async () => {
    mocks.deleteSaveState.mockResolvedValueOnce({
      status: 'failed',
      error: { code: 'deleteFailed', message: 'The state could not be removed.' },
    } satisfies DeleteSaveStateResponse);
    const { result } = renderHook(() => useSaveStates(options()));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(mocks.listSaveStates).toHaveBeenCalledTimes(1);

    await act(async () => result.current.delete(1));
    expect(mocks.listSaveStates).toHaveBeenCalledTimes(1);
    expect(result.current.states).toHaveLength(1);

    mocks.listSaveStates.mockResolvedValue([]);
    await act(async () => result.current.delete(1));
    expect(mocks.listSaveStates).toHaveBeenCalledTimes(2);
    expect(result.current.states).toEqual([]);
    expect(result.current.actionFailure).toBeNull();
  });

  it('issues no load and no delete while a managed game is launching, running, or blocked', async () => {
    for (const blocking of [
      { launchRunning: true },
      { launchPending: true },
      { launchBlocked: true },
    ]) {
      mocks.loadSaveState.mockClear();
      mocks.deleteSaveState.mockClear();
      const { result, unmount } = renderHook(() => useSaveStates(options(blocking)));
      await waitFor(() => expect(result.current.loaded).toBe(true));

      await act(async () => result.current.load(1));
      await act(async () => result.current.delete(1));

      // Not "refused with a message": no IPC request is made at all, because the backend would
      // refuse it and RetroFrontier must not spend a launch attempt to find that out.
      expect(mocks.loadSaveState).not.toHaveBeenCalled();
      expect(mocks.deleteSaveState).not.toHaveBeenCalled();
      unmount();
    }
  });

  it('discards a late list response that belongs to a previous game', async () => {
    const first = deferred<SaveStateView[]>();
    const second = deferred<SaveStateView[]>();
    mocks.listSaveStates.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { result, rerender } = renderHook(
      (props: ReturnType<typeof options>) => useSaveStates(props),
      { initialProps: options() },
    );
    // The first game's request is really in flight before the route changes.
    await act(async () => Promise.resolve());
    expect(mocks.listSaveStates).toHaveBeenCalledWith({ gameId: 7 });

    rerender(options({ gameId: 8 }));
    await act(async () => Promise.resolve());
    expect(mocks.listSaveStates).toHaveBeenCalledWith({ gameId: 8 });

    await act(async () => second.resolve([view({ id: 42, gameId: 8 })]));
    await act(async () => first.resolve([view({ id: 1, gameId: 7 })]));

    expect(result.current.states.map((state) => state.id)).toEqual([42]);
  });

  it('reloads when the backend reports the managed game ended', async () => {
    const { result, rerender } = renderHook(
      (props: ReturnType<typeof options>) => useSaveStates(props),
      {
        initialProps: options({ launchRunning: true }),
      },
    );
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(mocks.listSaveStates).toHaveBeenCalledTimes(1);

    // A session that just ended may have produced new states, and only the backend knows it ended.
    await act(async () => {
      rerender(options({ launchRunning: false }));
    });

    await waitFor(() => expect(mocks.listSaveStates).toHaveBeenCalledTimes(2));
  });

  it('surfaces a refused load verbatim, with the code the backend decided on', async () => {
    mocks.loadSaveState.mockResolvedValue({
      status: 'refused',
      error: {
        code: 'integrityMismatch',
        message: 'The registered identity of this save state no longer matches.',
      },
    } satisfies LoadSaveStateResponse);
    const { result } = renderHook(() => useSaveStates(options()));
    await waitFor(() => expect(result.current.loaded).toBe(true));

    await act(async () => result.current.load(1));

    expect(mocks.loadSaveState).toHaveBeenCalledWith({ saveStateId: 1 });
    // A Save-State verdict stays a Save-State verdict: re-coding this as `launchFailed` would
    // claim the launch was attempted and lose which of the two refusals really happened.
    expect(result.current.actionFailure).toEqual({
      code: 'integrityMismatch',
      message: 'The registered identity of this save state no longer matches.',
    });
    expect(result.current.actionFailure?.code).not.toBe('launchFailed');
    expect(result.current.loadPendingId).toBeNull();
  });

  it('carries a failed launch through under the one launch code', async () => {
    mocks.loadSaveState.mockResolvedValue({
      status: 'launchFailed',
      error: {
        code: 'coreNotInstalled',
        message: 'The approved core is not installed.',
        context: {
          systemId: null,
          coreId: null,
          biosRequirementIds: [],
          runtimeState: null,
          hostPrerequisite: null,
          exitCode: null,
          contentOptions: [],
        },
      },
    } satisfies LoadSaveStateResponse);
    const { result } = renderHook(() => useSaveStates(options()));
    await waitFor(() => expect(result.current.loaded).toBe(true));

    await act(async () => result.current.load(1));

    // The launch pipeline's own message is surfaced verbatim; no copy is invented, no text parsed.
    expect(result.current.actionFailure).toEqual({
      code: 'launchFailed',
      message: 'The approved core is not installed.',
    });
    expect(result.current.loadPendingId).toBeNull();
  });

  it('keeps no failure when a load really started', async () => {
    const { result } = renderHook(() => useSaveStates(options()));
    await waitFor(() => expect(result.current.loaded).toBe(true));

    await act(async () => result.current.load(1));

    expect(result.current.actionFailure).toBeNull();
  });

  it('dismisses the last action failure', async () => {
    mocks.deleteSaveState.mockResolvedValue({
      status: 'failed',
      error: { code: 'unsafeFilesystemTarget', message: 'The target could not be verified.' },
    } satisfies DeleteSaveStateResponse);
    const { result } = renderHook(() => useSaveStates(options()));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    await act(async () => result.current.delete(1));
    expect(result.current.actionFailure).not.toBeNull();

    act(() => result.current.dismissActionFailure());

    expect(result.current.actionFailure).toBeNull();
  });

  it('keeps at most one load or delete unresolved at a time', async () => {
    const pending = deferred<LoadSaveStateResponse>();
    mocks.loadSaveState.mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => useSaveStates(options()));
    await waitFor(() => expect(result.current.loaded).toBe(true));

    act(() => void result.current.load(1));
    await waitFor(() => expect(result.current.loadPendingId).toBe(1));
    await act(async () => result.current.load(2));
    await act(async () => result.current.delete(1));

    expect(mocks.loadSaveState).toHaveBeenCalledTimes(1);
    expect(mocks.deleteSaveState).not.toHaveBeenCalled();
    await act(async () => pending.resolve(started));
    expect(result.current.loadPendingId).toBeNull();
  });

  it('surfaces a transport failure of the list and offers a retry', async () => {
    mocks.listSaveStates.mockRejectedValueOnce({
      code: 'database_unavailable',
      message: 'The local database is unavailable.',
    });
    const { result } = renderHook(() => useSaveStates(options()));

    await waitFor(() => expect(result.current.error?.code).toBe('database_unavailable'));
    expect(result.current.loaded).toBe(true);

    await act(async () => result.current.retry());

    expect(result.current.error).toBeNull();
    expect(result.current.states).toHaveLength(1);
  });

  it('is safe to unmount while the initial list is still in flight', async () => {
    const pending = deferred<SaveStateView[]>();
    mocks.listSaveStates.mockReturnValueOnce(pending.promise);
    const { result, unmount } = renderHook(() => useSaveStates(options()));

    unmount();
    await act(async () => pending.resolve([view()]));

    expect(result.current.states).toEqual([]);
    expect(result.current.loaded).toBe(false);
  });
});
