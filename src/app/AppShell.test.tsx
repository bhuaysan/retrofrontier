import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { AppShell } from './AppShell';

const { getAppInfo, getRuntimeStatus } = vi.hoisted(() => ({
  getAppInfo: vi.fn(),
  getRuntimeStatus: vi.fn(),
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
  });

  it('renders the empty-library foundation and reports native status', async () => {
    render(<AppShell />);

    expect(screen.getByRole('heading', { name: 'LIBRARY IS EMPTY' })).toBeInTheDocument();
    expect(await screen.findByText('CONNECTED')).toBeInTheDocument();
    expect(screen.getByText('RetroFrontier 0.1.0')).toBeInTheDocument();
    expect(screen.getByText('DATABASE').parentElement).toHaveTextContent('READY');
    expect(screen.getByText('RUNTIME').parentElement).toHaveTextContent('READY');
  });

  it('switches the theme and keeps the selected theme on the document root', () => {
    render(<AppShell />);

    fireEvent.click(screen.getByRole('button', { name: 'LIGHT' }));

    expect(document.documentElement).toHaveAttribute('data-theme', 'light');
    expect(screen.getByRole('button', { name: 'LIGHT' })).toHaveAttribute('aria-pressed', 'true');
  });
});
