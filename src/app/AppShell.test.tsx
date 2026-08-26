import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AppShell } from './AppShell';

const { getAppInfo, getRuntimeStatus, getSystems } = vi.hoisted(() => ({
  getAppInfo: vi.fn(),
  getRuntimeStatus: vi.fn(),
  getSystems: vi.fn(),
}));

vi.mock('../platform/ipc', () => ({
  IpcError: class IpcError extends Error {
    readonly code: string;

    constructor(code: string, message: string) {
      super(message);
      this.code = code;
    }
  },
  getAppInfo,
  getRuntimeStatus,
  getSystems,
}));

describe('AppShell', () => {
  beforeEach(() => {
    getAppInfo.mockResolvedValue({
      appName: 'RetroFrontier',
      version: '0.1.0',
      platform: 'linux',
      architecture: 'x86_64',
      databaseReady: true,
    });
    getRuntimeStatus.mockResolvedValue({
      state: 'ready',
      installationId: 'install-1',
      releaseId: 'release-1',
      canRollback: false,
      repairRequired: false,
    });
    getSystems.mockResolvedValue({
      runtime: {
        state: 'ready',
        installationId: 'install-1',
        releaseId: 'release-1',
        canRollback: false,
        repairRequired: false,
      },
      biosRoot: '/documents/RetroFrontier/BIOS',
      biosRootStatus: 'ready',
      systems: [
        {
          id: 'nes',
          displayName: 'Nintendo Entertainment System',
          manufacturer: 'Nintendo',
          aliases: ['NES'],
          supportedExtensions: ['.nes'],
          core: {
            policy: {
              defaultCoreId: null,
              approvedCoreIds: [],
              decision: {
                kind: 'unresolved',
                researchItem: 'Default core is unresolved',
              },
            },
            availability: {
              runtimeState: 'ready',
              availableCoreIds: [],
              defaultCoreAvailable: null,
            },
          },
          bios: { policy: 'notRequired', ready: true, requirements: [] },
          readiness: {
            ready: false,
            reasons: [{ kind: 'corePolicyUnresolved', researchItem: 'Default core is unresolved' }],
          },
        },
      ],
    });
  });

  it('renders the empty-library foundation and reports native status', async () => {
    render(<AppShell />);

    expect(screen.getByRole('heading', { name: 'LIBRARY IS EMPTY' })).toBeInTheDocument();
    expect(await screen.findByText('CONNECTED')).toBeInTheDocument();
    expect(screen.getByText('RetroFrontier 0.1.0')).toBeInTheDocument();
    expect(screen.getByText('DATABASE').parentElement).toHaveTextContent('READY');
    expect(screen.getByText('RUNTIME').parentElement).toHaveTextContent('READY');
    expect(await screen.findByRole('heading', { name: 'SUPPORTED SYSTEMS' })).toBeInTheDocument();
    expect(screen.getByText('/documents/RetroFrontier/BIOS')).toBeInTheDocument();
    expect(
      screen.getByText(/system-specific subfolders are not searched yet/i),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: 'Nintendo Entertainment System' }),
    ).toBeInTheDocument();
  });

  it('switches the theme and keeps the selected theme on the document root', () => {
    render(<AppShell />);

    fireEvent.click(screen.getByRole('button', { name: 'LIGHT' }));

    expect(document.documentElement).toHaveAttribute('data-theme', 'light');
    expect(screen.getByRole('button', { name: 'LIGHT' })).toHaveAttribute('aria-pressed', 'true');
  });
});
