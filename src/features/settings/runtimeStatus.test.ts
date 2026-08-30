import { describe, expect, it } from 'vitest';

import type { RuntimeInstallState, RuntimeState } from '../../platform/ipc';
import {
  installErrorTitle,
  isRetryableInstallError,
  runtimeSourceLabel,
  runtimeStateLabel,
  runtimeSummary,
} from './runtimeStatus';

function state(overrides: Partial<RuntimeInstallState> = {}): RuntimeInstallState {
  return {
    status: {
      state: 'notInstalled',
      installationId: null,
      releaseId: null,
      canRollback: false,
      repairRequired: false,
    },
    sourceConfigured: true,
    sourceOrigin: 'qualification',
    releaseTarget: 'rf-runtime-linux-x86_64-001.manifest.json',
    installing: false,
    ...overrides,
  };
}

describe('runtimeSummary', () => {
  it('offers installation when nothing is installed and a source exists', () => {
    const summary = runtimeSummary(state());

    expect(summary.badge).toBe('NOT INSTALLED');
    expect(summary.action).toBe('install');
    expect(summary.actionLabel).toBe('INSTALL RUNTIME');
    expect(summary.disabledReason).toBeNull();
  });

  it('never offers installation when nothing is installed and no release source is configured', () => {
    const summary = runtimeSummary(state({ sourceConfigured: false, sourceOrigin: null }));

    expect(summary.badge).toBe('NOT INSTALLED');
    expect(summary.action).toBe('none');
    expect(summary.disabledReason).not.toBeNull();
    // The panel must say why rather than showing a button that cannot work.
    expect(summary.detail).toContain('no approved managed RetroArch release source');
  });

  it('still reports a verified runtime as ready when no release source is configured', () => {
    // Realistic after M7.5: the qualification runtime stays installed and playable while the
    // build that installed it is started without its release source. Usability and the ability
    // to reinstall are separate questions, and only the second one is lost.
    const summary = runtimeSummary(
      state({
        sourceConfigured: false,
        sourceOrigin: null,
        status: {
          state: 'ready',
          installationId: 'installation-1',
          releaseId: 'rf-runtime-1.22.2-linux-x86_64-001',
          canRollback: false,
          repairRequired: false,
        },
      }),
    );

    expect(summary.badge).toBe('READY');
    expect(summary.detail).toBe('The managed RetroArch runtime is installed and verified.');
    // No install or repair may be offered, and the panel says why the runtime cannot be changed.
    expect(summary.action).toBe('none');
    expect(summary.disabledReason).toBe(
      'No approved release source is configured, so this runtime cannot currently be reinstalled or repaired.',
    );
  });

  it('reports a broken runtime truthfully but cannot repair it without a release source', () => {
    const summary = runtimeSummary(
      state({
        sourceConfigured: false,
        sourceOrigin: null,
        status: {
          state: 'broken',
          installationId: 'installation-1',
          releaseId: null,
          canRollback: false,
          repairRequired: true,
        },
      }),
    );

    expect(summary.badge).toBe('REPAIR REQUIRED');
    expect(summary.detail).toContain('failed verification');
    expect(summary.action).toBe('none');
    expect(summary.disabledReason).toBe(
      'No approved release source is configured, so this runtime cannot currently be repaired.',
    );
  });

  it('reports progress instead of a second install button while one is running', () => {
    const summary = runtimeSummary(state({ installing: true }));

    expect(summary.badge).toBe('INSTALLING');
    expect(summary.action).toBe('none');
    expect(summary.disabledReason).toBe('An installation is already running.');
  });

  it('offers repair for a broken runtime and reinstall for a ready one', () => {
    const broken = runtimeSummary(
      state({
        status: {
          state: 'broken',
          installationId: null,
          releaseId: null,
          canRollback: false,
          repairRequired: true,
        },
      }),
    );
    expect(broken.action).toBe('repair');
    expect(broken.actionLabel).toBe('REPAIR RUNTIME');

    const ready = runtimeSummary(
      state({
        status: {
          state: 'ready',
          installationId: 'installation-1',
          releaseId: 'rf-runtime-1.22.2-linux-x86_64-001',
          canRollback: false,
          repairRequired: false,
        },
      }),
    );
    expect(ready.action).toBe('repair');
    expect(ready.actionLabel).toBe('REINSTALL RUNTIME');
    expect(ready.badge).toBe('READY');
    expect(ready.detail).toBe('The managed RetroArch runtime is installed and verified.');
    expect(ready.disabledReason).toBeNull();
  });

  it('gives every runtime state a summary that stays honest without a release source', () => {
    const states: RuntimeState[] = [
      'notInstalled',
      'ready',
      'installing',
      'updating',
      'repairing',
      'broken',
      'rollbackAvailable',
    ];
    for (const value of states) {
      const summary = runtimeSummary(
        state({
          sourceConfigured: false,
          sourceOrigin: null,
          status: {
            state: value,
            installationId: null,
            releaseId: null,
            canRollback: false,
            repairRequired: false,
          },
        }),
      );
      // Without a source nothing may be installed, reinstalled, or repaired, and every disabled
      // action carries its reason.
      expect(summary.action).toBe('none');
      expect(summary.disabledReason).not.toBeNull();
      expect(summary.badge).toBe(runtimeStateLabel(value));
    }
  });

  it('gives every runtime state a label and a summary', () => {
    const states: RuntimeState[] = [
      'notInstalled',
      'ready',
      'installing',
      'updating',
      'repairing',
      'broken',
      'rollbackAvailable',
    ];
    for (const value of states) {
      expect(runtimeStateLabel(value)).not.toBe('');
      const summary = runtimeSummary(
        state({
          status: {
            state: value,
            installationId: null,
            releaseId: null,
            canRollback: false,
            repairRequired: false,
          },
        }),
      );
      expect(summary.detail).not.toBe('');
      if (summary.action === 'none') {
        expect(summary.disabledReason).not.toBeNull();
      }
    }
  });
});

describe('release source labelling', () => {
  it('never presents a qualification repository as a public release channel', () => {
    expect(runtimeSourceLabel('qualification')).toBe('LOCAL QUALIFICATION REPOSITORY');
    expect(runtimeSourceLabel('production')).toBe('APPROVED RELEASE CHANNEL');
  });
});

describe('install error handling', () => {
  it('does not invite a retry for problems a retry cannot fix', () => {
    expect(isRetryableInstallError('sourceNotConfigured')).toBe(false);
    expect(isRetryableInstallError('unsupportedPlatform')).toBe(false);
    expect(isRetryableInstallError('gameRunning')).toBe(false);
    expect(isRetryableInstallError('installationInProgress')).toBe(false);
  });

  it('invites a retry for transient acquisition and verification failures', () => {
    expect(isRetryableInstallError('downloadFailed')).toBe(true);
    expect(isRetryableInstallError('verificationFailed')).toBe(true);
    expect(isRetryableInstallError('releaseNotTrusted')).toBe(true);
    expect(isRetryableInstallError('storageLimit')).toBe(true);
  });

  it('titles a running game distinctly from a generic failure', () => {
    expect(installErrorTitle('gameRunning')).toBe('GAME RUNNING');
    expect(installErrorTitle('installationFailed')).toBe('INSTALL FAILED');
  });
});
