import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { StrictMode } from 'react';

import type {
  ContentRoot,
  LaunchResponse,
  LaunchState,
  GameMetadataState,
  LibraryGameDetail,
  LibraryPage,
  MetadataStateChanged,
  ScanProgress,
  ScanSummary,
} from '../platform/ipc';
import { GAMEPAD_BUTTON_INDEX } from '../input/gamepadAdapter';
import { installRectStub, layoutColumn, layoutGrid } from '../test/geometry';
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
    queryLibrary: vi.fn(),
    getLibraryGameDetail: vi.fn(),
    getGameMetadata: vi.fn(),
    setGameFavorite: vi.fn(),
    getMetadataProviderStatus: vi.fn(),
    getMetadataProviderAccount: vi.fn(),
    setMetadataProviderCredentials: vi.fn(),
    clearMetadataProviderCredentials: vi.fn(),
    getScanStatus: vi.fn(),
    getScanIssuePage: vi.fn(),
    getSystems: vi.fn(),
    rescanLibrary: vi.fn(),
    openManagedRomFolder: vi.fn(),
    onLibraryScanProgress: vi.fn(),
    onLibraryScanCompleted: vi.fn(),
    getLaunchState: vi.fn(),
    launchGame: vi.fn(),
    onGameLaunchStateChanged: vi.fn(),
    onMetadataStateChanged: vi.fn(),
    progressHandlers: new Set<(progress: ScanProgress) => void>(),
    completedHandlers: new Set<(summary: ScanSummary) => void>(),
    metadataHandlers: new Set<(event: MetadataStateChanged) => void>(),
    pickExternalContentRoot: vi.fn(),
    isAppWindowFocused: vi.fn(),
    onAppWindowFocusChanged: vi.fn(),
    requestAppWindowFocus: vi.fn(),
    isDesktopRuntime: vi.fn(),
    windowFocusHandlers: new Set<(focused: boolean) => void>(),
    launchHandlers: new Set<(event: { state: LaunchState }) => void>(),
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
  queryLibrary: mocks.queryLibrary,
  getLibraryGameDetail: mocks.getLibraryGameDetail,
  getGameMetadata: mocks.getGameMetadata,
  setGameFavorite: mocks.setGameFavorite,
  getMetadataProviderStatus: mocks.getMetadataProviderStatus,
  getMetadataProviderAccount: mocks.getMetadataProviderAccount,
  setMetadataProviderCredentials: mocks.setMetadataProviderCredentials,
  clearMetadataProviderCredentials: mocks.clearMetadataProviderCredentials,
  getScanStatus: mocks.getScanStatus,
  getScanIssuePage: mocks.getScanIssuePage,
  getSystems: mocks.getSystems,
  rescanLibrary: mocks.rescanLibrary,
  openManagedRomFolder: mocks.openManagedRomFolder,
  onLibraryScanProgress: mocks.onLibraryScanProgress,
  onLibraryScanCompleted: mocks.onLibraryScanCompleted,
  onMetadataStateChanged: mocks.onMetadataStateChanged,
  getLaunchState: mocks.getLaunchState,
  launchGame: mocks.launchGame,
  onGameLaunchStateChanged: mocks.onGameLaunchStateChanged,
}));

vi.mock('../platform/folderPicker', () => ({
  pickExternalContentRoot: mocks.pickExternalContentRoot,
}));

