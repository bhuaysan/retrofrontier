import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { ControllerFooter } from '../components/ui/ControllerFooter';
import { InlineError } from '../components/ui/InlineError';
import { PixelRow } from '../components/ui/PixelRow';
import type { InputAction } from '../input/actions';
import { ownsApplicationInput } from '../input/inputOwnership';
import { FocusProvider } from '../focus/FocusProvider';
import { useFocusApi, useFocusBack, useFocusZone } from '../focus/focusContext';
import { focusNodes, focusZones } from '../focus/focusNodes';
import { useAppWindowFocus } from '../hooks/useAppWindowFocus';
import { useControllerInput } from '../hooks/useControllerInput';
import { useKeyboardInput } from '../hooks/useKeyboardInput';
import { useLaunchFocusReturn } from '../hooks/useLaunchFocusReturn';
import { useContentRoots } from '../hooks/useContentRoots';
import { useLibrarySummary } from '../hooks/useLibrarySummary';
import { useLibraryQuery } from '../hooks/useLibraryQuery';
import { useGameDetail } from '../hooks/useGameDetail';
import { useGameLaunch, type GameLaunchModel } from '../hooks/useGameLaunch';
import { useScanState } from '../hooks/useScanState';
import { useSystemCatalog } from '../hooks/useSystemCatalog';
import { pickExternalContentRoot } from '../platform/folderPicker';
import { openManagedRomFolder, type ScanSummary } from '../platform/ipc';
import { LibraryPage, ScanProgressPanel } from '../features/library/LibraryPage';
import { SettingsPage } from '../features/settings/SettingsPage';
import { systemAccent } from '../features/library/systemAccents';
import { GameDetailPage } from '../features/library/GameDetailPage';
import { gameRoute, isGameRoute, useRoute } from './routes';
import { PixelArrow } from '../components/ui/PixelIcon';

type Theme = 'dark' | 'light';

const THEME_STORAGE_KEY = 'retrofrontier.theme';

function initialTheme(): Theme {
  return window.localStorage.getItem(THEME_STORAGE_KEY) === 'light' ? 'light' : 'dark';
}

