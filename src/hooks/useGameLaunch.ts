import { useCallback, useEffect, useRef, useState } from 'react';

import { activeControllerIdentity } from '../input/gamepadQuirks';
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

/**
 * What the transient launch UI is currently presenting, and for whom.
 *
 * This is **presentation ownership only**. It is deliberately not a copy of process state: the
 * backend remains the sole authority on whether a game is running, and nothing here is ever
 * consulted to answer that question. Its job is to make three questions answerable without
 * inference:
 *
 * - which game owns the current transient launch interaction?
 * - is that interaction still open?
 * - may *this* Game Detail route render its transient UI?
 *
 * Without it, `contentOptions` and `failure` were application-global with no owner, so whichever
 * Game Detail route happened to be current rendered them — including a different game's.
 */
export interface LaunchInteraction {
  /** The game whose Game Detail route initiated this launch interaction. */
  gameId: number;
  /** What the interaction is presenting right now. Never process authority. */
  phase: 'pending' | 'contentSelection' | 'failure';
}

export interface GameLaunchModel {
  phase: LaunchPhase;
  /** The game the backend reports as running, whichever screen is open. */
  running: RunningGameSession | null;
  /**
   * A managed process record exists whose identity could not be established. A launch is refused
   * until it is resolved, and no running session can honestly be described.
   */
  blocked: boolean;
  /**
   * The game whose launch request this frontend issued and has not seen resolved yet.
   *
   * It is **global**: while it is set, no Game Detail may issue another launch request, and the
   * application-input ownership predicate treats the launch transition as in progress. Route
   * abandonment does not clear it, because the request really is still in flight.
   */
  pendingGameId: number | null;
  /** Who owns the transient launch UI, or `null` when nothing transient is owned. */
  interaction: LaunchInteraction | null;
  failure: LaunchFailure | null;
  contentOptions: LaunchContentOption[] | null;
  diagnostics: LaunchDiagnostic[];
  launch: (gameId: number, contentUnitId?: number) => Promise<void>;
  dismissFailure: () => void;
  cancelContentSelection: () => void;
  /**
   * The owning Game Detail route left while the interaction was still only frontend-transient.
   *
   * Drops the transient presentation and nothing else. An IPC request cannot be cancelled by
   * deleting frontend state, so the request is still allowed to resolve; what is abandoned is its
   * *presentation*. See the response policy in `launch()`.
   */
  abandonInteraction: () => void;
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
  const [interaction, setInteractionState] = useState<LaunchInteraction | null>(null);
  // The interaction is mirrored into a ref because an in-flight response handler has to read the
  // value that is current when it resolves, not the one captured when the request was issued.
  const interactionRef = useRef<LaunchInteraction | null>(null);
  const setInteraction = useCallback((next: LaunchInteraction | null) => {
    interactionRef.current = next;
    setInteractionState(next);
  }, []);
  // Likewise for the pending id: `launch` is stable, so it cannot close over the state value.
  const pendingRef = useRef<number | null>(null);
  const setPending = useCallback((next: number | null) => {
    pendingRef.current = next;
    setPendingGameId(next);
  }, []);

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
        setPending(null);
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
  }, [setPending]);

  /**
   * Issues one launch request.
   *
   * **Only one frontend launch request may be unresolved at a time.** A second request would make
   * the first response irrelevant to frontend state through the generation counter below, which is
   * exactly the displacement that must not happen: the first request may already have created a
   * real process. The refusal lives here rather than only in the UI, because this is where the
   * invariant belongs. A content-option continuation is *not* a second request — the first one has
   * already resolved by then, so `pendingGameId` is null and the continuation proceeds.
   *
   * **Response policy.** Two independent questions are asked about every response:
   *
   * - `owns()` — is this still the current request of a mounted hook? A response that is not is
   *   irrelevant to every piece of frontend state.
   * - `presents()` — does the transient interaction still belong to the game that asked? Route
   *   abandonment makes this false while `owns()` stays true.
   *
   * `pendingGameId` is cleared on *every* resolution regardless of `presents()`, because the request
   * really did resolve and input ownership depends on that fact. Transient *presentation* — content
   * options, a normalized failure — is written only while `presents()` holds, so an abandoned
   * request can never resurrect its surface on another game's route. An authoritative `started`
   * session is adopted **regardless** of `presents()`: the user did ask for that process, the
   * backend created it, and a route change must never make a real running process disappear.
   */
  const launch = useCallback(
    async (gameId: number, contentUnitId?: number) => {
      if (pendingRef.current !== null) return;

      const generation = requestGeneration.current + 1;
      requestGeneration.current = generation;
      const owns = () => mounted.current && requestGeneration.current === generation;
      const presents = () => interactionRef.current?.gameId === gameId;

      setPending(gameId);
      setInteraction({ gameId, phase: 'pending' });
      setFailure(null);
      setContentOptions(null);
      setDiagnostics([]);

      /** Drops a transient presentation whose owning route has gone. */
      const discardPresentation = () => {
        if (presents()) setInteraction(null);
      };

      try {
        const response = await launchGame({
          gameId,
          contentUnitId: contentUnitId ?? null,
          activeGamepadId: activeControllerIdentity(),
        });
        if (!owns()) return;
        switch (response.status) {
          case 'started':
            // Authoritative, and adopted even if the presentation was abandoned.
            setRunning(response.session);
            setDiagnostics(response.diagnostics);
            setPending(null);
            setInteraction(null);
            break;
          case 'contentSelectionRequired':
            setPending(null);
            if (!presents()) {
              discardPresentation();
              break;
            }
            setContentOptions(response.options);
            setInteraction({ gameId, phase: 'contentSelection' });
            break;
          case 'failed':
            setPending(null);
            if (!presents()) {
              discardPresentation();
              break;
            }
            setFailure(response.error);
            setInteraction({ gameId, phase: 'failure' });
            break;
        }
      } catch (reason: unknown) {
        if (!owns()) return;
        setPending(null);
        if (!presents()) {
          discardPresentation();
          return;
        }
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
        setInteraction({ gameId, phase: 'failure' });
      }
    },
    [setInteraction, setPending],
  );

  const dismissFailure = useCallback(() => {
    setFailure(null);
    setInteraction(null);
  }, [setInteraction]);

  const cancelContentSelection = useCallback(() => {
    setContentOptions(null);
    setInteraction(null);
  }, [setInteraction]);

  /**
   * The owning route left. Only the transient presentation is dropped.
   *
   * `pendingGameId` and `running` are untouched on purpose: a request in flight is still in flight,
   * and releasing input ownership early would hand the controller back to RetroFrontier while
   * RetroArch may already exist.
   */
  const abandonInteraction = useCallback(() => {
    setInteraction(null);
    setContentOptions(null);
    setFailure(null);
  }, [setInteraction]);

  const phase: LaunchPhase =
    running !== null ? 'running' : pendingGameId !== null ? 'launching' : 'idle';

  return {
    phase,
    running,
    blocked,
    pendingGameId,
    interaction,
    failure,
    contentOptions,
    diagnostics,
    launch,
    dismissFailure,
    cancelContentSelection,
    abandonInteraction,
  };
}