vi.mock('../platform/appWindow', () => ({
  isAppWindowFocused: mocks.isAppWindowFocused,
  onAppWindowFocusChanged: mocks.onAppWindowFocusChanged,
  requestAppWindowFocus: mocks.requestAppWindowFocus,
  isDesktopRuntime: mocks.isDesktopRuntime,
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

const metadataProviderStatus = {
  providerId: 'screenScraper' as const,
  credentialsConfigured: true,
  userAccount: 'notConfigured' as const,
  userAccountName: null,
  quota: {
    maxThreads: 1,
    maxRequestsPerMinute: 60,
    maxRequestsPerDay: 1000,
    maxNegativeRequestsPerDay: 100,
    requestsToday: 0,
    negativeRequestsToday: 0,
  },
  quotaObservedAt: null,
  deferredUntil: null,
  deferReason: null,
  offline: false,
  pendingJobs: 0,
  deferredJobs: 0,
  failedJobs: 0,
};

const metadataProviderAccount = {
  configured: false,
  state: 'notConfigured' as const,
  username: null,
};

const populatedLibraryPage: LibraryPage = {
  items: [
    {
      gameId: 1,
      systemId: 'nes',
      localTitle: 'Kirby Local',
      metadataTitle: 'Kirby’s Adventure',
      displayTitle: 'Kirby’s Adventure',
      sortTitle: 'kirby’s adventure',
      availability: 'available',
      favorite: false,
      metadataMatchState: 'matched',
      releaseDate: '1993-03-23',
      genre: 'Platform',
      region: 'US',
      coverRef: null,
    },
    {
      gameId: 2,
      systemId: 'nes',
      localTitle: 'A Very Long Local Title Without Metadata',
      metadataTitle: null,
      displayTitle: 'A Very Long Local Title Without Metadata',
      sortTitle: 'a very long local title without metadata',
      availability: 'unavailable',
      favorite: true,
      metadataMatchState: 'failed',
      releaseDate: null,
      genre: null,
      region: null,
      coverRef: 'rfmedia://localhost/cover/2',
    },
  ],
  total: 2,
  offset: 0,
  limit: 60,
};

const populatedGameDetail: LibraryGameDetail = {
  gameId: 1,
  systemId: 'nes',
  localTitle: 'Kirby Local',
  availability: 'available',
  favorite: false,
  contentUnits: [
    {
      unitId: 101,
      rootId: 1,
      kind: 'singleFile',
      localTitle: 'Kirby Local',
      primaryRelativePath: 'NES/Kirby.nes',
      fileCount: 1,
      availability: 'available',
    },
  ],
};

const populatedGameMetadata: GameMetadataState = {
  gameId: 1,
  providerId: 'screenScraper',
  status: 'matched',
  matchType: 'deterministicSha1',
  deterministic: true,
  providerGameId: 'hidden-provider-id',
  providerRomId: 'hidden-provider-rom-id',
  unsupportedReason: null,
  lastFailure: null,
  lastCheckedAt: 1,
  metadata: {
    metadata: {
      title: 'Kirby’s Adventure',
      sortTitle: 'kirby’s adventure',
      synopsis: 'A platform adventure.',
      releaseDate: '1993-03-23',
      developer: 'HAL Laboratory',
      publisher: 'Nintendo',
      genre: 'Platform',
      players: '1',
      region: 'US',
    },
    provenance: {
      providerId: 'screenScraper',
      providerGameId: 'hidden-provider-id',
      sourceCredit: 'ScreenScraper',
      fetchedAt: 1,
    },
  },
  cover: null,
  candidates: [],
  userSelection: null,
  jobs: [],
};

function setupDefaults() {
  mocks.progressHandlers.clear();
  mocks.completedHandlers.clear();
  mocks.metadataHandlers.clear();
  for (const mock of [
    mocks.getContentRoots,
    mocks.addExternalContentRoot,
    mocks.removeExternalContentRoot,
    mocks.setContentRootEnabled,
    mocks.getLibrarySummary,
    mocks.queryLibrary,
    mocks.getLibraryGameDetail,
    mocks.getGameMetadata,
    mocks.setGameFavorite,
    mocks.getMetadataProviderStatus,
    mocks.getMetadataProviderAccount,
    mocks.setMetadataProviderCredentials,
    mocks.clearMetadataProviderCredentials,
    mocks.getScanStatus,
    mocks.getScanIssuePage,
    mocks.getSystems,
    mocks.rescanLibrary,
    mocks.openManagedRomFolder,
    mocks.pickExternalContentRoot,
    mocks.onLibraryScanProgress,
    mocks.onLibraryScanCompleted,
    mocks.onMetadataStateChanged,
    mocks.getLaunchState,
    mocks.launchGame,
    mocks.onGameLaunchStateChanged,
    mocks.isAppWindowFocused,
    mocks.onAppWindowFocusChanged,
    mocks.requestAppWindowFocus,
    mocks.isDesktopRuntime,
  ]) {
    mock.mockReset();
  }
  mocks.windowFocusHandlers.clear();
  mocks.launchHandlers.clear();
  // The shell is exercised on the real desktop ownership path, where unknown native window focus
  // fails closed; a plain browser session is covered by `useAppWindowFocus.test.tsx`.
  mocks.isDesktopRuntime.mockReturnValue(true);
  mocks.isAppWindowFocused.mockResolvedValue(true);
  mocks.requestAppWindowFocus.mockResolvedValue(true);
  mocks.onAppWindowFocusChanged.mockImplementation(async (handler: (focused: boolean) => void) => {
    mocks.windowFocusHandlers.add(handler);
    return () => mocks.windowFocusHandlers.delete(handler);
  });
  mocks.getLaunchState.mockResolvedValue({ running: null, blocked: false });
  mocks.onGameLaunchStateChanged.mockImplementation(
    async (handler: (event: { state: LaunchState }) => void) => {
      mocks.launchHandlers.add(handler);
      return () => mocks.launchHandlers.delete(handler);
    },
  );
  mocks.getLibrarySummary.mockResolvedValue({ totalGames: 0, favoriteGames: 0, systems: [] });
  mocks.queryLibrary.mockResolvedValue({ items: [], total: 0, offset: 0, limit: 60 });
  mocks.getLibraryGameDetail.mockResolvedValue(populatedGameDetail);
  mocks.getGameMetadata.mockResolvedValue(populatedGameMetadata);
  mocks.setGameFavorite.mockResolvedValue({ gameId: 1, favorite: true });
  mocks.getMetadataProviderStatus.mockResolvedValue(metadataProviderStatus);
  mocks.getMetadataProviderAccount.mockResolvedValue(metadataProviderAccount);
  mocks.setMetadataProviderCredentials.mockResolvedValue(undefined);
  mocks.clearMetadataProviderCredentials.mockResolvedValue(undefined);
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
  mocks.onMetadataStateChanged.mockImplementation(
    async (handler: (event: MetadataStateChanged) => void) => {
      mocks.metadataHandlers.add(handler);
      return () => mocks.metadataHandlers.delete(handler);
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
      'aria-pressed',
      'true',
    );
    await screen.findByText('/documents/RetroFrontier/ROMs');

    fireEvent.click(screen.getByRole('button', { name: /Settings/ }));
    expect(await screen.findByRole('heading', { name: 'SETTINGS' })).toBeInTheDocument();
    expect(window.location.pathname).toBe('/settings');

    act(() => window.history.back());
    await waitFor(() => expect(window.location.pathname).toBe('/library'));
    expect(await screen.findByRole('heading', { level: 1, name: 'LIBRARY' })).toBeInTheDocument();
  });

  it('keeps the visible sidebar prefixes decorative in accessible group names', async () => {
    render(<AppShell />);

    expect(await screen.findByRole('region', { name: 'SYSTEMS' })).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: 'MENU' })).toBeInTheDocument();
    expect(screen.getAllByText('//', { selector: '.sidebar-prefix' })).toHaveLength(2);
  });

  it('keeps the shared sidebar on Library, Settings, and Game Detail without adding Settings search', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);

    expect(
      await screen.findByRole('complementary', { name: 'Library navigation' }),
    ).toBeInTheDocument();
    expect(screen.getByText('LOCAL LIBRARY')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Settings/ }));
    expect(await screen.findByRole('heading', { level: 1, name: 'SETTINGS' })).toBeInTheDocument();
    const settingsSidebar = screen.getByRole('complementary', { name: 'Library navigation' });
    expect(settingsSidebar).toBeInTheDocument();
    expect(within(settingsSidebar).getByRole('button', { name: 'Settings' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(
      within(settingsSidebar).getByRole('button', { name: /Nintendo Entertainment System/ }),
    ).toHaveTextContent('2');
    const mobileNavigation = screen.getByRole('navigation', { name: 'Primary navigation' });
    expect(within(mobileNavigation).getByRole('button', { name: 'SETTINGS' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.queryByRole('searchbox', { name: 'Search' })).not.toBeInTheDocument();
    expect(screen.getByText('LOCAL LIBRARY')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Go to Library' })).toBeInTheDocument();
    expect(screen.getByRole('group', { name: 'Theme' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'LIBRARY' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'METADATA' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'BACK TO LIBRARY' }));
    await screen.findByRole('heading', { name: 'LIBRARY' });
    fireEvent.click(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }));
    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });
    expect(screen.getByRole('complementary', { name: 'Library navigation' })).toBeInTheDocument();
    expect(screen.getByText('LOCAL LIBRARY')).toBeInTheDocument();
  });

  it('returns to the underlying Settings route when a running scan reaches a terminal state', async () => {
    mocks.getScanStatus.mockResolvedValue({
      running: true,
      progress: {
        runId: 16,
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
      },
      lastResult: null,
    });
    window.history.replaceState({}, '', '/settings');
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'SCAN IN PROGRESS' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'SETTINGS' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('complementary', { name: 'Library navigation' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('searchbox', { name: 'Search' })).not.toBeInTheDocument();

    act(() =>
      mocks.completedHandlers.forEach((handler) =>
        handler({
          runId: 16,
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
          durationMs: 100,
        }),
      ),
    );

    expect(await screen.findByRole('heading', { name: 'SETTINGS' })).toBeInTheDocument();
    expect(window.location.pathname).toBe('/settings');
  });

  it('lets Settings system rows return to the filtered Library', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);

    fireEvent.click(await screen.findByRole('button', { name: /Settings/ }));
    const settingsSidebar = await screen.findByRole('complementary', {
      name: 'Library navigation',
    });
    fireEvent.click(
      within(settingsSidebar).getByRole('button', { name: /Nintendo Entertainment System/ }),
    );

    expect(await screen.findByRole('heading', { level: 1, name: 'LIBRARY' })).toBeInTheDocument();
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
      systemId: 'nes',
      sort: 'titleAsc',
      offset: 0,
    });
  });

  it('opens a game detail from a semantic card link and returns without a full-library detail query', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    fireEvent.click(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }));

    expect(window.location.pathname).toBe('/games/1');
    const detailHeading = await screen.findByRole('heading', {
      level: 1,
      name: 'Kirby’s Adventure',
    });
    expect(detailHeading).toBeInTheDocument();
    expect(document.activeElement).toBe(detailHeading);
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledWith({ gameId: 1 });
    expect(mocks.getGameMetadata).toHaveBeenCalledWith({ gameId: 1 });
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(1);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(1);
    // M7 adds the Play action to Game Detail; the shell still loads exactly one bounded detail
    // and one metadata read for the opened game.
    expect(screen.getByRole('button', { name: /^play /i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('link', { name: /back to library/i }));
    const returnedCardHeading = await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    expect(returnedCardHeading).toBeInTheDocument();
    expect(window.location.pathname).toBe('/library');
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }),
      ),
    );
  });

  it('loads a valid deep link and gives invalid game routes a safe Library path', async () => {
    window.history.replaceState({}, '', '/games/1');
    render(<AppShell />);

    expect(
      await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' }),
    ).toBeInTheDocument();
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledWith({ gameId: 1 });

    window.history.replaceState({}, '', '/games/not-an-id');
    act(() => window.dispatchEvent(new PopStateEvent('popstate')));
    expect(
      await screen.findByRole('heading', { level: 1, name: 'INVALID GAME LINK' }),
    ).toBeInTheDocument();
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(1);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('link', { name: /back to library/i })).toHaveAttribute(
      'href',
      '/library',
    );
  });

  it('preserves the current library page and restores card focus after browser Back', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 121,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 121 }],
    });
    mocks.queryLibrary
      .mockResolvedValueOnce({ ...populatedLibraryPage, total: 121, offset: 0 })
      .mockResolvedValue({ ...populatedLibraryPage, total: 121, offset: 60 });
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    fireEvent.click(screen.getByRole('button', { name: 'NEXT PAGE' }));
    await screen.findByText('PAGE 2 OF 3');
    fireEvent.click(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }));
    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });

    act(() => window.history.back());
    await screen.findByText('PAGE 2 OF 3');
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({ sort: 'titleAsc', offset: 60 });
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }),
      ),
    );
  });

  it('does not restore a card after leaving detail through primary navigation', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    fireEvent.click(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }));
    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'Go to Library' }));
    expect(await screen.findByRole('heading', { name: 'LIBRARY' })).toBeInTheDocument();
    await waitFor(() =>
      expect(document.activeElement).not.toBe(
        screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }),
      ),
    );
  });

  it('returns focus to the Library heading when the originating card is no longer present', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValueOnce(populatedLibraryPage).mockResolvedValueOnce({
      ...populatedLibraryPage,
      items: [populatedLibraryPage.items[1]],
      total: 1,
    });
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    fireEvent.click(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }));
    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });
    fireEvent.click(screen.getByRole('link', { name: /back to library/i }));

    await screen.findByRole('heading', { name: 'A Very Long Local Title Without Metadata' });
    await waitFor(() => expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus());
  });

  it('does not mark either primary destination current on a game detail route', async () => {
    window.history.replaceState({}, '', '/games/1');
    render(<AppShell />);

    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });
    const primaryNavigation = screen.getByRole('navigation', { name: 'Primary navigation' });
    expect(within(primaryNavigation).getByRole('button', { name: 'LIBRARY' })).not.toHaveAttribute(
      'aria-current',
    );
    expect(within(primaryNavigation).getByRole('button', { name: 'SETTINGS' })).not.toHaveAttribute(
      'aria-current',
    );
  });

  it('does not refresh detail on scan progress and refreshes bounded detail once on completion', async () => {
    window.history.replaceState({}, '', '/games/1');
    render(<AppShell />);
    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });
    const localCalls = mocks.getLibraryGameDetail.mock.calls.length;
    const metadataCalls = mocks.getGameMetadata.mock.calls.length;

    act(() =>
      mocks.progressHandlers.forEach((handler) =>
        handler({
          runId: 41,
          phase: 'hashing',
          counters: {
            rootsDiscovered: 1,
            rootsCompleted: 0,
            filesDiscovered: 2,
            filesProcessed: 1,
            filesHashed: 1,
            bytesHashed: 10,
            issuesFound: 0,
          },
        }),
      ),
    );
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(localCalls);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(metadataCalls);

    const completion: ScanSummary = {
      runId: 41,
      state: 'completed',
      counters: {
        rootsDiscovered: 1,
        rootsCompleted: 1,
        filesDiscovered: 2,
        filesProcessed: 2,
        filesHashed: 2,
        bytesHashed: 20,
        issuesFound: 0,
      },
      durationMs: 1000,
    };
    act(() => mocks.completedHandlers.forEach((handler) => handler(completion)));
    await waitFor(() => {
      expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(localCalls + 1);
      expect(mocks.getGameMetadata).toHaveBeenCalledTimes(metadataCalls + 1);
    });
    act(() => mocks.completedHandlers.forEach((handler) => handler(completion)));
    expect(mocks.getLibraryGameDetail).toHaveBeenCalledTimes(localCalls + 1);
    expect(mocks.getGameMetadata).toHaveBeenCalledTimes(metadataCalls + 1);
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
    await waitFor(() => expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus());
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

  it('removes previous terminal issue content while a newer scan is running', async () => {
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

    expect(await screen.findByRole('heading', { name: 'SCAN IN PROGRESS' })).toBeInTheDocument();
    expect(screen.queryByText(/saved issues from terminal run #4/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/current scan #5 is still running/i)).not.toBeInTheDocument();
    expect(screen.queryByText('Path unreadable')).not.toBeInTheDocument();
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

    act(() =>
      mocks.progressHandlers.forEach((handler) =>
        handler({ ...progress, runId: 11, counters: { ...progress.counters, filesProcessed: 2 } }),
      ),
    );
    expect(screen.getByRole('heading', { name: 'SCAN COMPLETE' })).toBeInTheDocument();
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

  it('renders the real bounded populated library instead of the transitional state', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(
      screen.getByRole('heading', { name: 'A Very Long Local Title Without Metadata' }),
    ).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'LIBRARY READY' })).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'LIBRARY IS EMPTY' })).not.toBeInTheDocument();
    const search = screen.getByRole('searchbox', { name: 'Search' });
    expect(search).toHaveAttribute('placeholder', 'Search');
    expect(search.closest('search')?.querySelector('svg')).toBeNull();
    expect(screen.queryByText('SEARCH LIBRARY')).not.toBeInTheDocument();
    expect(screen.getByText('// FILTER')).toBeVisible();
    expect(screen.getAllByText('Nintendo Entertainment System').length).toBeGreaterThanOrEqual(1);
    expect(mocks.queryLibrary).toHaveBeenCalledWith({ sort: 'titleAsc', offset: 0 });
  });

  // M6.7B: the populated grid must read as games, not as database records. The tiles carry the
  // compact B1 hierarchy only; local-availability and metadata lifecycle prose no longer occupy a
  // permanent text row on every card.
  it('renders compact B1 game tiles without technical status rows in the real grid', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby\u2019s Adventure' });

    // Compact system badge, not the full catalog display name, inside the tile.
    expect(screen.getAllByText('NES')).toHaveLength(2);
    expect(screen.getByText('1993')).toBeInTheDocument();

    // No permanent availability or metadata rows on the tiles.
    expect(screen.queryByText('LOCAL')).not.toBeInTheDocument();
    expect(screen.queryByText('LOCAL FILE MISSING')).not.toBeInTheDocument();
    expect(
      screen.queryByText(/METADATA|MATCH REVIEW/, { ignore: '.visually-hidden' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText('Platform \u00b7 US')).not.toBeInTheDocument();

    // The missing local file stays visible, actionable, and browsable.
    const missing = screen.getByText('MISSING');
    expect(missing).toBeVisible();
    expect(
      screen.getByRole('link', { name: 'Open A Very Long Local Title Without Metadata details' }),
    ).toHaveAttribute('href', '/games/2');
  });

  // Regression for the sidebar flicker: a terminal ROM scan must not clear and refetch the system
  // catalog, because `useSystemCatalog.refresh()` blanks the sidebar and readiness panel first and
  // a ROM scan cannot change runtime, core, or BIOS state.
  it('keeps the system sidebar populated across a terminal scan completion', async () => {
    render(<AppShell />);

    await screen.findByRole('button', { name: /Nintendo Entertainment System/ });
    expect(mocks.getSystems).toHaveBeenCalledTimes(1);

    await act(async () => {
      mocks.completedHandlers.forEach((handler) =>
        handler({
          runId: 9,
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
          durationMs: 5,
        }),
      );
    });

    expect(
      screen.getByRole('button', { name: /Nintendo Entertainment System/ }),
    ).toBeInTheDocument();
    expect(mocks.getSystems).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('CHECKING SYSTEM CATALOG…')).not.toBeInTheDocument();
  });

  // Regression for the runtime pass: a first discovered game must leave the empty-library
  // onboarding state without an application restart, and that transition must not itself request
  // another scan.
  it('leaves the empty library for a populated one when a terminal scan discovers a game', async () => {
    mocks.getLibrarySummary
      .mockResolvedValueOnce({ totalGames: 0, favoriteGames: 0, systems: [] })
      .mockResolvedValue({
        totalGames: 1,
        favoriteGames: 0,
        systems: [{ systemId: 'nes', gameCount: 1 }],
      });
    mocks.queryLibrary.mockResolvedValue({
      ...populatedLibraryPage,
      items: [populatedLibraryPage.items[0]],
      total: 1,
    });
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'LIBRARY IS EMPTY' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /All systems/ })).toHaveTextContent('0');
    expect(mocks.queryLibrary).not.toHaveBeenCalled();

    await act(async () => {
      mocks.completedHandlers.forEach((handler) =>
        handler({
          runId: 4,
          state: 'completed',
          counters: {
            rootsDiscovered: 1,
            rootsCompleted: 1,
            filesDiscovered: 1,
            filesProcessed: 1,
            filesHashed: 1,
            bytesHashed: 3,
            issuesFound: 0,
          },
          durationMs: 12,
        }),
      );
    });

    expect(await screen.findByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'LIBRARY IS EMPTY' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /All systems/ })).toHaveTextContent('1');
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(1);
    expect(mocks.rescanLibrary).not.toHaveBeenCalled();
  });

  it('keeps the shell available on library query failure and retries', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary
      .mockRejectedValueOnce(new mocks.IpcError('database_unavailable', 'internal query detail'))
      .mockResolvedValueOnce(populatedLibraryPage);
    render(<AppShell />);

    expect(await screen.findByText('LIBRARY QUERY UNAVAILABLE')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /All systems/ })).toBeInTheDocument();
    expect(screen.getByRole('alert')).not.toHaveTextContent('internal query detail');
    fireEvent.click(screen.getByRole('button', { name: 'RETRY LIBRARY' }));
    expect(await screen.findByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
  });

  it('searches through the debounced backend query and distinguishes no results', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary
      .mockResolvedValueOnce(populatedLibraryPage)
      .mockResolvedValueOnce({ items: [], total: 0, offset: 0, limit: 60 })
      .mockResolvedValueOnce(populatedLibraryPage);
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    const search = screen.getByRole('searchbox', { name: 'Search' });
    fireEvent.change(search, { target: { value: 'Nothing%_\\' } });
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(1);
    expect(
      await screen.findByRole('heading', { name: 'NO MATCH FOR “Nothing%_\\”' }),
    ).toBeInTheDocument();
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
      search: 'Nothing%_\\',
      sort: 'titleAsc',
      offset: 0,
    });
    expect(screen.queryByRole('heading', { name: 'LIBRARY IS EMPTY' })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'Clear library search' }));
    expect(await screen.findByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(search).toHaveValue('');
  });

  it('uses backend system identities and summary counts for sidebar filters', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 0,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    const nes = screen.getByRole('button', { name: /Nintendo Entertainment System2/ });
    expect(nes).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(nes);
    await waitFor(() =>
      expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
        systemId: 'nes',
        sort: 'titleAsc',
        offset: 0,
      }),
    );
    expect(nes).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: /All systems2/ })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
  });

  it('keeps the Favorites Only filter working and unrelated to card selection', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValueOnce(populatedLibraryPage).mockResolvedValueOnce({
      ...populatedLibraryPage,
      items: [populatedLibraryPage.items[1]],
      total: 1,
    });
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    const favoritesFilter = screen.getByRole('button', { name: 'FAVORITES ONLY' });
    fireEvent.click(favoritesFilter);
    await waitFor(() =>
      expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
        favoritesOnly: true,
        sort: 'titleAsc',
        offset: 0,
      }),
    );
    expect(favoritesFilter).toHaveAttribute('aria-pressed', 'true');

    // A favorited game can be selected, and selecting it never writes favorite state.
    const select = await screen.findByRole('button', {
      name: 'Select A Very Long Local Title Without Metadata',
    });
    fireEvent.click(select);
    expect(
      screen.getByRole('button', { name: 'Deselect A Very Long Local Title Without Metadata' }),
    ).toHaveAttribute('aria-pressed', 'true');
    expect(await screen.findByText('1 SELECTED')).toBeVisible();
    expect(mocks.setGameFavorite).not.toHaveBeenCalled();
    expect(favoritesFilter).toHaveAttribute('aria-pressed', 'true');

    // Clearing the selection leaves the persisted favorite and the Favorites filter untouched.
    fireEvent.click(screen.getByRole('button', { name: 'CLEAR SELECTION' }));
    expect(screen.queryByText('1 SELECTED')).not.toBeInTheDocument();
    expect(mocks.setGameFavorite).not.toHaveBeenCalled();
    expect(favoritesFilter).toHaveAttribute('aria-pressed', 'true');
  });

  it('places the clear search and filters action at the right edge of the filter toolbar', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValueOnce(populatedLibraryPage).mockResolvedValueOnce({
      ...populatedLibraryPage,
      items: [populatedLibraryPage.items[1]],
      total: 1,
    });
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'FAVORITES ONLY' }));

    const filterBar = screen.getByRole('group', { name: 'Library filters' });
    const resetButton = await screen.findByRole('button', { name: 'CLEAR SEARCH & FILTERS' });
    expect(filterBar.lastElementChild).toBe(resetButton);
  });

  it('refreshes the bounded page once on completion and never on scan progress', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 2,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 2 }],
    });
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    const initialCalls = mocks.queryLibrary.mock.calls.length;

    act(() =>
      mocks.progressHandlers.forEach((handler) =>
        handler({
          runId: 41,
          phase: 'hashing',
          counters: {
            rootsDiscovered: 1,
            rootsCompleted: 0,
            filesDiscovered: 2,
            filesProcessed: 1,
            filesHashed: 1,
            bytesHashed: 10,
            issuesFound: 0,
          },
        }),
      ),
    );
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(initialCalls);

    const completion: ScanSummary = {
      runId: 41,
      state: 'completed',
      counters: {
        rootsDiscovered: 1,
        rootsCompleted: 1,
        filesDiscovered: 2,
        filesProcessed: 2,
        filesHashed: 2,
        bytesHashed: 20,
        issuesFound: 0,
      },
      durationMs: 1000,
    };
    act(() => mocks.completedHandlers.forEach((handler) => handler(completion)));
    await waitFor(() => expect(mocks.queryLibrary).toHaveBeenCalledTimes(initialCalls + 1));
    act(() => mocks.completedHandlers.forEach((handler) => handler(completion)));
    expect(mocks.queryLibrary).toHaveBeenCalledTimes(initialCalls + 1);
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

