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

  it('never offers installation when no approved release source is configured', () => {
    const summary = runtimeSummary(state({ sourceConfigured: false, sourceOrigin: null }));

    expect(summary.action).toBe('none');
    expect(summary.disabledReason).not.toBeNull();
    // The panel must say why rather than showing a button that cannot work.
    expect(summary.detail).toContain('no approved managed RetroArch release source');
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
