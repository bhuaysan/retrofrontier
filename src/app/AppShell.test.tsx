import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { StrictMode } from 'react';

import type { ContentRoot, ScanProgress, ScanSummary } from '../platform/ipc';
import { AppShell } from './AppShell';

const mocks = vi.hoisted(() => {
  class MockIpcError extends Error {
    readonly code: string;

    constructor(code: string, message: string) {
      super(message);
      this.name = 'IpcError';
      this.code = code;
    }
  }

  const normalizeIpcError = (reason: unknown) => {
    if (reason instanceof MockIpcError) return reason;
    if (typeof reason === 'object' && reason !== null && 'code' in reason && 'message' in reason) {
      const error = reason as { code: string; message: string };
      return new MockIpcError(error.code, error.message);
    }
    return new MockIpcError('ipc_unavailable', 'Native foundation unavailable.');
  };

  return {
    IpcError: MockIpcError,
    normalizeIpcError,
    getContentRoots: vi.fn(),
    addExternalContentRoot: vi.fn(),
    removeExternalContentRoot: vi.fn(),
    setContentRootEnabled: vi.fn(),
    getLibrarySummary: vi.fn(),
    getScanStatus: vi.fn(),
    getScanIssuePage: vi.fn(),
    getSystems: vi.fn(),
    rescanLibrary: vi.fn(),
    openManagedRomFolder: vi.fn(),
    onLibraryScanProgress: vi.fn(),
    onLibraryScanCompleted: vi.fn(),
    progressHandlers: new Set<(progress: ScanProgress) => void>(),
    completedHandlers: new Set<(summary: ScanSummary) => void>(),
    pickExternalContentRoot: vi.fn(),
  };
});

vi.mock('../platform/ipc', () => ({
  IpcError: mocks.IpcError,
  normalizeIpcError: mocks.normalizeIpcError,
  getContentRoots: mocks.getContentRoots,
  addExternalContentRoot: mocks.addExternalContentRoot,
  removeExternalContentRoot: mocks.removeExternalContentRoot,
  setContentRootEnabled: mocks.setContentRootEnabled,
  getLibrarySummary: mocks.getLibrarySummary,
  getScanStatus: mocks.getScanStatus,
  getScanIssuePage: mocks.getScanIssuePage,
  getSystems: mocks.getSystems,
  rescanLibrary: mocks.rescanLibrary,
  openManagedRomFolder: mocks.openManagedRomFolder,
  onLibraryScanProgress: mocks.onLibraryScanProgress,
  onLibraryScanCompleted: mocks.onLibraryScanCompleted,
}));

vi.mock('../platform/folderPicker', () => ({
  pickExternalContentRoot: mocks.pickExternalContentRoot,
}));

const managedRoot: ContentRoot = {
  id: 1,
  path: '/documents/RetroFrontier/ROMs',
  kind: 'managed',
  enabled: true,
  systemHint: null,
  availability: 'available',
  lastScanAt: null,
  lastSuccessfulScanAt: null,
  createdAt: 1,
  updatedAt: 1,
};

const systemsResponse = {
  runtime: {
    state: 'ready' as const,
    installationId: null,
    releaseId: null,
    canRollback: false,
    repairRequired: false,
  },
  biosRoot: '/documents/RetroFrontier/BIOS',
  biosRootStatus: 'ready' as const,
  systems: [
    {
      id: 'nes' as const,
      displayName: 'Nintendo Entertainment System',
      manufacturer: 'Nintendo',
      aliases: ['NES'],
      supportedExtensions: ['.nes'],
      core: {
        policy: {
          defaultCoreId: null,
          approvedCoreIds: [],
          decision: { kind: 'unresolved' as const, researchItem: 'TBD' },
        },
        availability: {
          runtimeState: 'ready' as const,
          availableCoreIds: [],
          defaultCoreAvailable: null,
        },
      },
      bios: { policy: 'notRequired' as const, ready: true, requirements: [] },
      readiness: { ready: false, reasons: [] },
    },
  ],
};