describe('AppShell M6.7A library composition', () => {
  const populatedSummary = {
    totalGames: 2,
    favoriteGames: 1,
    systems: [{ systemId: 'nes' as const, gameCount: 2 }],
  };

  const terminalCounters = {
    rootsDiscovered: 1,
    rootsCompleted: 1,
    filesDiscovered: 2,
    filesProcessed: 2,
    filesHashed: 2,
    bytesHashed: 20,
    issuesFound: 0,
  };

  const healthyResult: ScanSummary = {
    runId: 7,
    state: 'completed',
    counters: terminalCounters,
    durationMs: 4100,
  };

  const issueBearingResult: ScanSummary = {
    runId: 8,
    state: 'completed',
    counters: { ...terminalCounters, issuesFound: 1 },
    durationMs: 4100,
  };

  const failedResult: ScanSummary = {
    runId: 9,
    state: 'failed',
    counters: terminalCounters,
    durationMs: 900,
  };

  const unreadableIssuePage = {
    issues: [
      {
        id: 10,
        scanRunId: 8,
        rootId: 1,
        kind: 'unreadablePath' as const,
        relativePath: 'broken.nes',
        relatedPath: null,
        detail: 'Cannot read file',
        createdAt: 1,
      },
    ],
    scanRunId: 8,
    total: 1,
    offset: 0,
    limit: 50,
  };

  function terminalStatus(lastResult: ScanSummary) {
    return { running: false, progress: null, lastResult };
  }

  beforeEach(() => {
    setupDefaults();
    window.history.replaceState({}, '', '/library');
    mocks.getLibrarySummary.mockResolvedValue(populatedSummary);
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
    mocks.getScanStatus.mockResolvedValue(terminalStatus(healthyResult));
  });

  it('keeps exactly one visible Library heading in the populated state', async () => {
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    expect(screen.queryByRole('heading', { name: 'BROWSE LIBRARY' })).not.toBeInTheDocument();
    const libraryHeadings = screen.getAllByRole('heading', { name: /LIBRARY$/ });
    expect(libraryHeadings).toHaveLength(1);
    expect(libraryHeadings[0]).toBeVisible();
    expect(libraryHeadings[0].tagName).toBe('H1');
    expect(libraryHeadings[0]).toHaveAttribute('id', 'library-heading');
  });

  it('renders the populated filter toolbar before the authoritative Library heading', async () => {
    render(<AppShell />);

    const filterBar = await screen.findByRole('group', { name: 'Library filters' });
    const libraryHeading = await screen.findByRole('heading', { name: 'LIBRARY', level: 1 });

    expect(
      filterBar.compareDocumentPosition(libraryHeading) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('keeps the main landmark labelled by the single Library heading', async () => {
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    const main = screen.getByRole('main');
    expect(main).toHaveAttribute('aria-labelledby', 'library-heading');
    expect(main.querySelectorAll('#library-heading')).toHaveLength(1);
  });

  it('does not put a large scan-result panel in front of a healthy populated grid', async () => {
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    expect(screen.queryByRole('heading', { name: 'SCAN COMPLETE' })).not.toBeInTheDocument();
    expect(screen.queryByText(/scan finished without recorded issues/i)).not.toBeInTheDocument();
  });

  it('removes persistent healthy scan history UI while preserving a non-visual completion announcement', async () => {
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    expect(screen.queryByRole('heading', { name: 'SCAN COMPLETE' })).not.toBeInTheDocument();
    expect(screen.queryByText('LAST SCAN')).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'SCAN ISSUES' })).not.toBeInTheDocument();

    const announcement = screen
      .getByText(/scan finished successfully/i)
      .closest<HTMLElement>('[role="status"]');
    expect(announcement).not.toBeNull();
    expect(announcement).toHaveClass('visually-hidden');
    expect(announcement).toHaveTextContent('2 GAMES AVAILABLE');
    expect(announcement).toHaveTextContent('0 ISSUES');
  });

  it('keeps a failed terminal scan prominent in a populated library', async () => {
    mocks.getScanStatus.mockResolvedValue(terminalStatus(failedResult));
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'SCAN FINISHED WITH ERRORS' })).toBeVisible();
    expect(screen.queryByText('LAST SCAN')).not.toBeInTheDocument();
  });

  it('keeps the issue workflow for a successful scan that recorded real issues', async () => {
    mocks.getScanStatus.mockResolvedValue(terminalStatus(issueBearingResult));
    mocks.getScanIssuePage.mockResolvedValue(unreadableIssuePage);
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'SCAN COMPLETE' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'SCAN ISSUES' })).toBeVisible();
    expect(screen.getByText('Path unreadable')).toBeVisible();
    expect(screen.queryByText('LAST SCAN')).not.toBeInTheDocument();
  });

  it('does not render a scan-issue region for a zero-total issue page', async () => {
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    expect(screen.queryByRole('heading', { name: 'SCAN ISSUES' })).not.toBeInTheDocument();
    expect(screen.queryByText(/no persisted scan issues were recorded/i)).not.toBeInTheDocument();
  });

  it('still offers issue retry when the bounded issue page fails in a populated library', async () => {
    mocks.getScanIssuePage.mockRejectedValue(
      new mocks.IpcError('library_unavailable', 'internal issue-store detail'),
    );
    render(<AppShell />);

    expect(await screen.findByText('SCAN ISSUES UNAVAILABLE')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'RETRY ISSUES' })).toBeVisible();
    expect(screen.getByRole('alert')).not.toHaveTextContent('internal issue-store detail');
  });

  it('hides the pagination row when the bounded result fits on a single page', async () => {
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    expect(screen.queryByRole('navigation', { name: 'Library pages' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'PREVIOUS PAGE' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'NEXT PAGE' })).not.toBeInTheDocument();
    expect(screen.queryByText('PAGE 1 OF 1')).not.toBeInTheDocument();
  });

  it('keeps pagination rendered and operable when more than one page exists', async () => {
    mocks.getLibrarySummary.mockResolvedValue({
      totalGames: 121,
      favoriteGames: 1,
      systems: [{ systemId: 'nes', gameCount: 121 }],
    });
    mocks.queryLibrary
      .mockResolvedValueOnce({ ...populatedLibraryPage, total: 121, offset: 0 })
      .mockResolvedValue({ ...populatedLibraryPage, total: 121, offset: 60 });
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    expect(screen.getByRole('navigation', { name: 'Library pages' })).toBeVisible();
    expect(screen.getByText('PAGE 1 OF 3')).toBeVisible();
    expect(screen.getByRole('button', { name: 'PREVIOUS PAGE' })).toBeDisabled();

    fireEvent.click(screen.getByRole('button', { name: 'NEXT PAGE' }));
    await screen.findByText('PAGE 2 OF 3');
    expect(mocks.queryLibrary).toHaveBeenLastCalledWith({ sort: 'titleAsc', offset: 60 });
  });

  it('keeps the query result range and system context as live compact metadata', async () => {
    render(<AppShell />);

    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    const range = screen.getByText('1–2 OF 2');
    expect(range).toBeVisible();
    expect(range).toHaveAttribute('aria-live', 'polite');
    expect(screen.getByText('ALL SYSTEMS')).toBeVisible();
    expect(screen.queryByText('TITLE ORDER · LOCAL DATA')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Nintendo Entertainment System/ }));
    expect(await screen.findByText('NINTENDO ENTERTAINMENT SYSTEM')).toBeVisible();
  });

  it('keeps the large scan result while the library is still empty', async () => {
    mocks.getLibrarySummary.mockResolvedValue({ totalGames: 0, favoriteGames: 0, systems: [] });
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'SCAN COMPLETE' })).toBeVisible();
    expect(screen.getByRole('heading', { name: 'LIBRARY IS EMPTY' })).toBeVisible();
    expect(screen.queryByText('LAST SCAN')).not.toBeInTheDocument();
  });

  it('replaces the underlying Library with a dedicated running Scan composition', async () => {
    mocks.getScanStatus.mockResolvedValue({
      running: true,
      progress: {
        runId: 11,
        phase: 'hashing' as const,
        counters: { ...terminalCounters, filesProcessed: 1 },
      },
      lastResult: healthyResult,
    });
    render(<AppShell />);

    expect(await screen.findByRole('heading', { name: 'SCAN IN PROGRESS' })).toBeVisible();
    expect(screen.queryByText('LAST SCAN')).not.toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'LIBRARY' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('complementary', { name: 'Library navigation' }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('search')).not.toBeInTheDocument();
    expect(screen.queryByRole('group', { name: 'Library filters' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('link', { name: /Open Kirby’s Adventure details/ }),
    ).not.toBeInTheDocument();
  });
});