function RouteRow({
  label,
  route,
  active,
  onClick,
}: {
  label: string;
  route: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <PixelRow
      accent="var(--text-dim)"
      active={active}
      confirmLabel="OPEN"
      focusId={focusNodes.sidebarRoute(route)}
      label={label}
      onClick={onClick}
    />
  );
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
  systemId,
  count,
  accent,
  active,
  onClick,
  onConfirm,
}: {
  label: string;
  systemId: string | null;
  count: number;
  accent: string;
  active: boolean;
  onClick: () => void;
  onConfirm: () => void;
}) {
  return (
    <PixelRow
      label={label}
      count={count}
      accent={accent}
      active={active}
      activeMode="pressed"
      confirmLabel="FILTER"
      focusId={focusNodes.sidebarSystem(systemId)}
      onClick={onClick}
      onConfirm={onConfirm}
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
      <input
        autoComplete="off"
        aria-label="Search"
        id="library-search-input"
        onChange={(event) => onChange(event.target.value)}
        placeholder="Search"
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
  return (
    <FocusProvider>
      <AppShellBody />
    </FocusProvider>
  );
}

function AppShellBody() {
  const focus = useFocusApi();
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [libraryScanCompletionRunId, setLibraryScanCompletionRunId] = useState<number | null>(null);
  const { route, navigate } = useRoute();
  const isLibraryRoute = route === 'library';
  const isSettingsRoute = route === 'settings';
  const gameRouteState = isGameRoute(route) ? route : null;
  const usesPersistentSidebar = isLibraryRoute || isSettingsRoute || gameRouteState !== null;
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
  const scanRunning = scan.status?.running === true;
  const showsFooter = usesPersistentSidebar || scanRunning;
  const onFavoriteCommitted = useCallback(() => {
    void refreshSummary();
  }, [refreshSummary]);
  const library = useLibraryQuery({
    enabled: isLibraryRoute && Boolean(summary && summary.totalGames > 0),
    scanCompletionRunId: libraryScanCompletionRunId,
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

  // Launch state is application-wide: a game keeps running while the user browses elsewhere, so
  // the shell owns the hook rather than the detail screen.
  const gameLaunch = useGameLaunch();
  const windowFocused = useAppWindowFocus();
  // One authoritative ownership predicate for the whole application. It is deliberately conservative
  // about the launch transition: the backend may already have spawned RetroArch while React still
  // sees `running === null`, so ownership is released at the launch request rather than when the
  // running state arrives. See `src/input/inputOwnership.ts`.
  const ownsInput = ownsApplicationInput({
    windowFocused,
    running: gameLaunch.running,
    blocked: gameLaunch.blocked,
    pendingGameId: gameLaunch.pendingGameId,
  });
  const onInputAction = useCallback(
    (action: InputAction) => focus.dispatch(action, 'gamepad'),
    [focus],
  );
  const onKeyboardAction = useCallback(
    (action: InputAction) => focus.dispatch(action, 'keyboard'),
    [focus],
  );
  const { connected: controllerConnected, unsupported: controllerUnsupported } = useControllerInput(
    {
      enabled: ownsInput,
      onAction: onInputAction,
    },
  );
  useKeyboardInput({ enabled: ownsInput, onAction: onKeyboardAction });
  // The logical context a launch belongs to. A return after a managed game only restores the
  // captured origin while this is still the context the launch was started from.
  const launchRouteKey = gameRouteState !== null ? `game:${currentGameId}` : route;
  const routeFallbackNodeId =
    gameRouteState !== null
      ? focusNodes.detail('play')
      : isSettingsRoute
        ? focusNodes.settings('heading')
        : focusNodes.libraryHeading;
  // Transient launch state belongs to the Game Detail route that started it. Leaving that route
  // abandons the *presentation* — content options, a normalized failure, the pending surface — and
  // nothing else: the IPC request stays in flight, `pendingGameId` stays set, and input ownership
  // stays released, because RetroArch may already exist. M8 deliberately does not browser-trap Tab
  // or the pointer inside a focus scope, so this path is reachable without the semantic
  // Cancel/Dismiss action and must be handled rather than assumed away.
  const launchInteractionGameId = gameLaunch.interaction?.gameId ?? null;
  const { abandonInteraction } = gameLaunch;
  useEffect(() => {
    if (launchInteractionGameId === null) return;
    if (launchInteractionGameId === currentGameId) return;
    abandonInteraction();
  }, [abandonInteraction, currentGameId, launchInteractionGameId]);
  // A multi-step launch is one interaction: PLAY, a `contentSelectionRequired` answer, and the
  // version the user then confirms all belong to the same launch, so the origin is captured once at
  // its beginning. The launch facts are handed over so the hook can tell a continuation from a
  // resolution and never keep an origin that can no longer produce a return.
  const contentSelectionOpen = gameLaunch.interaction?.phase === 'contentSelection';
  const { beginLaunchInteraction } = useLaunchFocusReturn({
    running: gameLaunch.running,
    blocked: gameLaunch.blocked,
    pendingGameId: gameLaunch.pendingGameId,
    contentSelectionOpen,
    windowFocused,
    routeKey: typeof launchRouteKey === 'string' ? launchRouteKey : 'library',
    fallbackNodeId: routeFallbackNodeId,
  });
  // The explicit launch-focus handoff. The origin is recorded synchronously where the UI initiates
  // the launch, which is the user's actual intent, instead of sampling whichever node happens to be
  // focused once the backend reports a running process. Calling this on *every* launch is correct
  // and deliberate: `beginLaunchInteraction` itself decides whether the call starts a new
  // interaction or continues the open one, so a content-selection confirmation cannot overwrite the
  // PLAY origin with a temporary content-option identity.
  const launchWithFocusHandoff = useMemo<GameLaunchModel>(
    () => ({
      ...gameLaunch,
      launch: (gameId: number, contentUnitId?: number) => {
        beginLaunchInteraction();
        return gameLaunch.launch(gameId, contentUnitId);
      },
    }),
    [beginLaunchInteraction, gameLaunch],
  );
  // The route-scoped view handed to Game Detail. Scoping here rather than inside the screen means a
  // Game Detail route structurally *cannot* render transient launch state it does not own, even for
  // the single render between a route change and the abandonment effect above. `pendingGameId`,
  // `running`, and `blocked` stay global, because those are facts about the application, not about
  // one screen's transient surface.
  const ownsTransientLaunchUi =
    launchInteractionGameId !== null && launchInteractionGameId === currentGameId;
  const launchForGameDetail = useMemo<GameLaunchModel>(
    () => ({
      ...launchWithFocusHandoff,
      contentOptions: ownsTransientLaunchUi ? launchWithFocusHandoff.contentOptions : null,
      failure: ownsTransientLaunchUi ? launchWithFocusHandoff.failure : null,
    }),
    [launchWithFocusHandoff, ownsTransientLaunchUi],
  );
  // ---------------------------------------------------------------------------------------------
  // Library controller navigation zones
  //
  // Real DualSense qualification found directional movement crossing between the sidebar and the
  // game grid purely because a card happened to lie in the pressed direction. The two regions are
  // therefore declared as explicit zones: movement is resolved *within* the region the focused
  // element belongs to, and crossing is an explicit transition. Pointer clicks, native Tab order,
  // and text editing are untouched — a zone refuses nothing and traps nothing.
  //
  // Only the Library declares zones. Game Detail and Settings keep the reviewed M8 behaviour, where
  // the sidebar is a legitimate geometric neighbour of the screen's own controls.
  // ---------------------------------------------------------------------------------------------

  /**
   * A pending handoff from a confirmed sidebar filter into the main Library area.
   *
   * `settleVersion` is the query result the handoff must outlive. Confirming a *different* filter
   * starts a bounded query while the previous result stays rendered, so entering at its first card
   * would focus a game belonging to the filter the user just left — and that card may unmount a
   * moment later. Confirming the filter that is already active starts no query at all, so there is
   * nothing to wait for and `settleVersion` is `null`.
   */
  const [libraryMainEntry, setLibraryMainEntry] = useState<{
    id: number;
    settleVersion: number | null;
  } | null>(null);
  const libraryMainEntrySequence = useRef(0);
  // Which handoff has already been answered. It is a ref rather than a state clear because it
  // records that an imperative side effect — moving focus — has happened, and re-answering the same
  // handoff is what must be prevented; leaving the resolved request in state is harmless.
  const resolvedLibraryMainEntry = useRef(0);
  const librarySystemId = library.systemId;
  const libraryResultVersion = library.resultVersion;
  const enterLibraryMain = useCallback(
    (targetSystemId: string | null) => {
      libraryMainEntrySequence.current += 1;
      setLibraryMainEntry({
        id: libraryMainEntrySequence.current,
        settleVersion: targetSystemId === librarySystemId ? null : libraryResultVersion,
      });
    },
    [libraryResultVersion, librarySystemId],
  );

  useEffect(() => {
    if (libraryMainEntry === null) return;
    if (resolvedLibraryMainEntry.current >= libraryMainEntry.id) return;
    // The user left the Library before the handoff could resolve; it must not drag them back.
    if (!isLibraryRoute || scanRunning) {
      resolvedLibraryMainEntry.current = libraryMainEntry.id;
      return;
    }
    if (
      libraryMainEntry.settleVersion !== null &&
      library.resultVersion === libraryMainEntry.settleVersion
    ) {
      return;
    }
    if (library.initialLoading || library.refreshing || library.pageLoading) return;
    resolvedLibraryMainEntry.current = libraryMainEntry.id;
    // The honest first target of the *committed* result: the first game of the selected view, or
    // the Library heading when that view has none. No focusable element is invented for this.
    const firstGameId = library.page?.items[0]?.gameId ?? null;
    if (firstGameId === null || !focus.focusNode(focusNodes.libraryGame(firstGameId))) {
      focus.focusNode(focusNodes.libraryHeading);
    }
  }, [
    focus,
    isLibraryRoute,
    library.initialLoading,
    library.page,
    library.pageLoading,
    library.refreshing,
    library.resultVersion,
    libraryMainEntry,
    scanRunning,
  ]);

  const librarySidebarZoneRef = useFocusZone({ id: focusZones.librarySidebar });
  const libraryMainZoneRef = useFocusZone({
    id: focusZones.libraryMain,
    // `back` out of the main area is a focus transition, not a route change: the Library is the
    // root route and there is nowhere to navigate to. It returns to the sidebar entry that is
    // actually selected, so the next Up/Down starts from where the user really is.
    back: {
      label: 'SYSTEMS',
      run: () => {
        if (focus.focusNode(focusNodes.sidebarSystem(librarySystemId))) return;
        focus.focusNode(focusNodes.sidebarSystem(null));
      },
    },
  });

  // The Library is the root destination, so there is no *route* to go back to and no route-level
  // `back` is registered. The Library's main navigation zone declares its own `back` instead, which
  // is a focus transition to the sidebar rather than a navigation.
  useFocusBack(
    isLibraryRoute || scanRunning ? null : { label: 'LIBRARY', run: () => onBackToLibrary() },
  );
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
    <div className={`app-shell${scanRunning ? ' app-shell--scan' : ''}`} data-theme={theme}>
      <header className="app-header">
        <button
          type="button"
          className="wordmark"
          aria-label="Go to Library"
          onClick={() => navigateFromShell('library')}
        >
          RETRO<span>FRONTIER</span>
        </button>
        {isLibraryRoute && !scanRunning && summary && summary.totalGames > 0 ? (
          <LibrarySearch
            onChange={library.setSearchInput}
            onClear={library.clearSearch}
            value={library.searchInput}
          />
        ) : null}
        {(isLibraryRoute || isSettingsRoute || gameRouteState !== null) && !scanRunning ? (
          <MobileRouteNav
            route={isLibraryRoute ? 'library' : isSettingsRoute ? 'settings' : null}
            navigate={navigateFromShell}
          />
        ) : null}
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

      {usesPersistentSidebar && !scanRunning ? (
        <aside
          className="app-sidebar"
          aria-label="Library navigation"
          ref={isLibraryRoute ? librarySidebarZoneRef : undefined}
        >
          <section aria-labelledby="systems-heading">
            <p id="systems-heading" className="sidebar-label">
              <span aria-hidden="true" className="sidebar-prefix">
                //
              </span>{' '}
              SYSTEMS
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
                confirmLabel="FILTER"
                focusId={focusNodes.sidebarSystem(null)}
                onClick={() => {
                  library.setSystemId(null);
                  navigateFromShell('library');
                }}
                onConfirm={() => {
                  library.setSystemId(null);
                  navigateFromShell('library');
                  enterLibraryMain(null);
                }}
              />
              {catalogSystems.map((system) => (
                <SystemSummaryRow
                  key={system.id}
                  label={system.displayName}
                  systemId={system.id}
                  count={systemCounts.get(system.id) ?? 0}
                  accent={systemAccent(system.id)}
                  active={isLibraryRoute && library.systemId === system.id}
                  onClick={() => {
                    library.setSystemId(system.id);
                    navigateFromShell('library');
                  }}
                  onConfirm={() => {
                    library.setSystemId(system.id);
                    navigateFromShell('library');
                    enterLibraryMain(system.id);
                  }}
                />
              ))}
            </ul>
          </section>

          <nav className="sidebar-menu" aria-labelledby="menu-heading">
            <p id="menu-heading" className="sidebar-label">
              <span aria-hidden="true" className="sidebar-prefix">
                //
              </span>{' '}
              MENU
            </p>
            <ul className="pixel-row-list">
              <RouteRow
                label="Settings"
                route="settings"
                active={isSettingsRoute}
                onClick={() => navigateFromShell('settings')}
              />
            </ul>
          </nav>
        </aside>
      ) : null}

      {scanRunning ? (
        <main
          aria-labelledby="scan-screen-heading"
          className="app-main scan-main"
          id="main-content"
        >
          <div className="scan-screen-content">
            <div className="section-heading">
              <h1 id="scan-screen-heading">
                <PixelArrow className="heading-arrow" />
                SCAN IN PROGRESS
              </h1>
              <span aria-hidden="true" />
            </div>
            <ScanProgressPanel progress={scan.status?.progress ?? null} />
          </div>
        </main>
      ) : isLibraryRoute ? (
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
          mainZoneRef={libraryMainZoneRef}
        />
      ) : gameRouteState ? (
        <GameDetailPage
          detail={gameDetail}
          launch={launchForGameDetail}
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

      {showsFooter ? (
        <ControllerFooter
          controllerConnected={controllerConnected}
          controllerUnsupported={controllerUnsupported}
          gameRunning={gameLaunch.running !== null}
          interactive={ownsInput}
          status={footerStatus}
        />
      ) : null}
    </div>
  );
}
