import { useEffect, useRef } from 'react';

import { useFocusApi, useFocusedNodeId } from '../focus/focusContext';
import type { FocusNodeId } from '../focus/focusNodes';
import { requestAppWindowFocus } from '../platform/appWindow';
import type { RunningGameSession } from '../platform/ipc';

interface UseLaunchFocusReturnOptions {
  /** The backend-owned running session. React never infers this from a timer. */
  running: RunningGameSession | null;
  /** A managed process exists whose identity could not be established. */
  blocked: boolean;
  windowFocused: boolean;
  /** Used when the target the launch started from no longer exists. */
  fallbackNodeId: FocusNodeId;
}

/**
 * Returns RetroFrontier to the foreground once a managed game has really ended.
 *
 * The backend is the only authority on whether the game is still running. When it reports that the
 * session ended, the window is asked to come forward exactly once — no retry loop, and no repeated
 * foreground stealing — and DOM focus is only restored after the application window actually owns
 * focus again, so focus is never handed to an invisible window.
 */
export function useLaunchFocusReturn({
  running,
  blocked,
  windowFocused,
  fallbackNodeId,
}: UseLaunchFocusReturnOptions): void {
  const api = useFocusApi();
  const focusedNodeId = useFocusedNodeId();
  const launchOrigin = useRef<FocusNodeId | null>(null);
  const previousRunning = useRef<RunningGameSession | null>(running);
  const pendingReturn = useRef<FocusNodeId | null>(null);
  const requestedForSession = useRef<number | null>(null);

  const focusedRef = useRef(focusedNodeId);
  useEffect(() => {
    focusedRef.current = focusedNodeId;
  }, [focusedNodeId]);

  useEffect(() => {
    // While launch state is blocked it is uncertain, so the last known session is held rather than
    // consumed: a return is only performed once the backend can describe the state honestly again.
    if (blocked) return;
    const previous = previousRunning.current;
    previousRunning.current = running;
    if (previous === null && running !== null) {
      // The target the launch was started from, captured once. It deliberately does not follow
      // focus afterwards: the run belongs to RetroArch, not to RetroFrontier's focus.
      launchOrigin.current = focusedRef.current;
      return;
    }
    if (previous === null || running !== null) return;
    if (requestedForSession.current === previous.sessionId) return;

    requestedForSession.current = previous.sessionId;
    pendingReturn.current = launchOrigin.current ?? fallbackNodeId;
    launchOrigin.current = null;
    void requestAppWindowFocus();
  }, [blocked, fallbackNodeId, running]);

  useEffect(() => {
    if (pendingReturn.current === null || !windowFocused) return;
    const target = pendingReturn.current;
    pendingReturn.current = null;
    api.requestFocus(target, { fallback: fallbackNodeId, resolveOnRegister: true });
  }, [api, fallbackNodeId, windowFocused]);
}