// M6.7 B1 selection delta: the Library card is no longer the Favorite mutation surface; it carries
// the B1 multi-select checkbox, and selection is transient frontend state owned by the Library
// composition.
describe('AppShell M6.7 B1 library card selection', () => {
  const populatedSummary = {
    totalGames: 2,
    favoriteGames: 1,
    systems: [{ systemId: 'nes' as const, gameCount: 2 }],
  };

  beforeEach(() => {
    setupDefaults();
    window.history.replaceState({}, '', '/library');
    mocks.getLibrarySummary.mockResolvedValue(populatedSummary);
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
  });

  it('renders one selection control per card and no Favorite star in the grid', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    expect(screen.getByRole('button', { name: 'Select Kirby’s Adventure' })).toBeVisible();
    expect(
      screen.getByRole('button', { name: 'Select A Very Long Local Title Without Metadata' }),
    ).toBeVisible();
    expect(document.querySelectorAll('.game-card-select')).toHaveLength(2);
    expect(screen.queryByRole('button', { name: /favorites$/i })).not.toBeInTheDocument();
    expect(document.querySelector('.game-card-favorite')).toBeNull();
  });

  it('shows no selection bar until something is selected', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    expect(screen.queryByRole('group', { name: 'Library selection' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'CLEAR SELECTION' })).not.toBeInTheDocument();
    expect(screen.queryByText(/SELECTED/)).not.toBeInTheDocument();
  });

  it('places the selection bar between the filter toolbar and the LIBRARY heading', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'Select Kirby’s Adventure' }));

    const filterBar = screen.getByRole('group', { name: 'Library filters' });
    const selectionBar = screen.getByRole('group', { name: 'Library selection' });
    const libraryHeading = screen.getByRole('heading', { name: 'LIBRARY', level: 1 });

    expect(
      filterBar.compareDocumentPosition(selectionBar) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      selectionBar.compareDocumentPosition(libraryHeading) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it('counts one, then several, selected cards and never navigates to Game Detail', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'Select Kirby’s Adventure' }));
    expect(screen.getByText('1 SELECTED')).toBeVisible();

    fireEvent.click(
      screen.getByRole('button', { name: 'Select A Very Long Local Title Without Metadata' }),
    );
    expect(screen.getByText('2 SELECTED')).toBeVisible();
    expect(screen.queryByText('1 SELECTED')).not.toBeInTheDocument();

    // Selection never leaves the Library route.
    expect(window.location.pathname).toBe('/library');
    expect(screen.getByRole('heading', { name: 'LIBRARY', level: 1 })).toBeVisible();
    expect(mocks.getLibraryGameDetail).not.toHaveBeenCalled();
  });

  it('deselects the same card again without navigating', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'Select Kirby’s Adventure' }));
    fireEvent.click(screen.getByRole('button', { name: 'Deselect Kirby’s Adventure' }));

    expect(screen.getByRole('button', { name: 'Select Kirby’s Adventure' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    expect(screen.queryByRole('group', { name: 'Library selection' })).not.toBeInTheDocument();
    expect(window.location.pathname).toBe('/library');
    expect(mocks.getLibraryGameDetail).not.toHaveBeenCalled();
  });

  it('clears every selected card from the selection bar', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'Select Kirby’s Adventure' }));
    fireEvent.click(
      screen.getByRole('button', { name: 'Select A Very Long Local Title Without Metadata' }),
    );
    expect(screen.getByText('2 SELECTED')).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'CLEAR SELECTION' }));

    expect(screen.queryByRole('group', { name: 'Library selection' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Select Kirby’s Adventure' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );
    expect(
      screen.getByRole('button', { name: 'Select A Very Long Local Title Without Metadata' }),
    ).toHaveAttribute('aria-pressed', 'false');
    expect(window.location.pathname).toBe('/library');
  });

  it('keeps a missing local file selectable and still browsable', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    expect(screen.getByText('MISSING')).toBeVisible();
    const select = screen.getByRole('button', {
      name: 'Select A Very Long Local Title Without Metadata',
    });
    expect(select).not.toBeDisabled();
    fireEvent.click(select);

    expect(screen.getByText('1 SELECTED')).toBeVisible();
    expect(
      screen.getByRole('link', { name: 'Open A Very Long Local Title Without Metadata details' }),
    ).toHaveAttribute('href', '/games/2');
  });

  it('keeps the full-card detail target working while a card is selected', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'Select Kirby’s Adventure' }));
    fireEvent.click(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }));

    expect(
      await screen.findByRole('heading', { name: 'Kirby’s Adventure', level: 1 }),
    ).toBeVisible();
    expect(window.location.pathname).toBe('/games/1');
  });

  it('leaves no invisible selection behind when the committed query changes', async () => {
    mocks.queryLibrary.mockReset();
    mocks.queryLibrary.mockResolvedValueOnce(populatedLibraryPage).mockResolvedValue({
      ...populatedLibraryPage,
      items: [populatedLibraryPage.items[0]],
      total: 1,
    });
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'A Very Long Local Title Without Metadata' });

    fireEvent.click(
      screen.getByRole('button', { name: 'Select A Very Long Local Title Without Metadata' }),
    );
    expect(screen.getByText('1 SELECTED')).toBeVisible();

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search' }), {
      target: { value: 'kirby' },
    });
    await waitFor(() =>
      expect(mocks.queryLibrary).toHaveBeenLastCalledWith({
        search: 'kirby',
        sort: 'titleAsc',
        offset: 0,
      }),
    );

    await waitFor(() =>
      expect(screen.queryByRole('group', { name: 'Library selection' })).not.toBeInTheDocument(),
    );
    expect(screen.queryByText(/SELECTED/)).not.toBeInTheDocument();
  });

  it('leaves no hidden selected cards behind after page navigation', async () => {
    const firstPage = { ...populatedLibraryPage, total: 61 };
    const secondPage = {
      items: [{ ...populatedLibraryPage.items[0], gameId: 61, displayTitle: 'Page Two Game' }],
      total: 61,
      offset: 60,
      limit: 60,
    };
    mocks.getLibrarySummary.mockResolvedValue({ ...populatedSummary, totalGames: 61 });
    mocks.queryLibrary.mockReset();
    mocks.queryLibrary.mockResolvedValueOnce(firstPage).mockResolvedValue(secondPage);
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    fireEvent.click(screen.getByRole('button', { name: 'Select Kirby’s Adventure' }));
    expect(screen.getByText('1 SELECTED')).toBeVisible();

    fireEvent.click(screen.getByRole('button', { name: 'NEXT PAGE' }));

    expect(await screen.findByRole('heading', { name: 'Page Two Game' })).toBeVisible();
    expect(screen.queryByRole('group', { name: 'Library selection' })).not.toBeInTheDocument();
    expect(screen.queryByText(/SELECTED/)).not.toBeInTheDocument();
  });
});

