import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { GameLaunchStateChanged, LaunchResponse, RunningGameSession } from '../platform/ipc';
import { useGameLaunch } from './useGameLaunch';

const mocks = vi.hoisted(() => ({
  launchGame: vi.fn(),
  getLaunchState: vi.fn(),
  onGameLaunchStateChanged: vi.fn(),
}));

vi.mock('../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../platform/ipc')>();
  return {
    ...actual,
    launchGame: mocks.launchGame,
    getLaunchState: mocks.getLaunchState,
    onGameLaunchStateChanged: mocks.onGameLaunchStateChanged,
  };
});

const session: RunningGameSession = {
  sessionId: 11,
  gameId: 7,
  contentUnitId: 3,
  coreId: 'beetle-psx',
  startedAt: 1_756_000_000_000,
};

let emit: (event: GameLaunchStateChanged) => void = () => undefined;

beforeEach(() => {
  mocks.getLaunchState.mockResolvedValue({ running: null, blocked: false });
  mocks.onGameLaunchStateChanged.mockImplementation(
    (handler: (event: GameLaunchStateChanged) => void) => {
      emit = handler;
      return Promise.resolve(() => undefined);
    },
  );
});

function started(): LaunchResponse {
  return { status: 'started', session, diagnostics: [] };
}

describe('useGameLaunch', () => {
  it('starts idle and becomes running when the backend accepts the launch', async () => {
    mocks.launchGame.mockResolvedValue(started());
    const { result } = renderHook(() => useGameLaunch());
    await waitFor(() => expect(mocks.getLaunchState).toHaveBeenCalled());

    expect(result.current.phase).toBe('idle');

    await act(async () => {
      await result.current.launch(7);
    });

    expect(mocks.launchGame).toHaveBeenCalledWith({ gameId: 7, contentUnitId: null });
    expect(result.current.phase).toBe('running');
    expect(result.current.running).toEqual(session);
    expect(result.current.failure).toBeNull();
  });

  it('reports the launching phase while the backend has not answered', async () => {
    let resolve: ((response: LaunchResponse) => void) | undefined;
    mocks.launchGame.mockImplementation(
      () =>
        new Promise<LaunchResponse>((resolveResponse) => {
          resolve = resolveResponse;
        }),
    );
    const { result } = renderHook(() => useGameLaunch());
    await waitFor(() => expect(mocks.getLaunchState).toHaveBeenCalled());

    let pending: Promise<void>;
    act(() => {
      pending = result.current.launch(7);
    });
    await waitFor(() => expect(result.current.phase).toBe('launching'));
    expect(result.current.pendingGameId).toBe(7);

    await act(async () => {
      resolve?.(started());
      await pending;
    });

    expect(result.current.phase).toBe('running');
  });

  it('surfaces a normalized failure without parsing its message', async () => {
    mocks.launchGame.mockResolvedValue({
      status: 'failed',
      error: {
        code: 'biosMissing',
        message: 'A required BIOS file is missing.',
        context: {
          systemId: 'playstation',
          coreId: null,
          biosRequirementIds: ['playstation-bios'],
          runtimeState: null,
          hostPrerequisite: null,
          exitCode: null,
          contentOptions: [],
        },
      },
    } satisfies LaunchResponse);
    const { result } = renderHook(() => useGameLaunch());
    await waitFor(() => expect(mocks.getLaunchState).toHaveBeenCalled());

    await act(async () => {
      await result.current.launch(7);
    });

    expect(result.current.phase).toBe('idle');
    expect(result.current.failure?.code).toBe('biosMissing');
    expect(result.current.failure?.context.biosRequirementIds).toEqual(['playstation-bios']);

    act(() => result.current.dismissFailure());
    expect(result.current.failure).toBeNull();
  });

  it('offers the launchable content units and relaunches with the chosen one', async () => {
    mocks.launchGame.mockResolvedValueOnce({
      status: 'contentSelectionRequired',
      options: [
        {
          contentUnitId: 3,
          kind: 'chd',
          localTitle: 'Disc 1',
          fileCount: 1,
          availability: 'available',
        },
        {
          contentUnitId: 4,
          kind: 'm3u',
          localTitle: 'Full set',
          fileCount: 3,
          availability: 'available',
        },
      ],
    } satisfies LaunchResponse);
    mocks.launchGame.mockResolvedValueOnce(started());
    const { result } = renderHook(() => useGameLaunch());
    await waitFor(() => expect(mocks.getLaunchState).toHaveBeenCalled());

    await act(async () => {
      await result.current.launch(7);
    });
    expect(result.current.contentOptions).toHaveLength(2);
    expect(result.current.phase).toBe('idle');

    await act(async () => {
      await result.current.launch(7, 4);
    });

    expect(mocks.launchGame).toHaveBeenLastCalledWith({ gameId: 7, contentUnitId: 4 });
    expect(result.current.phase).toBe('running');
    expect(result.current.contentOptions).toBeNull();
  });

  it('returns to a stable state when the backend reports the game exited', async () => {
    mocks.launchGame.mockResolvedValue(started());
    const { result } = renderHook(() => useGameLaunch());
    await waitFor(() => expect(mocks.getLaunchState).toHaveBeenCalled());
    await act(async () => {
      await result.current.launch(7);
    });
    expect(result.current.phase).toBe('running');

    act(() => {
      emit({
        state: { running: null, blocked: false },
        exited: { sessionId: 11, gameId: 7, outcome: 'completed', exitCode: 0 },
      });
    });

    expect(result.current.phase).toBe('idle');
    expect(result.current.running).toBeNull();
    expect(result.current.pendingGameId).toBeNull();
  });

  it('adopts a game that was already running when the application started', async () => {
    mocks.getLaunchState.mockResolvedValue({ running: session, blocked: false });

    const { result } = renderHook(() => useGameLaunch());

    await waitFor(() => expect(result.current.phase).toBe('running'));
    expect(result.current.running).toEqual(session);
  });

  it('reports a blocked launch state without inventing a running session', async () => {
    mocks.getLaunchState.mockResolvedValue({ running: null, blocked: true });

    const { result } = renderHook(() => useGameLaunch());

    await waitFor(() => expect(result.current.blocked).toBe(true));
    expect(result.current.running).toBeNull();
    expect(result.current.phase).toBe('idle');
  });

  it('turns a transport rejection into a normalized internal failure', async () => {
    mocks.launchGame.mockRejectedValue({ code: 'ipc_unavailable', message: 'unreachable' });
    const { result } = renderHook(() => useGameLaunch());
    await waitFor(() => expect(mocks.getLaunchState).toHaveBeenCalled());

    await act(async () => {
      await result.current.launch(7);
    });

    expect(result.current.failure?.code).toBe('internalLaunchFailure');
    expect(result.current.phase).toBe('idle');
  });
});