const inactiveStatus = { running: false, progress: null, lastResult: null };

function setupDefaults() {
  mocks.progressHandlers.clear();
  mocks.completedHandlers.clear();
  for (const mock of [
    mocks.getContentRoots,
    mocks.addExternalContentRoot,
    mocks.removeExternalContentRoot,
    mocks.setContentRootEnabled,
    mocks.getLibrarySummary,
    mocks.getScanStatus,
    mocks.getScanIssuePage,
    mocks.getSystems,
    mocks.rescanLibrary,
    mocks.openManagedRomFolder,
    mocks.pickExternalContentRoot,
    mocks.onLibraryScanProgress,
    mocks.onLibraryScanCompleted,
  ]) {
    mock.mockReset();
  }
  mocks.getLibrarySummary.mockResolvedValue({ totalGames: 0, favoriteGames: 0, systems: [] });
  mocks.getContentRoots.mockResolvedValue([managedRoot]);
  mocks.getSystems.mockResolvedValue(systemsResponse);
  mocks.getScanStatus.mockResolvedValue(inactiveStatus);
  mocks.getScanIssuePage.mockResolvedValue({
    issues: [],
    scanRunId: null,
    total: 0,
    offset: 0,
    limit: 50,
  });
  mocks.addExternalContentRoot.mockResolvedValue({
    ...managedRoot,
    id: 2,
    kind: 'external',
    path: '/roms/external',
  });
  mocks.removeExternalContentRoot.mockResolvedValue(undefined);
  mocks.setContentRootEnabled.mockResolvedValue({
    ...managedRoot,
    enabled: false,
    availability: 'disabled',
  });
  mocks.openManagedRomFolder.mockResolvedValue(undefined);
  mocks.rescanLibrary.mockResolvedValue({
    runId: 1,
    state: 'completed',
    counters: {
      rootsDiscovered: 1,
      rootsCompleted: 1,
      filesDiscovered: 0,
      filesProcessed: 0,
      filesHashed: 0,
      bytesHashed: 0,
      issuesFound: 0,
    },
    durationMs: 1000,
  });
  mocks.pickExternalContentRoot.mockResolvedValue(null);
  mocks.onLibraryScanProgress.mockImplementation(
    async (handler: (progress: ScanProgress) => void) => {
      mocks.progressHandlers.add(handler);
      return () => mocks.progressHandlers.delete(handler);
    },
  );
  mocks.onLibraryScanCompleted.mockImplementation(
    async (handler: (summary: ScanSummary) => void) => {
      mocks.completedHandlers.add(handler);
      return () => mocks.completedHandlers.delete(handler);
    },
  );
}