describe('AppShell M8 controller navigation and focus', () => {
  const populatedM8Summary = {
    totalGames: 2,
    favoriteGames: 1,
    systems: [{ systemId: 'nes' as const, gameCount: 2 }],
  };

  let pads: (FakeGamepad | null)[] = [];

  interface FakeGamepad {
    index: number;
    id: string;
    mapping: string;
    connected: boolean;
    buttons: { pressed: boolean }[];
    axes: number[];
  }

  function fakePad(pressedIndex: number | null = null): FakeGamepad {
    return {
      index: 0,
      id: 'Qualification Pad (STANDARD GAMEPAD)',
      mapping: 'standard',
      connected: true,
      buttons: Array.from({ length: 17 }, (_value, index) => ({ pressed: index === pressedIndex })),
      axes: [0, 0, 0, 0],
    };
  }

  /** Lets the real animation-frame polling loop observe the current controller state. */
  async function polled() {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 32));
    });
  }

  /** Presses one Standard Gamepad button and lets the polling loop observe press and release. */
  async function pressButton(buttonIndex: number) {
    pads = [fakePad(buttonIndex)];
    await polled();
    pads = [fakePad()];
    await polled();
  }

  async function connectController() {
    pads = [fakePad()];
    await polled();
  }

  function layoutLibrary() {
    layoutGrid(Array.from(document.querySelectorAll('[data-game-detail-link]')), 2);
    layoutColumn(Array.from(document.querySelectorAll('.pixel-row')));
  }

  function footerHints() {
    return screen.getByRole('list', { name: 'Controller actions' });
  }

  /**
   * Flushes the native window-focus resolution.
   *
   * The desktop ownership gate starts closed and only opens once `isAppWindowFocused()` and the
   * focus subscription have both resolved, so a test that asserts on keyboard or controller
   * behaviour must let those settle first — otherwise it would pass merely because input was not
   * owned yet.
   */
  async function inputOwnershipSettled() {
    await act(async () => undefined);
    expect(mocks.isDesktopRuntime).toHaveBeenCalled();
  }

  beforeEach(() => {
    setupDefaults();
    installRectStub();
    pads = [];
    vi.stubGlobal('navigator', { ...window.navigator, getGamepads: () => pads });
    window.history.replaceState({}, '', '/library');
    mocks.getLibrarySummary.mockResolvedValue(populatedM8Summary);
    mocks.queryLibrary.mockResolvedValue(populatedLibraryPage);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('moves Library focus across the rendered grid with the D-pad', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    await connectController();
    // With nothing focused, the first directional action enters the first navigable node.
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    expect(screen.getByRole('button', { name: /All systems/ })).toHaveFocus();

    // Right leaves the sidebar for the game grid, then moves along the rendered row.
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadRight);
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveFocus();

    await pressButton(GAMEPAD_BUTTON_INDEX.dpadRight);
    expect(
      screen.getByRole('link', { name: 'Open A Very Long Local Title Without Metadata details' }),
    ).toHaveFocus();

    await pressButton(GAMEPAD_BUTTON_INDEX.dpadLeft);
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveFocus();
  });

  it('opens a game with confirm and returns to the same card with back', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    await connectController();
    act(() => screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }).focus());
    await pressButton(GAMEPAD_BUTTON_INDEX.confirm);

    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });
    expect(window.location.pathname).toBe('/games/1');

    await pressButton(GAMEPAD_BUTTON_INDEX.back);

    await screen.findByRole('heading', { name: 'LIBRARY' });
    await waitFor(() =>
      expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveFocus(),
    );
  });

  it('selects the focused card with context without opening it', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    await connectController();
    act(() => screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }).focus());
    await pressButton(GAMEPAD_BUTTON_INDEX.context);

    expect(window.location.pathname).toBe('/library');
    expect(await screen.findByText('1 SELECTED')).toBeVisible();
  });

  it('shows only the actions the focused node really supports', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    act(() => {
      screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }).focus();
    });
    expect(within(footerHints()).getByText('OPEN')).toBeInTheDocument();
    expect(within(footerHints()).getByText('SELECT')).toBeInTheDocument();
    // The Library is the root destination, so there is nothing to go back to.
    expect(within(footerHints()).queryByText('LIBRARY')).not.toBeInTheDocument();

    act(() => {
      screen.getByRole('button', { name: /All systems/ }).focus();
    });
    expect(within(footerHints()).queryByText('SELECT')).not.toBeInTheDocument();
  });

  it('offers a back hint on Game Detail and none on the Library root', async () => {
    window.history.replaceState({}, '', '/games/1');
    render(<AppShell />);
    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });

    expect(within(footerHints()).getByText('LIBRARY')).toBeInTheDocument();
  });

  it('stops consuming controller input while a managed game is running', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    act(() => {
      mocks.launchHandlers.forEach((handler) =>
        handler({
          state: {
            running: {
              sessionId: 7,
              gameId: 1,
              contentUnitId: 101,
              coreId: 'nestopia',
              startedAt: 1,
            },
            blocked: false,
          },
        }),
      );
    });

    await connectController();
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    expect(document.body).toHaveFocus();
  });

  it('stops consuming controller input while launch state is blocked', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    act(() => {
      mocks.launchHandlers.forEach((handler) =>
        handler({ state: { running: null, blocked: true } }),
      );
    });

    await connectController();
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    expect(document.body).toHaveFocus();
  });

  it('stops consuming controller input while the application window is unfocused', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    act(() => mocks.windowFocusHandlers.forEach((handler) => handler(false)));

    await connectController();
    act(() => screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }).focus());
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadRight);
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveFocus();

    act(() => mocks.windowFocusHandlers.forEach((handler) => handler(true)));
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadRight);
    expect(
      screen.getByRole('link', { name: 'Open A Very Long Local Title Without Metadata details' }),
    ).toHaveFocus();
  });

  it('returns the window once and restores launch focus when the game ends', async () => {
    window.history.replaceState({}, '', '/games/1');
    render(<AppShell />);
    const play = await screen.findByRole('button', { name: 'Play Kirby’s Adventure' });
    act(() => play.focus());

    act(() =>
      mocks.launchHandlers.forEach((handler) =>
        handler({
          state: {
            running: {
              sessionId: 7,
              gameId: 1,
              contentUnitId: 101,
              coreId: 'nestopia',
              startedAt: 1,
            },
            blocked: false,
          },
        }),
      ),
    );
    act(() => mocks.windowFocusHandlers.forEach((handler) => handler(false)));
    expect(mocks.requestAppWindowFocus).not.toHaveBeenCalled();

    // Whatever happens to DOM focus while RetroArch owns the screen, the launch origin is what the
    // return restores.
    act(() => screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' }).focus());

    act(() =>
      mocks.launchHandlers.forEach((handler) =>
        handler({ state: { running: null, blocked: false } }),
      ),
    );
    await waitFor(() => expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('button', { name: 'Play Kirby’s Adventure' })).not.toHaveFocus();

    act(() => mocks.windowFocusHandlers.forEach((handler) => handler(true)));
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Play Kirby’s Adventure' })).toHaveFocus(),
    );

    // A later window focus change must not steal focus a second time.
    act(() => screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' }).focus());
    act(() => mocks.windowFocusHandlers.forEach((handler) => handler(false)));
    act(() => mocks.windowFocusHandlers.forEach((handler) => handler(true)));
    expect(
      screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' }),
    ).toHaveFocus();
    expect(mocks.requestAppWindowFocus).toHaveBeenCalledTimes(1);
  });

  it('keeps Tab, pointer focus, and search typing working alongside controller navigation', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    const search = screen.getByRole('searchbox', { name: 'Search' });
    act(() => search.focus());
    fireEvent.keyDown(search, { key: 'ArrowDown' });
    expect(search).toHaveFocus();
    fireEvent.change(search, { target: { value: 'kirby' } });
    expect(search).toHaveValue('kirby');

    act(() => {
      screen.getByRole('link', { name: 'Open Kirby’s Adventure details' }).focus();
    });
    fireEvent.keyDown(document.body, { key: 'ArrowRight' });
    expect(
      screen.getByRole('link', { name: 'Open A Very Long Local Title Without Metadata details' }),
    ).toHaveFocus();
  });

  it('stops consuming controller input while a launch request is still pending', async () => {
    // The backend creates the process and settles the launch before the authoritative running state
    // reaches React. Ownership must therefore be released at the launch request, not at `running`.
    window.history.replaceState({}, '', '/games/1');
    let settleLaunch: ((value: LaunchResponse) => void) | undefined;
    mocks.launchGame.mockImplementation(
      () =>
        new Promise<LaunchResponse>((resolve) => {
          settleLaunch = resolve;
        }),
    );

    render(<AppShell />);
    const play = await screen.findByRole('button', { name: 'Play Kirby’s Adventure' });
    const favorite = screen.getByRole('button', {
      name: 'Add Kirby’s Adventure to favorites',
    });
    layoutColumn([play, favorite]);

    await connectController();
    act(() => play.focus());
    await act(async () => {
      fireEvent.click(play);
    });
    expect(mocks.launchGame).toHaveBeenCalledTimes(1);

    await pressButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    expect(favorite).not.toHaveFocus();
    await pressButton(GAMEPAD_BUTTON_INDEX.confirm);
    expect(mocks.launchGame).toHaveBeenCalledTimes(1);

    // The launch failed without ever starting a process: ownership returns immediately.
    await act(async () => {
      settleLaunch?.({
        status: 'failed',
        error: {
          code: 'runtimeNotReady',
          message: 'The managed runtime is not ready.',
          context: {
            systemId: 'nes',
            coreId: null,
            biosRequirementIds: [],
            runtimeState: null,
            hostPrerequisite: null,
            exitCode: null,
            contentOptions: [],
          },
        },
      });
    });
    // Ownership returns immediately, and the failure surface — a temporary M8 focus scope — takes
    // entry focus. The controller can therefore act on it at once.
    const failure = await screen.findByRole('group', { name: 'Launch failed' });
    const dismiss = within(failure).getByRole('button', { name: 'DISMISS' });
    await waitFor(() => expect(dismiss).toHaveFocus());
    await pressButton(GAMEPAD_BUTTON_INDEX.confirm);
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: 'Launch failed' })).not.toBeInTheDocument(),
    );
    expect(play).toHaveFocus();

    layoutColumn([play, favorite]);
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    expect(favorite).toHaveFocus();
  });

  it('does not replay a direction held across the launch transition', async () => {
    window.history.replaceState({}, '', '/games/1');
    let settleLaunch: ((value: LaunchResponse) => void) | undefined;
    mocks.launchGame.mockImplementation(
      () =>
        new Promise<LaunchResponse>((resolve) => {
          settleLaunch = resolve;
        }),
    );

    render(<AppShell />);
    const play = await screen.findByRole('button', { name: 'Play Kirby’s Adventure' });
    const favorite = screen.getByRole('button', {
      name: 'Add Kirby’s Adventure to favorites',
    });
    layoutColumn([play, favorite]);

    await connectController();
    act(() => play.focus());
    await act(async () => {
      fireEvent.click(play);
    });

    // The direction stays physically held for the whole uncertain interval and beyond it.
    pads = [fakePad(GAMEPAD_BUTTON_INDEX.dpadDown)];
    await polled();
    await polled();
    expect(favorite).not.toHaveFocus();

    await act(async () => {
      settleLaunch?.({
        status: 'failed',
        error: {
          code: 'runtimeNotReady',
          message: 'The managed runtime is not ready.',
          context: {
            systemId: 'nes',
            coreId: null,
            biosRequirementIds: [],
            runtimeState: null,
            hostPrerequisite: null,
            exitCode: null,
            contentOptions: [],
          },
        },
      });
    });
    // Ownership has returned while the direction is still physically held: it must be adopted, not
    // replayed, so nothing moves until it is released and pressed again.
    const failure = await screen.findByRole('group', { name: 'Launch failed' });
    const dismiss = within(failure).getByRole('button', { name: 'DISMISS' });
    await waitFor(() => expect(dismiss).toHaveFocus());
    await polled();
    await polled();
    expect(dismiss).toHaveFocus();

    // Released, dismissed, and only then does a fresh press move again.
    pads = [fakePad()];
    await polled();
    await pressButton(GAMEPAD_BUTTON_INDEX.confirm);
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: 'Launch failed' })).not.toBeInTheDocument(),
    );
    layoutColumn([play, favorite]);
    expect(play).toHaveFocus();
    await pressButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    expect(favorite).toHaveFocus();
  });

  it('lets the controller act on content selection once the launch request resolved', async () => {
    window.history.replaceState({}, '', '/games/1');
    mocks.launchGame.mockResolvedValue({
      status: 'contentSelectionRequired',
      options: [
        {
          contentUnitId: 11,
          localTitle: 'Kirby Disc 1',
          kind: 'singleFile',
          fileCount: 1,
          availability: 'available',
        },
        {
          contentUnitId: 12,
          localTitle: 'Kirby Disc 2',
          kind: 'singleFile',
          fileCount: 1,
          availability: 'available',
        },
      ],
    });

    render(<AppShell />);
    const play = await screen.findByRole('button', { name: 'Play Kirby’s Adventure' });
    await connectController();
    act(() => play.focus());
    await act(async () => {
      fireEvent.click(play);
    });

    const surface = await screen.findByRole('group', { name: 'Choose a version' });
    const first = within(surface).getByRole('button', { name: /Kirby Disc 1/ });
    const second = within(surface).getByRole('button', { name: /Kirby Disc 2/ });
    layoutColumn([first, second]);
    act(() => first.focus());

    await pressButton(GAMEPAD_BUTTON_INDEX.dpadDown);
    expect(second).toHaveFocus();
    await pressButton(GAMEPAD_BUTTON_INDEX.confirm);
    expect(mocks.launchGame).toHaveBeenLastCalledWith({ gameId: 1, contentUnitId: 12 });
  });

  it('updates the footer action immediately when the focused card changes state', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });
    layoutLibrary();

    await connectController();
    const card = screen.getByRole('link', { name: 'Open Kirby’s Adventure details' });
    act(() => card.focus());
    expect(within(footerHints()).getByText('SELECT')).toBeInTheDocument();

    // Context selects the card. Its focus identity does not change, but the action it now supports
    // does, and the footer must say so without waiting for anything else to move.
    await pressButton(GAMEPAD_BUTTON_INDEX.context);
    expect(card).toHaveFocus();
    expect(within(footerHints()).getByText('DESELECT')).toBeInTheDocument();
    expect(within(footerHints()).queryByText('SELECT')).not.toBeInTheDocument();

    await pressButton(GAMEPAD_BUTTON_INDEX.context);
    expect(within(footerHints()).getByText('SELECT')).toBeInTheDocument();
  });

  it('drops the confirm hint when the focused control becomes disabled', async () => {
    window.history.replaceState({}, '', '/games/1');
    let settleLaunch: ((value: LaunchResponse) => void) | undefined;
    mocks.launchGame.mockImplementation(
      () =>
        new Promise<LaunchResponse>((resolve) => {
          settleLaunch = resolve;
        }),
    );

    render(<AppShell />);
    const play = await screen.findByRole('button', { name: 'Play Kirby’s Adventure' });
    act(() => play.focus());
    expect(within(footerHints()).getByText('PLAY')).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(play);
    });
    expect(within(footerHints()).queryByText('PLAY')).not.toBeInTheDocument();
    expect(settleLaunch).toBeDefined();
  });

  it('leaves Escape in the Library search to the platform instead of navigating', async () => {
    render(<AppShell />);
    await screen.findByRole('heading', { name: 'Kirby’s Adventure' });

    await inputOwnershipSettled();

    const search = screen.getByRole('searchbox', { name: 'Search' });
    act(() => search.focus());
    fireEvent.change(search, { target: { value: 'kirby' } });
    // `fireEvent` returns false when a handler called preventDefault. The Library has no semantic
    // Back at all, so suppressing the field's own Escape would remove behaviour and add none.
    const notPrevented = fireEvent.keyDown(search, { key: 'Escape' });

    expect(notPrevented).toBe(true);
    expect(window.location.pathname).toBe('/library');
    expect(search).toHaveFocus();
  });

  it('does not navigate away when Escape is pressed in the Settings credential fields', async () => {
    window.history.replaceState({}, '', '/settings');
    render(<AppShell />);
    const username = await screen.findByLabelText('ACCOUNT NAME');
    const password = screen.getByLabelText('ACCOUNT PASSWORD');
    await inputOwnershipSettled();

    for (const field of [username, password]) {
      act(() => field.focus());
      expect(fireEvent.keyDown(field, { key: 'Escape' })).toBe(true);
      expect(field).toHaveFocus();
      expect(window.location.pathname).toBe('/settings');
    }
  });

  it('still produces the route back from an ordinary focused control', async () => {
    window.history.replaceState({}, '', '/games/1');
    render(<AppShell />);
    await screen.findByRole('heading', { level: 1, name: 'Kirby’s Adventure' });

    await inputOwnershipSettled();

    const back = screen.getByRole('link', { name: /BACK TO LIBRARY/ });
    act(() => back.focus());
    fireEvent.keyDown(back, { key: 'Escape' });
    await screen.findByRole('heading', { name: 'LIBRARY' });
    expect(window.location.pathname).toBe('/library');
  });

  it('shows a scope back hint while a temporary surface is open and removes it after', async () => {
    window.history.replaceState({}, '', '/games/1');
    mocks.launchGame.mockResolvedValue({
      status: 'contentSelectionRequired',
      options: [
        {
          contentUnitId: 11,
          localTitle: 'Kirby Disc 1',
          kind: 'singleFile',
          fileCount: 1,
          availability: 'available',
        },
      ],
    });

    render(<AppShell />);
    const play = await screen.findByRole('button', { name: 'Play Kirby’s Adventure' });
    act(() => play.focus());
    expect(within(footerHints()).getByText('LIBRARY')).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(play);
    });
    await screen.findByRole('group', { name: 'Choose a version' });
    // The innermost scope owns `back` while it is open.
    expect(within(footerHints()).getByText('CANCEL')).toBeInTheDocument();
    expect(within(footerHints()).queryByText('LIBRARY')).not.toBeInTheDocument();

    await connectController();
    await pressButton(GAMEPAD_BUTTON_INDEX.back);
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: 'Choose a version' })).not.toBeInTheDocument(),
    );
    expect(within(footerHints()).getByText('LIBRARY')).toBeInTheDocument();
    expect(within(footerHints()).queryByText('CANCEL')).not.toBeInTheDocument();
  });
});
