import type {
  RuntimeInstallErrorCode,
  RuntimeInstallState,
  RuntimeSourceOrigin,
  RuntimeState,
} from '../../platform/ipc';

/** What the runtime panel should offer the user right now. */
export type RuntimeAction = 'install' | 'repair' | 'none';

export interface RuntimeSummary {
  /** Short state label shown beside the panel heading. */
  badge: string;
  /** One sentence describing the current state truthfully. */
  detail: string;
  action: RuntimeAction;
  actionLabel: string;
  /**
   * Why the offered action cannot be taken, or `null` when it can. A disabled action always has a
   * reason: an unexplained dead button is worse than no button.
   */
  disabledReason: string | null;
}

export function runtimeStateLabel(state: RuntimeState): string {
  switch (state) {
    case 'notInstalled':
      return 'NOT INSTALLED';
    case 'ready':
      return 'READY';
    case 'installing':
      return 'INSTALLING';
    case 'updating':
      return 'UPDATING';
    case 'repairing':
      return 'REPAIRING';
    case 'broken':
      return 'REPAIR REQUIRED';
    case 'rollbackAvailable':
      return 'READY';
  }
}

export function runtimeSourceLabel(origin: RuntimeSourceOrigin): string {
  switch (origin) {
    case 'production':
      return 'APPROVED RELEASE CHANNEL';
    case 'qualification':
      // Never presented as a public release: a maintainer must be able to tell at a glance that
      // this build installs from a locally published qualification repository.
      return 'LOCAL QUALIFICATION REPOSITORY';
  }
}

/**
 * Whether the runtime can be changed, which is not the same question as whether it works.
 *
 * An installed runtime stays valid and playable after the build that installed it loses its
 * release source, so a missing source must never be reported as though nothing were installed.
 * It only removes the ability to install, reinstall, or repair.
 */
const NO_SOURCE_INSTALL_REASON = 'No approved managed release source is configured.';
const NO_SOURCE_MUTATION_REASON =
  'No approved release source is configured, so this runtime cannot currently be reinstalled or repaired.';
const NO_SOURCE_REPAIR_REASON =
  'No approved release source is configured, so this runtime cannot currently be repaired.';

export function runtimeSummary(state: RuntimeInstallState): RuntimeSummary {
  const badge = runtimeStateLabel(state.status.state);
  // The verified state RuntimeManager reported is what the panel describes; the source only
  // decides which actions are offered. Files on disk never stand in for that verification.
  const canMutate = state.sourceConfigured;

  if (state.installing) {
    return {
      badge: 'INSTALLING',
      detail:
        'RetroFrontier is downloading, verifying, and installing the managed RetroArch runtime.',
      action: 'none',
      actionLabel: 'INSTALLING…',
      disabledReason: 'An installation is already running.',
    };
  }

  switch (state.status.state) {
    case 'notInstalled':
      return {
        badge,
        detail: canMutate
          ? 'No managed RetroArch runtime is installed. RetroFrontier will download and verify the approved release before any game can start.'
          : 'This build has no approved managed RetroArch release source, so the runtime cannot be installed yet.',
        action: canMutate ? 'install' : 'none',
        actionLabel: 'INSTALL RUNTIME',
        disabledReason: canMutate ? null : NO_SOURCE_INSTALL_REASON,
      };
    case 'broken':
      return {
        badge,
        detail: canMutate
          ? 'The managed RetroArch runtime failed verification. RetroFrontier will rebuild the approved release into a fresh installation.'
          : 'The managed RetroArch runtime failed verification and cannot be used until it is reinstalled.',
        action: canMutate ? 'repair' : 'none',
        actionLabel: 'REPAIR RUNTIME',
        disabledReason: canMutate ? null : NO_SOURCE_REPAIR_REASON,
      };
    case 'ready':
    case 'rollbackAvailable':
      return {
        badge,
        detail: 'The managed RetroArch runtime is installed and verified.',
        action: canMutate ? 'repair' : 'none',
        actionLabel: 'REINSTALL RUNTIME',
        disabledReason: canMutate ? null : NO_SOURCE_MUTATION_REASON,
      };
    case 'installing':
    case 'updating':
    case 'repairing':
      return {
        badge,
        detail: 'A managed runtime operation is in progress.',
        action: 'none',
        actionLabel: 'INSTALL RUNTIME',
        disabledReason: 'A managed runtime operation is already running.',
      };
  }
}

/**
 * Whether retrying the same action could plausibly succeed without the user changing something.
 *
 * A refused install must not invite a pointless retry loop: an unconfigured source and an
 * unsupported platform will never change by clicking again, and a running game needs the user to
 * close it first.
 */
export function isRetryableInstallError(code: RuntimeInstallErrorCode): boolean {
  switch (code) {
    case 'sourceNotConfigured':
    case 'unsupportedPlatform':
    case 'gameRunning':
    case 'installationInProgress':
      return false;
    case 'releaseNotTrusted':
    case 'downloadFailed':
    case 'verificationFailed':
    case 'extractionFailed':
    case 'storageLimit':
    case 'installationFailed':
      return true;
  }
}

export function installErrorTitle(code: RuntimeInstallErrorCode): string {
  switch (code) {
    case 'sourceNotConfigured':
      return 'NO RELEASE SOURCE';
    case 'installationInProgress':
      return 'ALREADY INSTALLING';
    case 'gameRunning':
      return 'GAME RUNNING';
    case 'releaseNotTrusted':
      return 'RELEASE NOT TRUSTED';
    case 'downloadFailed':
      return 'DOWNLOAD FAILED';
    case 'verificationFailed':
      return 'VERIFICATION FAILED';
    case 'extractionFailed':
      return 'INSTALL FAILED';
    case 'storageLimit':
      return 'NOT ENOUGH STORAGE';
    case 'unsupportedPlatform':
      return 'UNSUPPORTED SYSTEM';
    case 'installationFailed':
      return 'INSTALL FAILED';
  }
}