describe('AppShell M6.2 shell and library states', () => {
  beforeEach(() => {
    setupDefaults();
    window.history.replaceState({}, '', '/library');
  });

  it('renders Library as the active destination and navigates to Settings and back', async () => {
    render(<AppShell />);

    expect(screen.getByRole('heading', { name: 'LIBRARY' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /All systems/ })).toHaveAttribute(
      'aria-current',
      'page',
    );
    await screen.findByText('/documents/RetroFrontier/ROMs');

    fireEvent.click(screen.getByRole('button', { name: /Settings/ }));
    expect(await screen.findByRole('heading', { name: 'SETTINGS' })).toBeInTheDocument();
    expect(window.location.pathname).toBe('/settings');

    act(() => window.history.back());
    expect(await screen.findByRole('heading', { name: 'LIBRARY' })).toBeInTheDocument();
    expect(window.location.pathname).toBe('/library');
  });

  it('shows an honest checking state while the system catalog is loading', async () => {
    let resolveSystems: ((response: typeof systemsResponse) => void) | undefined;
    mocks.getSystems.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveSystems = resolve;
        }),
    );
    render(<AppShell />);

    expect(screen.getByText('CHECKING SYSTEM CATALOG…')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /Nintendo Entertainment System/ }),
    ).not.toBeInTheDocument();

    await waitFor(() => expect(resolveSystems).toBeDefined());
    act(() => resolveSystems?.(systemsResponse));
    expect(
      await screen.findByRole('button', { name: /Nintendo Entertainment System/ }),
    ).toBeInTheDocument();
  });

  it('surfaces system catalog failure without fabricated rows and retries successfully', async () => {
    mocks.getSystems.mockRejectedValueOnce(
      new mocks.IpcError('catalog_invalid', 'internal catalog detail'),
    );
    mocks.getSystems.mockResolvedValueOnce(systemsResponse);
    render(<AppShell />);

    expect(await screen.findByText('SYSTEM CATALOG UNAVAILABLE')).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /Nintendo Entertainment System/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('alert')).not.toHaveTextContent('internal catalog detail');

    fireEvent.click(screen.getByRole('button', { name: 'RETRY SYSTEMS' }));
    expect(
      await screen.findByRole('button', { name: /Nintendo Entertainment System/ }),
    ).toBeInTheDocument();
    expect(mocks.getSystems).toHaveBeenCalledTimes(2);
  });

  it('uses a default accent for a system id added by the backend', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 1,
      favoriteGames: 0,
      systems: [{ systemId: 'future_console', gameCount: 1 }],
    });
    mocks.getSystems.mockResolvedValue({
      ...systemsResponse,
      systems: [
        { ...systemsResponse.systems[0], id: 'future_console', displayName: 'Future Console' },
      ],
    });
    render(<AppShell />);

    const systemRow = await screen.findByRole('button', { name: /Future Console/ });
    expect(systemRow.querySelector('.system-swatch')).toHaveStyle({
      background: 'var(--accent-3)',
    });
  });

  it('presents the managed folder, opens it, and treats a cancelled picker as normal', async () => {
    render(<AppShell />);
    await screen.findByText('/documents/RetroFrontier/ROMs');

    fireEvent.click(screen.getByRole('button', { name: /OPEN FOLDER/ }));
    await waitFor(() => expect(mocks.openManagedRomFolder).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole('button', { name: 'ADD EXTERNAL FOLDER' }));
    await waitFor(() => expect(mocks.pickExternalContentRoot).toHaveBeenCalledTimes(1));
    expect(mocks.addExternalContentRoot).not.toHaveBeenCalled();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('sends a selected folder only through the root command and refreshes state', async () => {
    mocks.pickExternalContentRoot.mockResolvedValue('/roms/external');
    render(<AppShell />);
    await screen.findByText('/documents/RetroFrontier/ROMs');

    fireEvent.click(screen.getByRole('button', { name: 'ADD EXTERNAL FOLDER' }));

    await waitFor(() => {
      expect(mocks.addExternalContentRoot).toHaveBeenCalledWith({ path: '/roms/external' });
    });
    expect(mocks.getContentRoots.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(mocks.getLibrarySummary.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it('exposes only supported external-root disable and remove operations in Settings', async () => {
    const externalRoot: ContentRoot = {
      ...managedRoot,
      id: 2,
      kind: 'external',
      path: '/roms/external',
      systemHint: 'nes',
    };
    let currentRoots = [managedRoot, externalRoot];
    mocks.getContentRoots.mockImplementation(() => Promise.resolve(currentRoots));
    mocks.removeExternalContentRoot.mockImplementation(async () => {
      currentRoots = [managedRoot];
    });
    render(<AppShell />);
    fireEvent.click(screen.getByRole('button', { name: /Settings/ }));
    expect(await screen.findByText('/roms/external')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'DISABLE ROOT' }));
    await waitFor(() => {
      expect(mocks.setContentRootEnabled).toHaveBeenCalledWith({ rootId: 2, enabled: false });
    });

    const removeTrigger = screen.getByRole('button', { name: 'REMOVE ROOT' });
    removeTrigger.focus();
    expect(removeTrigger).toHaveFocus();
    fireEvent.click(removeTrigger);
    const confirmation = await screen.findByRole('alertdialog', {
      name: /remove this root from retrofrontier/i,
    });
    expect(within(confirmation).getByRole('button', { name: 'REMOVE ROOT' })).toHaveFocus();

    fireEvent.click(within(confirmation).getByRole('button', { name: 'CANCEL' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'REMOVE ROOT' })).toHaveFocus());

    fireEvent.click(screen.getByRole('button', { name: 'REMOVE ROOT' }));
    const secondConfirmation = await screen.findByRole('alertdialog', {
      name: /remove this root from retrofrontier/i,
    });
    fireEvent.click(within(secondConfirmation).getByRole('button', { name: 'REMOVE ROOT' }));
    await waitFor(() =>
      expect(mocks.removeExternalContentRoot).toHaveBeenCalledWith({ rootId: 2 }),
    );
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: 'CONTENT ROOTS' })).toHaveFocus(),
    );
  });

  it('shows truthful progress, does not refresh the summary per progress event, and refreshes once on completion', async () => {
    render(<AppShell />);
    await waitFor(() => expect(mocks.progressHandlers.size).toBe(1));
    const summaryCallsBeforeProgress = mocks.getLibrarySummary.mock.calls.length;
    const issueCallsBeforeProgress = mocks.getScanIssuePage.mock.calls.length;

    const discovery: ScanProgress = {
      runId: 7,
      phase: 'discovery',
      counters: {
        rootsDiscovered: 1,
        rootsCompleted: 0,
        filesDiscovered: 0,
        filesProcessed: 0,
        filesHashed: 0,
        bytesHashed: 0,
        issuesFound: 0,
      },
    };
    act(() => mocks.progressHandlers.forEach((handler) => handler(discovery)));
    expect(await screen.findByText('Discovering folders and files')).toBeInTheDocument();
    expect(screen.getByText(/total progress is not known yet/i)).toBeInTheDocument();
    expect(mocks.getLibrarySummary.mock.calls.length).toBe(summaryCallsBeforeProgress);
    expect(mocks.getScanIssuePage.mock.calls.length).toBe(issueCallsBeforeProgress);

    const hashing: ScanProgress = {
      ...discovery,
      phase: 'hashing',
      counters: { ...discovery.counters, filesDiscovered: 10, filesProcessed: 4, filesHashed: 4 },
    };
    act(() => mocks.progressHandlers.forEach((handler) => handler(hashing)));
    expect(await screen.findByText(/40% reflects processed files/i)).toBeInTheDocument();

    const completed: ScanSummary = {
      runId: 7,
      state: 'completed',
      counters: { ...hashing.counters, rootsCompleted: 1 },
      durationMs: 4100,
    };
    act(() => mocks.completedHandlers.forEach((handler) => handler(completed)));
    expect(await screen.findByRole('heading', { name: 'SCAN COMPLETE' })).toBeInTheDocument();
    await waitFor(() =>
      expect(mocks.getLibrarySummary.mock.calls.length).toBe(summaryCallsBeforeProgress + 1),
    );
    await waitFor(() =>
      expect(mocks.getScanIssuePage.mock.calls.length).toBe(issueCallsBeforeProgress + 1),
    );
  });

  it('keeps the previous terminal issue page visible while a newer scan is running', async () => {
    mocks.getScanIssuePage.mockResolvedValue({
      issues: [
        {
          id: 10,
          scanRunId: 4,
          rootId: 1,
          kind: 'unreadablePath',
          relativePath: 'broken.nes',
          relatedPath: null,
          detail: 'Cannot read file',
          createdAt: 1,
        },
      ],
      scanRunId: 4,
      total: 1,
      offset: 0,
      limit: 50,
    });
    render(<AppShell />);
    expect(await screen.findByText('Path unreadable')).toBeInTheDocument();
    const issueCallsBeforeProgress = mocks.getScanIssuePage.mock.calls.length;

    act(() =>
      mocks.progressHandlers.forEach((handler) =>
        handler({
          runId: 5,
          phase: 'discovery',
          counters: {
            rootsDiscovered: 1,
            rootsCompleted: 0,
            filesDiscovered: 0,
            filesProcessed: 0,
            filesHashed: 0,
            bytesHashed: 0,
            issuesFound: 0,
          },
        }),
      ),
    );

    expect(await screen.findByText(/saved issues from terminal run #4/i)).toBeInTheDocument();
    expect(screen.getByText(/current scan #5 is still running/i)).toBeInTheDocument();
    expect(screen.getByText('Path unreadable')).toBeInTheDocument();
    expect(mocks.getScanIssuePage.mock.calls.length).toBe(issueCallsBeforeProgress);
  });

  it('loads bounded issue pages without replacing the first page', async () => {
    mocks.getScanIssuePage.mockImplementation(({ offset }: { offset: number }) =>
      Promise.resolve(
        offset === 0
          ? {
              issues: [
                {
                  id: 10,
                  scanRunId: 4,
                  rootId: 1,
                  kind: 'unreadablePath',
                  relativePath: 'one.nes',
                  relatedPath: null,
                  detail: null,
                  createdAt: 1,
                },
              ],
              scanRunId: 4,
              total: 2,
              offset: 0,
              limit: 50,
            }
          : {
              issues: [
                {
                  id: 11,
                  scanRunId: 4,
                  rootId: 1,
                  kind: 'missingReferencedFile',
                  relativePath: 'two.bin',
                  relatedPath: null,
                  detail: null,
                  createdAt: 2,
                },
              ],
              scanRunId: 4,
              total: 2,
              offset: 1,
              limit: 50,
            },
      ),
    );
    render(<AppShell />);
    expect(await screen.findByText('Path unreadable')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /LOAD MORE ISSUES/ }));
    expect(await screen.findByText('Missing referenced file')).toBeInTheDocument();
    expect(mocks.getScanIssuePage).toHaveBeenCalledWith({ offset: 1, limit: 50 });
  });

  it('clears load-more state when completion supersedes a stale issue page request', async () => {
    let resolveLoadMore: ((page: unknown) => void) | undefined;
    const firstPage = {
      issues: [
        {
          id: 10,
          scanRunId: 4,
          rootId: 1,
          kind: 'unreadablePath',
          relativePath: 'first.nes',
          relatedPath: null,
          detail: 'first page',
          createdAt: 1,
        },
      ],
      scanRunId: 4,
      total: 2,
      offset: 0,
      limit: 50,
    };
    const refreshedPage = {
      ...firstPage,
      issues: [
        {
          ...firstPage.issues[0],
          id: 20,
          relativePath: 'refreshed.nes',
          detail: 'refreshed page',
        },
      ],
    };
    let issuePageCall = 0;
    mocks.getScanIssuePage.mockImplementation(({ offset }: { offset: number }) => {
      issuePageCall += 1;
      if (offset === 1) {
        return new Promise((resolve) => {
          resolveLoadMore = resolve;
        });
      }
      return Promise.resolve(issuePageCall === 1 ? firstPage : refreshedPage);
    });

    render(<AppShell />);
    expect(await screen.findByText('first page')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /LOAD MORE ISSUES/ }));
    await waitFor(() => expect(resolveLoadMore).toBeDefined());

    const completion: ScanSummary = {
      runId: 7,
      state: 'completed',
      counters: {
        rootsDiscovered: 1,
        rootsCompleted: 1,
        filesDiscovered: 1,
        filesProcessed: 1,
        filesHashed: 1,
        bytesHashed: 12,
        issuesFound: 1,
      },
      durationMs: 1000,
    };
    act(() => mocks.completedHandlers.forEach((handler) => handler(completion)));
    expect(await screen.findByText('refreshed page')).toBeInTheDocument();

    await act(async () => {
      resolveLoadMore?.({
        ...firstPage,
        issues: [
          {
            ...firstPage.issues[0],
            id: 11,
            relativePath: 'stale.nes',
            detail: 'stale page',
          },
        ],
        offset: 1,
      });
    });

    expect(screen.queryByText('stale page')).not.toBeInTheDocument();
    const loadMore = screen.getByRole('button', { name: /LOAD MORE ISSUES/ });
    expect(loadMore).not.toBeDisabled();
  });

  it('clears refresh loading when a later issue request supersedes it', async () => {
    let resolveRefresh: ((page: unknown) => void) | undefined;
    let issuePageCall = 0;
    const firstPage = {
      issues: [
        {
          id: 10,
          scanRunId: 4,
          rootId: 1,
          kind: 'unreadablePath',
          relativePath: 'first.nes',
          relatedPath: null,
          detail: 'first page',
          createdAt: 1,
        },
      ],
      scanRunId: 4,
      total: 2,
      offset: 0,
      limit: 50,
    };
    const secondPage = {
      ...firstPage,
      issues: [
        {
          ...firstPage.issues[0],
          id: 11,
          relativePath: 'second.nes',
          detail: 'second page',
        },
      ],
      offset: 1,
    };
    mocks.getScanIssuePage.mockImplementation(({ offset }: { offset: number }) => {
      issuePageCall += 1;
      if (offset === 0 && issuePageCall === 2) {
        return new Promise((resolve) => {
          resolveRefresh = resolve;
        });
      }
      return Promise.resolve(offset === 0 ? firstPage : secondPage);
    });

    render(<AppShell />);
    expect(await screen.findByText('first page')).toBeInTheDocument();

    act(() =>
      mocks.completedHandlers.forEach((handler) =>
        handler({
          runId: 8,
          state: 'completed',
          counters: {
            rootsDiscovered: 1,
            rootsCompleted: 1,
            filesDiscovered: 1,
            filesProcessed: 1,
            filesHashed: 1,
            bytesHashed: 0,
            issuesFound: 1,
          },
          durationMs: 1000,
        }),
      ),
    );
    await waitFor(() => expect(resolveRefresh).toBeDefined());
    expect(screen.getByText('Refreshing saved issues…')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /LOAD MORE ISSUES/ }));
    expect(await screen.findByText('second page')).toBeInTheDocument();
    expect(screen.getByText('Refreshing saved issues…')).toBeInTheDocument();

    await act(async () => {
      resolveRefresh?.(firstPage);
    });
    await waitFor(() =>
      expect(screen.queryByText('Refreshing saved issues…')).not.toBeInTheDocument(),
    );
  });

  it('keeps refresh loading owned by the newest overlapping issue request', async () => {
    let resolveOlderRefresh: ((page: unknown) => void) | undefined;
    let resolveNewerRefresh: ((page: unknown) => void) | undefined;
    let issuePageCall = 0;
    const page = {
      issues: [
        {
          id: 30,
          scanRunId: 9,
          rootId: 1,
          kind: 'unreadablePath',
          relativePath: 'newest.nes',
          relatedPath: null,
          detail: 'newest issue page',
          createdAt: 1,
        },
      ],
      scanRunId: 9,
      total: 1,
      offset: 0,
      limit: 50,
    };
    mocks.getScanIssuePage.mockImplementation(() => {
      issuePageCall += 1;
      return new Promise((resolve) => {
        if (issuePageCall === 1) resolveOlderRefresh = resolve;
        else resolveNewerRefresh = resolve;
      });
    });

    render(<AppShell />);
    await waitFor(() => expect(resolveOlderRefresh).toBeDefined());

    act(() =>
      mocks.completedHandlers.forEach((handler) =>
        handler({
          runId: 9,
          state: 'completed',
          counters: {
            rootsDiscovered: 1,
            rootsCompleted: 1,
            filesDiscovered: 1,
            filesProcessed: 1,
            filesHashed: 1,
            bytesHashed: 0,
            issuesFound: 1,
          },
          durationMs: 1000,
        }),
      ),
    );
    await waitFor(() => expect(resolveNewerRefresh).toBeDefined());
    expect(screen.getByText(/reading saved issues/i)).toBeInTheDocument();

    await act(async () => {
      resolveOlderRefresh?.({ ...page, issues: [] });
    });

    expect(screen.getByText(/reading saved issues/i)).toBeInTheDocument();

    await act(async () => {
      resolveNewerRefresh?.(page);
    });
    expect(await screen.findByText('newest issue page')).toBeInTheDocument();
    expect(screen.queryByText(/reading saved issues/i)).not.toBeInTheDocument();
  });

  it('keeps the shell available when the bounded issue page fails', async () => {
    mocks.getScanIssuePage.mockRejectedValue(
      new mocks.IpcError('library_unavailable', 'internal issue-store detail'),
    );
    render(<AppShell />);

    expect(await screen.findByText('SCAN ISSUES UNAVAILABLE')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'LIBRARY' })).toBeInTheDocument();
    expect(screen.getByRole('alert')).not.toHaveTextContent('internal issue-store detail');
  });

  it('keeps an event update that arrives while the initial status query is pending', async () => {
    let resolveStatus: ((status: typeof inactiveStatus) => void) | undefined;
    mocks.getScanStatus.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveStatus = resolve;
        }),
    );
    render(<AppShell />);
    await waitFor(() => {
      expect(mocks.getScanStatus).toHaveBeenCalled();
      expect(mocks.progressHandlers.size).toBe(1);
    });

    const progress: ScanProgress = {
      runId: 9,
      phase: 'hashing',
      counters: {
        rootsDiscovered: 1,
        rootsCompleted: 1,
        filesDiscovered: 3,
        filesProcessed: 1,
        filesHashed: 1,
        bytesHashed: 12,
        issuesFound: 0,
      },
    };
    act(() => mocks.progressHandlers.forEach((handler) => handler(progress)));
    resolveStatus?.(inactiveStatus);

    expect(await screen.findByRole('heading', { name: /SCAN IN PROGRESS/ })).toBeInTheDocument();
    expect(screen.getByText(/33% reflects processed files/i)).toBeInTheDocument();
  });

  it('does not let progress after completion resurrect the terminal scan state', async () => {
    render(<AppShell />);
    await waitFor(() => expect(mocks.progressHandlers.size).toBe(1));

    const progress: ScanProgress = {
      runId: 12,
      phase: 'hashing',
      counters: {
        rootsDiscovered: 1,
        rootsCompleted: 1,
        filesDiscovered: 2,
        filesProcessed: 1,
        filesHashed: 1,
        bytesHashed: 12,
        issuesFound: 0,
      },
    };
    const completion: ScanSummary = {
      runId: 12,
      state: 'completed',
      counters: progress.counters,
      durationMs: 1200,
    };

    act(() => mocks.progressHandlers.forEach((handler) => handler(progress)));
    expect(await screen.findByRole('heading', { name: 'SCAN IN PROGRESS' })).toBeInTheDocument();
    act(() => mocks.completedHandlers.forEach((handler) => handler(completion)));
    expect(await screen.findByRole('heading', { name: 'SCAN COMPLETE' })).toBeInTheDocument();

    await waitFor(() => expect(mocks.getLibrarySummary).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(mocks.getScanIssuePage).toHaveBeenCalledTimes(2));
    const summaryCallsAfterCompletion = mocks.getLibrarySummary.mock.calls.length;
    const issueCallsAfterCompletion = mocks.getScanIssuePage.mock.calls.length;

    act(() => mocks.progressHandlers.forEach((handler) => handler(progress)));
    expect(screen.getByRole('heading', { name: 'SCAN COMPLETE' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'SCAN IN PROGRESS' })).not.toBeInTheDocument();
    expect(mocks.getLibrarySummary).toHaveBeenCalledTimes(summaryCallsAfterCompletion);
    expect(mocks.getScanIssuePage).toHaveBeenCalledTimes(issueCallsAfterCompletion);
  });

  it('keeps scan counters outside the polite phase live region', async () => {
    render(<AppShell />);
    await waitFor(() => expect(mocks.progressHandlers.size).toBe(1));

    const progress = (filesProcessed: number): ScanProgress => ({
      runId: 13,
      phase: 'hashing',
      counters: {
        rootsDiscovered: 1,
        rootsCompleted: 1,
        filesDiscovered: 4,
        filesProcessed,
        filesHashed: filesProcessed,
        bytesHashed: filesProcessed * 10,
        issuesFound: 0,
      },
    });

    act(() => mocks.progressHandlers.forEach((handler) => handler(progress(1))));
    expect(await screen.findByText('1 PROCESSED')).toBeInTheDocument();
    const liveRegion = document.querySelector('.scan-phase-row [role="status"]');
    expect(liveRegion).toHaveTextContent('Hashing content');
    expect(liveRegion).not.toHaveTextContent('PROCESSED');
    expect(liveRegion).not.toHaveAttribute('aria-atomic');
    expect(screen.getByText('1 PROCESSED')).toHaveAttribute('aria-hidden', 'true');

    const announcedText = liveRegion?.textContent;
    act(() => mocks.progressHandlers.forEach((handler) => handler(progress(2))));
    expect(await screen.findByText('2 PROCESSED')).toBeInTheDocument();
    expect(liveRegion?.textContent).toBe(announcedText);
  });

  it('renders a restrained populated state without querying or fabricating game cards', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 3,
      favoriteGames: 1,
      systems: [
        { systemId: 'nes', gameCount: 2 },
        { systemId: 'snes', gameCount: 1 },
      ],
    });
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'LIBRARY READY' })).toBeInTheDocument();
    expect(screen.getAllByText('3').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Nintendo Entertainment System').length).toBeGreaterThanOrEqual(1);
    expect(screen.queryByText('Chrono Trigger')).not.toBeInTheDocument();
  });

  it('cleans up both scan event listeners on unmount', async () => {
    const { unmount } = render(<AppShell />);
    await waitFor(() => {
      expect(mocks.progressHandlers.size).toBe(1);
      expect(mocks.completedHandlers.size).toBe(1);
    });

    unmount();
    expect(mocks.progressHandlers.size).toBe(0);
    expect(mocks.completedHandlers.size).toBe(0);
  });

  it('cleans up listener registrations that resolve after an effect teardown', async () => {
    type UnlistenMock = ReturnType<typeof vi.fn<() => void>>;
    const progressRegistrations: Array<{
      resolve: (unlisten: () => void) => void;
      unlisten: UnlistenMock;
    }> = [];
    const completedRegistrations: Array<{
      resolve: (unlisten: () => void) => void;
      unlisten: UnlistenMock;
    }> = [];
    mocks.onLibraryScanProgress.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          const unlisten = vi.fn<() => void>();
          progressRegistrations.push({ resolve, unlisten });
        }),
    );
    mocks.onLibraryScanCompleted.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          const unlisten = vi.fn<() => void>();
          completedRegistrations.push({ resolve, unlisten });
        }),
    );

    const { unmount } = render(
      <StrictMode>
        <AppShell />
      </StrictMode>,
    );
    await waitFor(() => {
      expect(progressRegistrations).toHaveLength(2);
      expect(completedRegistrations).toHaveLength(2);
    });

    act(() => {
      progressRegistrations[0].resolve(progressRegistrations[0].unlisten);
      completedRegistrations[0].resolve(completedRegistrations[0].unlisten);
    });
    await waitFor(() => {
      expect(progressRegistrations[0].unlisten).toHaveBeenCalledTimes(1);
      expect(completedRegistrations[0].unlisten).toHaveBeenCalledTimes(1);
    });
    expect(progressRegistrations[1].unlisten).not.toHaveBeenCalled();
    expect(completedRegistrations[1].unlisten).not.toHaveBeenCalled();

    act(() => {
      progressRegistrations[1].resolve(progressRegistrations[1].unlisten);
      completedRegistrations[1].resolve(completedRegistrations[1].unlisten);
    });
    await waitFor(() => expect(mocks.getScanStatus).toHaveBeenCalled());

    unmount();
    expect(progressRegistrations[1].unlisten).toHaveBeenCalledTimes(1);
    expect(completedRegistrations[1].unlisten).toHaveBeenCalledTimes(1);
  });
});
