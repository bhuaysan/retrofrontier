import type { RunningGameSession } from '../platform/ipc';

/**
 * The backend-owned launch facts plus the native window state that decide whether RetroFrontier may
 * consume application input. Every field comes from an authoritative source: the Tauri window for
 * focus, the M7 launch state for the rest. Nothing here is inferred from a timer.
 */
export interface ApplicationInputOwnership {
  /** The application window is confirmed focused. Unknown native state is not "focused". */
  windowFocused: boolean;
  /** The backend-reported running managed game. */
  running: RunningGameSession | null;
  /** A managed process record exists whose identity could not be established. */
  blocked: boolean;
  /** A launch request this frontend issued has not resolved yet. */
  pendingGameId: number | null;
}

/**
 * The one application-input ownership predicate.
 *
 * It is deliberately conservative about the launch transition. M7 creates the process and settles
 * the launch inside the backend, so between the request and the authoritative `running` state there
 * is an interval where React still sees `running === null` while RetroArch may already exist. During
 * that interval `pendingGameId` is set, and RetroFrontier must not consume controller input that
 * belongs to the emulator. Ownership is released at the launch *request*, not when the running state
 * finally arrives.
 *
 * It returns as soon as the backend can describe the state honestly again: a failed launch and a
 * `contentSelectionRequired` response both clear `pendingGameId` without starting a process, so the
 * user can immediately interact with the failure or the content-selection surface.
 *
 * There is exactly one predicate on purpose — a second copy of this rule somewhere else would drift.
 */
export function ownsApplicationInput(state: ApplicationInputOwnership): boolean {
  return (
    state.windowFocused && !state.blocked && state.running === null && state.pendingGameId === null
  );
}
