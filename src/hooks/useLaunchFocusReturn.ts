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
  /** A launch request this frontend issued has not resolved yet. */
  pendingGameId: number | null;
  /**
   * The launch is waiting for the user to choose a content unit. The launch interaction is still the
   * same one: it has not resolved, and it may still continue into a running process.
   */
  contentSelectionOpen: boolean;
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

/** A return that has been created and is waiting for the application window to own focus. */
interface PendingReturn {
  sessionId: number;
  origin: LaunchOrigin | null;
}

export interface LaunchFocusReturn {
  /**
   * Marks the start of a launch *interaction* and records its origin.
   *
   * Call it synchronously wherever the UI issues a launch — that is the moment of the user's
   * intent. A multi-step launch (PLAY, then `contentSelectionRequired`, then choosing a version) is
   * **one** interaction: a call made while an interaction is still open continues it and
   * deliberately does not re-capture, because the temporary content-option node the user confirmed
   * from no longer exists when RetroArch exits.
   */
  beginLaunchInteraction: () => void;
}

/**
 * Returns RetroFrontier to the foreground once a managed game has really ended.
 *
 * Four things are deliberately kept apart:
 *
 * 1. **Launch interaction origin.** Captured synchronously by `beginLaunchInteraction()` when the UI
 *    issues the launch. Sampling whichever node happens to be focused when `running` later arrives
 *    is not the same moment: focus, and even the route, can change in between, so the recorded
 *    "origin" could belong to something the user never launched from. The origin belongs to the
 *    whole interaction, so a content-selection continuation keeps the identity the user really
 *    launched from rather than the temporary option node.
 * 2. **Interaction lifetime.** An interaction that resolves without ever starting a process — a
 *    normalized failure, a cancelled content selection, a transport error — owns no return, so its
 *    origin is dropped instead of being kept indefinitely and contaminating the next, independent
 *    launch.
 * 3. **Backend running state.** The backend remains the only authority on whether the game is still
 *    running. When it reports the session ended, the window is asked to come forward exactly once —
 *    no retry loop, and no repeated foreground stealing.
 * 4. **Post-process return target.** Resolved against the route that is current *when the window
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
  pendingGameId,
  contentSelectionOpen,
  windowFocused,
  routeKey,
  fallbackNodeId,
}: UseLaunchFocusReturnOptions): LaunchFocusReturn {
  const api = useFocusApi();
  /**
   * The origin of the launch interaction that has not resolved yet, or `null` when no interaction is
   * in flight. Being non-null *is* the "an interaction is already open" marker, which is what makes
   * a content-selection continuation part of the same interaction rather than a new one.
   */
  const interactionOrigin = useRef<LaunchOrigin | null>(null);
  const previousRunning = useRef<RunningGameSession | null>(running);
  const requestedForSession = useRef<number | null>(null);
  /**
   * The return that is waiting for window focus, and the state that makes it *observable*.
   *
   * The generation is the reactivity: the transition that creates a return — the backend reporting
   * that the managed session ended — must itself make the restore path runnable, because the window
   * may already be focused at that moment and no further focus change will ever arrive. A return
   * recorded only in a ref could never schedule its own restore and would stay pending forever.
   * The payload stays in a ref so consuming it needs no second state update and no extra render.
   */
  const pendingReturn = useRef<PendingReturn | null>(null);
  const [returnGeneration, setReturnGeneration] = useState(0);

  // The current route and its deterministic target are mirrored into refs so both the synchronous
  // capture callback and the return effect read the values that are current *at the moment they
  // run*, without making either of them re-run on every route change.
  const routeKeyRef = useRef(routeKey);
  const fallbackRef = useRef(fallbackNodeId);
  useEffect(() => {
    routeKeyRef.current = routeKey;
    fallbackRef.current = fallbackNodeId;
  }, [fallbackNodeId, routeKey]);

  const beginLaunchInteraction = useCallback(() => {
    // A continuation of an interaction that is still open must not overwrite the origin the user
    // actually launched from. Only a genuinely new interaction captures.
    if (interactionOrigin.current !== null) return;
    interactionOrigin.current = { nodeId: api.getFocusedNodeId(), routeKey: routeKeyRef.current };
  }, [api]);

  useEffect(() => {
    // While launch state is blocked it is uncertain, so the last known session *and* the open
    // interaction are both held rather than consumed: a return is only performed, and an
    // interaction only discarded, once the backend can describe the state honestly again.
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
      const origin = interactionOrigin.current;
      interactionOrigin.current = null;
      pendingReturn.current = { sessionId: previous.sessionId, origin };
      setReturnGeneration((generation) => generation + 1);
      void requestAppWindowFocus();
      return;
    }

    // The interaction resolved without ever starting a process: a normalized failure, a cancelled
    // content selection, or a transport error. There will be no return, so the origin is dropped
    // here rather than surviving into the next, independent launch. A `contentSelectionRequired`
    // response is deliberately *not* a resolution — the same interaction continues through it.
    if (running === null && pendingGameId === null && !contentSelectionOpen) {
      interactionOrigin.current = null;
    }
  }, [blocked, contentSelectionOpen, pendingGameId, running]);

  useEffect(() => {
    const pending = pendingReturn.current;
    if (pending === null) return;
    // No DOM focus is stolen into a window the compositor never brought forward.
    if (!windowFocused) return;
    // Consumed once. A later rerender, route change, or focus change finds nothing pending, so the
    // restoration cannot repeat and cannot fight a focus the user has since moved themselves.
    pendingReturn.current = null;
    const { origin } = pending;
    const fallback = fallbackRef.current;

    // The origin is only meaningful while its own route is still on screen. Otherwise the user
    // moved on during the run, and the honest target is where they are now.
    if (origin !== null && origin.nodeId !== null && origin.routeKey === routeKeyRef.current) {
      api.requestFocus(origin.nodeId, { fallback, resolveOnRegister: true });
      return;
    }
    api.requestFocus(fallback, { resolveOnRegister: true });
    // `returnGeneration` is a reactivity token, not a value this effect reads: it is what makes the
    // exit transition itself schedule this effect.
  }, [api, returnGeneration, windowFocused]);

  return { beginLaunchInteraction };
}
