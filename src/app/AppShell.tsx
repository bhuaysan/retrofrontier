import { useCallback, useEffect, useState } from 'react';

import { InlineError } from '../components/ui/InlineError';
import { PixelRow } from '../components/ui/PixelRow';
import { useContentRoots } from '../hooks/useContentRoots';
import { useLibrarySummary } from '../hooks/useLibrarySummary';
import { useLibraryQuery } from '../hooks/useLibraryQuery';
import { useGameDetail } from '../hooks/useGameDetail';
import { useScanState } from '../hooks/useScanState';
import { useSystemCatalog } from '../hooks/useSystemCatalog';
import { pickExternalContentRoot } from '../platform/folderPicker';
import { openManagedRomFolder, type ScanSummary } from '../platform/ipc';
import { LibraryPage } from '../features/library/LibraryPage';
import { SettingsPage } from '../features/settings/SettingsPage';
import { systemAccent } from '../features/library/systemAccents';
import { GameDetailPage } from '../features/library/GameDetailPage';
import { gameRoute, isGameRoute, useRoute } from './routes';

type Theme = 'dark' | 'light';

const THEME_STORAGE_KEY = 'retrofrontier.theme';

function initialTheme(): Theme {
  return window.localStorage.getItem(THEME_STORAGE_KEY) === 'light' ? 'light' : 'dark';
}

