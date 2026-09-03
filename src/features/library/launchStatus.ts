import type { LaunchErrorCode, LaunchFailure } from '../../platform/ipc';

/**
 * Presentation for the normalized M7 launch failures.
 *
 * The heading comes from the stable code and the explanation from the message Rust generated, so
 * the UI never parses text and never invents a second source of truth for what went wrong. The
 * hint is the one actionable next step for that code.
 */
export function launchFailureTitle(code: LaunchErrorCode): string {
  switch (code) {
    case 'gameNotFound':
      return 'GAME NOT FOUND';
    case 'gameUnavailable':
    case 'contentUnavailable':
      return 'CONTENT UNAVAILABLE';
    case 'contentSelectionRequired':
      return 'CHOOSE A VERSION';
    case 'runtimeNotReady':
      return 'RUNTIME NOT READY';
    case 'corePolicyUnresolved':
      return 'SYSTEM NOT SUPPORTED YET';
    case 'coreNotInstalled':
      return 'CORE NOT INSTALLED';
    case 'coreNotApproved':
      return 'CORE NOT APPROVED';
    case 'biosMissing':
      return 'BIOS MISSING';
    case 'biosInvalid':
      return 'BIOS NOT RECOGNIZED';
    case 'biosNotCoveredByCatalog':
      return 'BIOS IDENTITY UNKNOWN';
    case 'hostPrerequisiteMissing':
      return 'SYSTEM REQUIREMENT MISSING';
    case 'gameAlreadyRunning':
      return 'A GAME IS ALREADY RUNNING';
    case 'processExitedDuringLaunch':
      return 'RETROARCH STOPPED IMMEDIATELY';
    // A refusal *before* anything was spawned: RetroFrontier could not prepare save-state
    // tracking for the session, so it declined to start the game rather than run one it could not
    // account for. Nothing about the game, its content, or the runtime is implicated.
    case 'saveStateBaselineFailed':
      return 'SAVE STATE TRACKING UNAVAILABLE';
    case 'configPreparationFailed':
    case 'spawnFailed':
    case 'processIdentityFailed':
    case 'sessionPersistenceFailed':
    case 'internalLaunchFailure':
      return 'LAUNCH FAILED';
  }
}

export function launchFailureHint(code: LaunchErrorCode): string | null {
  switch (code) {
    case 'gameNotFound':
      return 'Rescan the library and open the game again.';
    case 'gameUnavailable':
    case 'contentUnavailable':
      return 'Check that the game files are still in their content folder, then rescan.';
    case 'runtimeNotReady':
      return 'Open Settings to check the managed runtime.';
    case 'corePolicyUnresolved':
      return 'RetroFrontier will support this system once its core has been approved.';
    case 'coreNotInstalled':
    case 'coreNotApproved':
      return 'Open Settings to check the managed runtime and its installed cores.';
    case 'biosMissing':
    case 'biosInvalid':
    case 'biosNotCoveredByCatalog':
      return 'Check the BIOS requirements listed above, then try again.';
    case 'hostPrerequisiteMissing':
      return 'Start RetroFrontier from a normal graphical desktop session.';
    case 'gameAlreadyRunning':
      return 'Close the running game before starting another one.';
    case 'processExitedDuringLaunch':
      return 'The RetroArch log in the RetroFrontier logs folder has the details.';
    case 'saveStateBaselineFailed':
      return 'The game was not started and nothing was changed. Try again; the RetroFrontier logs folder has the details if it persists.';
    case 'contentSelectionRequired':
    case 'configPreparationFailed':
    case 'spawnFailed':
    case 'processIdentityFailed':
    case 'sessionPersistenceFailed':
    case 'internalLaunchFailure':
      return null;
  }
}

/** The exit code is shown only when the backend actually reported one. */
export function launchFailureExitCode(failure: LaunchFailure): number | null {
  return failure.context.exitCode;
}
