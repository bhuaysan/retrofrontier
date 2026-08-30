import { useCallback, useEffect, useRef, useState } from 'react';

import {
  getLaunchState,
  launchGame,
  normalizeIpcError,
  onGameLaunchStateChanged,
  type LaunchContentOption,
  type LaunchDiagnostic,
  type LaunchFailure,
  type RunningGameSession,
} from '../platform/ipc';

export type LaunchPhase = 'idle' | 'launching' | 'running';

export interface GameLaunchModel {
  phase: LaunchPhase;
  /** The game the backend reports as running, whichever screen is open. */
  running: RunningGameSession | null;
  /**
   * A managed process record exists whose identity could not be established. A launch is refused
   * until it is resolved, and no running session can honestly be described.
   */
  blocked: boolean;
  /** The game this screen is currently waiting on. */
  pendingGameId: number | null;
  failure: LaunchFailure | null;
  contentOptions: LaunchContentOption[] | null;
  diagnostics: LaunchDiagnostic[];
  launch: (gameId: number, contentUnitId?: number) => Promise<void>;
  dismissFailure: () => void;
  cancelContentSelection: () => void;
}

/**
 * Owns the M7 launch interaction for the shell.
 *
 * The backend is authoritative: this hook reads the durable launch state on mount, follows the
 * launch-state event, and never infers that a game stopped from a local timer. A launch failure is
 * kept as its normalized code so the UI can choose copy without parsing text.
 */
export function useGameLaunch(): GameLaunchModel {
  const mounted = useRef(true);
  const requestGeneration = useRef(0);
  const [running, setRunning] = useState<RunningGameSession | null>(null);
  const [blocked, setBlocked] = useState(false);
  const [pendingGameId, setPendingGameId] = useState<number | null>(null);
  const [failure, setFailure] = useState<LaunchFailure | null>(null);
  const [contentOptions, setContentOptions] = useState<LaunchContentOption[] | null>(null);
  const [diagnostics, setDiagnostics] = useState<LaunchDiagnostic[]>([]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      requestGeneration.current += 1;
    };
  }, []);

  useEffect(() => {
    let disposed = false;

    void getLaunchState()
      .then((state) => {
        if (disposed || !mounted.current) return;
        setRunning(state.running);
        setBlocked(state.blocked);
      })
      .catch(() => undefined);

    let unlisten: (() => void) | undefined;
    const subscription = onGameLaunchStateChanged((event) => {
      if (disposed || !mounted.current) return;
      setRunning(event.state.running);
      setBlocked(event.state.blocked);
      if (event.state.running === null) {
        setPendingGameId(null);
        setDiagnostics([]);
      }
    })
      .then((next) => {
        if (disposed) next();
        else unlisten = next;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
      void subscription;
    };
  }, []);

  const launch = useCallback(async (gameId: number, contentUnitId?: number) => {
    const generation = requestGeneration.current + 1;
    requestGeneration.current = generation;
    const owns = () => mounted.current && requestGeneration.current === generation;

    setPendingGameId(gameId);
    setFailure(null);
    setContentOptions(null);
    setDiagnostics([]);

    try {
      const response = await launchGame({
        gameId,
        contentUnitId: contentUnitId ?? null,
      });
      if (!owns()) return;
      switch (response.status) {
        case 'started':
          setRunning(response.session);
          setDiagnostics(response.diagnostics);
          break;
        case 'contentSelectionRequired':
          setContentOptions(response.options);
          setPendingGameId(null);
          break;
        case 'failed':
          setFailure(response.error);
          setPendingGameId(null);
          break;
      }
    } catch (reason: unknown) {
      if (!owns()) return;
      // Only a transport-level rejection reaches this branch; every launch problem is a
      // normalized response. The generic code keeps the UI honest about which it was.
      const error = normalizeIpcError(reason);
      setFailure({
        code: 'internalLaunchFailure',
        message: error.message,
        context: {
          systemId: null,
          coreId: null,
          biosRequirementIds: [],
          runtimeState: null,
          hostPrerequisite: null,
          exitCode: null,
          contentOptions: [],
        },
      });
      setPendingGameId(null);
    }
  }, []);

  const dismissFailure = useCallback(() => setFailure(null), []);
  const cancelContentSelection = useCallback(() => setContentOptions(null), []);

  const phase: LaunchPhase =
    running !== null ? 'running' : pendingGameId !== null ? 'launching' : 'idle';

  return {
    phase,
    running,
    blocked,
    pendingGameId,
    failure,
    contentOptions,
    diagnostics,
    launch,
    dismissFailure,
    cancelContentSelection,
  };
}