function RouteRow({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return <PixelRow label={label} accent="var(--text-dim)" active={active} onClick={onClick} />;
}

function MobileRouteNav({
  route,
  navigate,
}: {
  route: 'library' | 'settings' | null;
  navigate: (route: 'library' | 'settings') => void;
}) {
  return (
    <nav className="mobile-nav" aria-label="Primary navigation">
      <button
        type="button"
        className={`mobile-nav-link${route === 'library' ? ' mobile-nav-link--active' : ''}`}
        aria-current={route === 'library' ? 'page' : undefined}
        onClick={() => navigate('library')}
      >
        LIBRARY
      </button>
      <button
        type="button"
        className={`mobile-nav-link${route === 'settings' ? ' mobile-nav-link--active' : ''}`}
        aria-current={route === 'settings' ? 'page' : undefined}
        onClick={() => navigate('settings')}
      >
        SETTINGS
      </button>
    </nav>
  );
}

function SystemSummaryRow({
  label,
  count,
  accent,
  active,
  onClick,
}: {
  label: string;
  count: number;
  accent: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <PixelRow
      label={label}
      count={count}
      accent={accent}
      active={active}
      activeMode="pressed"
      onClick={onClick}
    />
  );
}

function LibrarySearch({
  value,
  onChange,
  onClear,
}: {
  value: string;
  onChange: (value: string) => void;
  onClear: () => void;
}) {
  return (
    <search className="library-search">
      <label className="library-search-label" htmlFor="library-search-input">
        SEARCH LIBRARY
      </label>
      <svg aria-hidden="true" shapeRendering="crispEdges" viewBox="0 0 16 16">
        <path
          d="M2 2h9v2h2v2h1v5h-2V7h-1V5H4v6h6v2H2zM10 11h2v2h2v2h-2v-1h-2z"
          fill="currentColor"
        />
      </svg>
      <input
        autoComplete="off"
        id="library-search-input"
        onChange={(event) => onChange(event.target.value)}
        placeholder="TITLE OR LOCAL NAME…"
        spellCheck="false"
        type="search"
        value={value}
      />
      {value ? (
        <button aria-label="Clear library search" onClick={onClear} type="button">
          CLEAR
        </button>
      ) : null}
    </search>
  );
}

export function AppShell() {
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [libraryScanCompletionRunId, setLibraryScanCompletionRunId] = useState<number | null>(null);
  const { route, navigate } = useRoute();
  const isLibraryRoute = route === 'library';
  const isSettingsRoute = route === 'settings';
  const gameRouteState = isGameRoute(route) ? route : null;
  const currentGameId = gameRouteState?.gameId ?? null;
  const [libraryFocusGameId, setLibraryFocusGameId] = useState<number | null>(null);
  const {
    summary,
    loading: summaryLoading,
    error: summaryError,
    refresh: refreshSummary,
  } = useLibrarySummary();
  const {
    roots,
    loading: rootsLoading,
    error: rootsError,
    refresh: refreshRoots,
    addExternalRoot,
    removeExternalRoot,
    updateRootEnabled,
  } = useContentRoots();
  const {
    systems: catalogSystems,
    statuses: catalogStatuses,
    loading: catalogLoading,
    error: catalogError,
    refresh: refreshCatalog,
  } = useSystemCatalog();
  // A ROM scan changes local content only. Runtime state, approved cores, and BIOS files are all
  // outside the scanned roots, so the system catalog is deliberately not refetched here:
  // `useSystemCatalog.refresh()` clears the catalog before refetching, which blanked the sidebar
  // and the readiness panel on every terminal scan.
  const onScanCompleted = useCallback(
    (result: ScanSummary) => {
      setLibraryScanCompletionRunId(result.runId);
      void refreshSummary();
    },
    [refreshSummary],
  );
  const scan = useScanState({ onCompleted: onScanCompleted });
  const onFavoriteCommitted = useCallback(() => {
    void refreshSummary();
  }, [refreshSummary]);
  const library = useLibraryQuery({
    enabled: isLibraryRoute && Boolean(summary && summary.totalGames > 0),
    scanCompletionRunId: libraryScanCompletionRunId,
    onFavoriteCommitted,
  });
  const gameDetail = useGameDetail({
    enabled: gameRouteState !== null && currentGameId !== null,
    gameId: currentGameId,
    scanCompletionRunId: libraryScanCompletionRunId,
    onFavoriteCommitted,
  });

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  }, [theme]);

  const onAddExternalFolder = useCallback(async () => {
    const path = await pickExternalContentRoot();
    if (path === null) {
      return false;
    }
    await addExternalRoot(path);
    await refreshSummary();
    return true;
  }, [addExternalRoot, refreshSummary]);

  const onOpenManagedFolder = useCallback(async () => {
    await openManagedRomFolder();
  }, []);

  const onOpenGame = useCallback(
    (gameId: number) => {
      setLibraryFocusGameId(gameId);
      navigate(gameRoute(gameId));
    },
    [navigate],
  );

  const onBackToLibrary = useCallback(() => {
    navigate('library');
  }, [navigate]);

  const navigateFromShell = useCallback(
    (nextRoute: 'library' | 'settings') => {
      setLibraryFocusGameId(null);
      navigate(nextRoute);
    },
    [navigate],
  );

  const onLibraryFocusRestored = useCallback(() => {
    setLibraryFocusGameId(null);
  }, []);

  const detailSystemStatus = gameDetail.localDetail
    ? (catalogStatuses.find((status) => status.id === gameDetail.localDetail?.systemId) ?? null)
    : null;

  const systemCounts = new Map(
    (summary?.systems ?? []).map((system) => [system.systemId, system.gameCount]),
  );
  const footerStatus = scan.statusError
    ? 'SCAN STATUS UNKNOWN'
    : scan.statusLoading && !scan.status
      ? 'CHECKING SCAN STATUS'
      : scan.status?.running
        ? 'SCAN IN PROGRESS'
        : 'SCAN READY';

  return (
    <div className="app-shell" data-theme={theme}>
      <header className="app-header">
        <button
          type="button"
          className="wordmark"
          aria-label="Go to Library"
          onClick={() => navigateFromShell('library')}
        >
          RETRO<span>FRONTIER</span>
        </button>
        {isLibraryRoute && summary && summary.totalGames > 0 ? (
          <LibrarySearch
            onChange={library.setSearchInput}
            onClear={library.clearSearch}
            value={library.searchInput}
          />
        ) : null}
        <MobileRouteNav
          route={isLibraryRoute ? 'library' : isSettingsRoute ? 'settings' : null}
          navigate={navigateFromShell}
        />
        <div className="theme-toggle" role="group" aria-label="Theme">
          <button
            type="button"
            className={`theme-option${theme === 'dark' ? ' theme-option--active' : ''}`}
            aria-pressed={theme === 'dark'}
            onClick={() => setTheme('dark')}
          >
            DARK
          </button>
          <button
            type="button"
            className={`theme-option${theme === 'light' ? ' theme-option--active' : ''}`}
            aria-pressed={theme === 'light'}
            onClick={() => setTheme('light')}
          >
            LIGHT
          </button>
        </div>
      </header>

      <aside className="app-sidebar" aria-label="Library navigation">
        <section aria-labelledby="systems-heading">
          <p id="systems-heading" className="sidebar-label">
            // SYSTEMS
          </p>
          {catalogLoading && (
            <p className="sidebar-catalog-status loading-inline" role="status">
              CHECKING SYSTEM CATALOG…
            </p>
          )}
          {catalogError && (
            <InlineError
              title="SYSTEM CATALOG UNAVAILABLE"
              message="RetroFrontier could not load the supported systems. No system rows are shown; try again."
              actionLabel="RETRY SYSTEMS"
              onAction={() => void refreshCatalog()}
            />
          )}
          <ul className="pixel-row-list">
            <PixelRow
              label="All systems"
              count={summary?.totalGames ?? 0}
              accent="var(--accent)"
              active={isLibraryRoute && library.systemId === null}
              activeMode="pressed"
              onClick={() => {
                library.setSystemId(null);
                navigateFromShell('library');
              }}
            />
            {catalogSystems.map((system) => (
              <SystemSummaryRow
                key={system.id}
                label={system.displayName}
                count={systemCounts.get(system.id) ?? 0}
                accent={systemAccent(system.id)}
                active={isLibraryRoute && library.systemId === system.id}
                onClick={() => {
                  library.setSystemId(system.id);
                  navigateFromShell('library');
                }}
              />
            ))}
          </ul>
        </section>

        <nav className="sidebar-menu" aria-labelledby="menu-heading">
          <p id="menu-heading" className="sidebar-label">
            // MENU
          </p>
          <ul className="pixel-row-list">
            <RouteRow
              label="Settings"
              active={isSettingsRoute}
              onClick={() => navigateFromShell('settings')}
            />
          </ul>
        </nav>
      </aside>

      {isLibraryRoute ? (
        <LibraryPage
          summary={summary}
          summaryLoading={summaryLoading}
          summaryError={summaryError}
          refreshSummary={refreshSummary}
          roots={roots}
          rootsLoading={rootsLoading}
          rootsError={rootsError}
          refreshRoots={refreshRoots}
          systems={catalogSystems}
          library={library}
          scan={scan}
          onAddExternalFolder={onAddExternalFolder}
          onOpenManagedFolder={onOpenManagedFolder}
          onManageRoots={() => navigateFromShell('settings')}
          onOpenGame={onOpenGame}
          restoreFocusGameId={libraryFocusGameId}
          onFocusRestored={onLibraryFocusRestored}
        />
      ) : gameRouteState ? (
        <GameDetailPage
          detail={gameDetail}
          gameId={currentGameId}
          onBackToLibrary={onBackToLibrary}
          onRetryReadiness={() => void refreshCatalog()}
          readinessError={catalogError}
          readinessLoading={catalogLoading}
          systemStatus={detailSystemStatus}
        />
      ) : (
        <SettingsPage
          roots={roots}
          rootsLoading={rootsLoading}
          rootsError={rootsError}
          refreshRoots={refreshRoots}
          removeExternalRoot={removeExternalRoot}
          updateRootEnabled={updateRootEnabled}
          systems={catalogSystems}
          scan={scan}
          refreshSummary={refreshSummary}
          onAddExternalFolder={onAddExternalFolder}
          onOpenManagedFolder={onOpenManagedFolder}
          onBackToLibrary={() => navigateFromShell('library')}
        />
      )}

      <footer className="app-footer">
        <span>LOCAL LIBRARY</span>
        <span aria-hidden="true">·</span>
        <span>{footerStatus}</span>
        <span className="footer-spacer" />
        <span className="footer-note">ROM files stay on your disk</span>
      </footer>
    </div>
  );
}
