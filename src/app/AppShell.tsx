import { useCallback, useEffect, useState } from 'react';

import { PixelRow } from '../components/ui/PixelRow';
import { useContentRoots } from '../hooks/useContentRoots';
import { useLibrarySummary } from '../hooks/useLibrarySummary';
import { useScanState } from '../hooks/useScanState';
import { useSystemCatalog } from '../hooks/useSystemCatalog';
import { pickExternalContentRoot } from '../platform/folderPicker';
import { openManagedRomFolder, type SystemId } from '../platform/ipc';
import { LibraryPage } from '../features/library/LibraryPage';
import { SettingsPage } from '../features/settings/SettingsPage';
import { useRoute } from './routes';

type Theme = 'dark' | 'light';

const THEME_STORAGE_KEY = 'retrofrontier.theme';

const SYSTEM_ACCENTS: Record<SystemId, string> = {
  nes: 'var(--accent)',
  snes: 'var(--accent-2)',
  nintendo_64: 'var(--accent-3)',
  game_boy: 'var(--accent-4)',
  game_boy_color: 'var(--accent-4)',
  game_boy_advance: 'var(--accent-4)',
  mega_drive: 'var(--accent-3)',
  playstation: 'var(--accent-5)',
  sega_saturn: 'var(--accent-6)',
  sega_dreamcast: 'var(--accent-6)',
  nintendo_gamecube: 'var(--accent-2)',
};

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
  route: 'library' | 'settings';
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
}: {
  label: string;
  count: number;
  accent: string;
}) {
  return (
    <PixelRow
      label={label}
      count={count}
      accent={accent}
      disabled
      title="System filtering arrives with the library browsing milestone."
    />
  );
}

export function AppShell() {
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const { route, navigate } = useRoute();
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
  const systemCatalog = useSystemCatalog();
  const onScanCompleted = useCallback(() => {
    void refreshSummary();
  }, [refreshSummary]);
  const scan = useScanState({ onCompleted: onScanCompleted });

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

  const systemCounts = new Map(
    (summary?.systems ?? []).map((system) => [system.systemId, system.gameCount]),
  );
  const footerStatus = scan.statusError
    ? 'SCAN STATUS UNKNOWN'
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
          onClick={() => navigate('library')}
        >
          RETRO<span>FRONTIER</span>
        </button>
        <MobileRouteNav route={route} navigate={navigate} />
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
        <nav aria-labelledby="systems-heading">
          <h2 id="systems-heading" className="sidebar-label">
            // SYSTEMS
          </h2>
          <ul className="pixel-row-list">
            <PixelRow
              label="All systems"
              count={summary?.totalGames ?? 0}
              accent="var(--accent)"
              active={route === 'library'}
              onClick={() => navigate('library')}
            />
            {systemCatalog.systems.map((system) => (
              <SystemSummaryRow
                key={system.id}
                label={system.displayName}
                count={systemCounts.get(system.id) ?? 0}
                accent={SYSTEM_ACCENTS[system.id]}
              />
            ))}
          </ul>
        </nav>

        <nav className="sidebar-menu" aria-labelledby="menu-heading">
          <h2 id="menu-heading" className="sidebar-label">
            // MENU
          </h2>
          <ul className="pixel-row-list">
            <RouteRow
              label="Settings"
              active={route === 'settings'}
              onClick={() => navigate('settings')}
            />
          </ul>
        </nav>
      </aside>

      {route === 'library' ? (
        <LibraryPage
          summary={summary}
          summaryLoading={summaryLoading}
          summaryError={summaryError}
          refreshSummary={refreshSummary}
          roots={roots}
          rootsLoading={rootsLoading}
          rootsError={rootsError}
          refreshRoots={refreshRoots}
          systems={systemCatalog.systems}
          scan={scan}
          onAddExternalFolder={onAddExternalFolder}
          onOpenManagedFolder={onOpenManagedFolder}
          onManageRoots={() => navigate('settings')}
        />
      ) : (
        <SettingsPage
          roots={roots}
          rootsLoading={rootsLoading}
          rootsError={rootsError}
          refreshRoots={refreshRoots}
          removeExternalRoot={removeExternalRoot}
          updateRootEnabled={updateRootEnabled}
          systems={systemCatalog.systems}
          scan={scan}
          refreshSummary={refreshSummary}
          onAddExternalFolder={onAddExternalFolder}
          onOpenManagedFolder={onOpenManagedFolder}
          onBackToLibrary={() => navigate('library')}
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
