import { useCallback, useEffect, useRef, useState } from 'react';

import { useFocusApi } from '../focus/focusContext';
import type { FocusNodeId } from '../focus/focusNodes';
import { requestAppWindowFocus } from '../platform/appWindow';
import type { RunningGameSession } from '../platform/ipc';

interface UseLaunchFocusReturnOptions {
  /** The backend-owned running session. React never infers this from a timer. */
  running: RunningGameSession | null;
  /** A managed process exists whose identity could not be established. */
  blocked: boolean;
  windowFocused: boolean;
  /**
   * The logical identity of the route or scope currently on screen. A recorded launch origin is only
   * restored while this is still the same context the launch was started from.
   */
  routeKey: string;
  /** The deterministic target of the **current** route, used when the origin is not restorable. */
  fallbackNodeId: FocusNodeId;
}

/** What the launch was started from: a semantic identity plus the context it belonged to. */
interface LaunchOrigin {
  nodeId: FocusNodeId | null;
  routeKey: string;
}

/**
 * The return lifecycle, as observable state rather than a ref.
 *
 * It is state on purpose. The transition that *creates* a return — the backend reporting that the
 * managed session ended — must itself make the restore path runnable, because the window may
 * already be focused at that moment and no further focus change will ever arrive. A ref mutation
 * cannot schedule the restore, so a return recorded that way stays pending forever.
 */
type ReturnPhase =
  | { kind: 'idle' }
  | { kind: 'waitingForWindow'; sessionId: number; origin: LaunchOrigin | null };

export interface LaunchFocusReturn {
  /**
   * Records the semantic launch origin. Call it synchronously where the UI initiates a launch —
   * that is the moment of the user's intent, and it is the only moment at which the origin is
   * unambiguous.
   */
  captureLaunchOrigin: () => void;
}

/**
 * Returns RetroFrontier to the foreground once a managed game has really ended.
 *
 * Three things are deliberately kept apart:
 *
 * 1. **Launch intent origin.** Captured synchronously by `captureLaunchOrigin()` when the UI issues
 *    the launch. Sampling whichever node happens to be focused when `running` later arrives is not
 *    the same moment: focus, and even the route, can change in between, so the recorded "origin"
 *    could belong to something the user never launched from.
 * 2. **Backend running state.** The backend remains the only authority on whether the game is still
 *    running. When it reports the session ended, the window is asked to come forward exactly once —
 *    no retry loop, and no repeated foreground stealing.
 * 3. **Post-process return target.** Resolved against the route that is current *when the window
 *    really comes back*. If the user navigated elsewhere while the game ran they are not dragged
 *    back: the current route's own deterministic target is used instead, and no obsolete request is
 *    left pending in another route.
 *
 * DOM focus is only restored after the application window actually owns focus, so focus is never
 * handed to an invisible window — and it is restored immediately if the window already owns focus
 * when the process ends.
 */
export function useLaunchFocusReturn({
  running,
  blocked,
  windowFocused,
  routeKey,
  fallbackNodeId,
}: UseLaunchFocusReturnOptions): LaunchFocusReturn {
  const api = useFocusApi();
  const launchOrigin = useRef<LaunchOrigin | null>(null);
  const previousRunning = useRef<RunningGameSession | null>(running);
  const requestedForSession = useRef<number | null>(null);
  const [returnPhase, setReturnPhase] = useState<ReturnPhase>({ kind: 'idle' });

  // The current route and its deterministic target are mirrored into refs so both the synchronous
  // capture callback and the return effect read the values that are current *at the moment they
  // run*, without making either of them re-run on every route change.
  const routeKeyRef = useRef(routeKey);
  const fallbackRef = useRef(fallbackNodeId);
  useEffect(() => {
    routeKeyRef.current = routeKey;
    fallbackRef.current = fallbackNodeId;
  }, [fallbackNodeId, routeKey]);

  const captureLaunchOrigin = useCallback(() => {
    launchOrigin.current = { nodeId: api.getFocusedNodeId(), routeKey: routeKeyRef.current };
  }, [api]);

  useEffect(() => {
    // While launch state is blocked it is uncertain, so the last known session is held rather than
    // consumed: a return is only performed once the backend can describe the state honestly again.
    if (blocked) return;
    const previous = previousRunning.current;
    previousRunning.current = running;

    if (
      previous !== null &&
      running === null &&
      requestedForSession.current !== previous.sessionId
    ) {
      // A managed session really ended. The pending return becomes observable in the same
      // transition, so the restore runs even with the window already focused.
      requestedForSession.current = previous.sessionId;
      const origin = launchOrigin.current;
      launchOrigin.current = null;
      setReturnPhase({ kind: 'waitingForWindow', sessionId: previous.sessionId, origin });
      void requestAppWindowFocus();
    }
  }, [blocked, running]);

  useEffect(() => {
    if (returnPhase.kind !== 'waitingForWindow') return;
    // No DOM focus is stolen into a window the compositor never brought forward.
    if (!windowFocused) return;
    setReturnPhase({ kind: 'idle' });
    const { origin } = returnPhase;
    const fallback = fallbackRef.current;

    // The origin is only meaningful while its own route is still on screen. Otherwise the user
    // moved on during the run, and the honest target is where they are now.
    if (origin !== null && origin.nodeId !== null && origin.routeKey === routeKeyRef.current) {
      api.requestFocus(origin.nodeId, { fallback, resolveOnRegister: true });
      return;
    }
    api.requestFocus(fallback, { resolveOnRegister: true });
  }, [api, returnPhase, windowFocused]);

  return { captureLaunchOrigin };
}
