import { useEffect, useState, type ReactNode } from 'react';

import { PixelButton } from '../components/ui/PixelButton';
import { PixelRow } from '../components/ui/PixelRow';
import {
  getAppInfo,
  getSystems,
  getRuntimeStatus,
  IpcError,
  type AppInfo,
  type RuntimeStatus,
  type ReadinessReason,
  type SystemId,
  type SystemStatus,
  type SystemsResponse,
} from '../platform/ipc';

type Theme = 'dark' | 'light';

const THEME_STORAGE_KEY = 'retrofrontier.theme';

const fallbackSystems: Array<{ label: string; accent: string }> = [
  { label: 'All systems', accent: 'var(--accent)' },
  { label: 'Nintendo Entertainment System', accent: 'var(--accent)' },
  { label: 'SNES', accent: 'var(--accent-2)' },
  { label: 'Nintendo 64', accent: 'var(--accent-3)' },
  { label: 'Game Boy', accent: 'var(--accent-4)' },
  { label: 'Game Boy Color', accent: 'var(--accent-4)' },
  { label: 'Game Boy Advance', accent: 'var(--accent-4)' },
  { label: 'Mega Drive', accent: 'var(--accent-3)' },
  { label: 'PlayStation', accent: 'var(--accent-5)' },
  { label: 'Sega Saturn', accent: 'var(--accent-6)' },
  { label: 'Sega Dreamcast', accent: 'var(--accent-6)' },
  { label: 'Nintendo GameCube', accent: 'var(--accent-2)' },
];

const systemAccents: Record<SystemId, string> = {
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
  const savedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
  return savedTheme === 'light' ? 'light' : 'dark';
}

function EmptyLibraryIcon() {
  return (
    <svg
      className="empty-library-icon"
      viewBox="0 0 7 6"
      shapeRendering="crispEdges"
      aria-hidden="true"
    >
      <path d="M0 0h1v1h-1zM1 0h1v1h-1zM2 0h1v1h-1zM0 1h1v1h-1zM3 1h1v1h-1zM4 1h1v1h-1zM5 1h1v1h-1zM6 1h1v1h-1zM0 2h1v1h-1zM6 2h1v1h-1zM0 3h1v1h-1zM6 3h1v1h-1zM0 4h1v1h-1zM6 4h1v1h-1zM0 5h1v1h-1zM1 5h1v1h-1zM2 5h1v1h-1zM3 5h1v1h-1zM4 5h1v1h-1zM5 5h1v1h-1zM6 5h1v1h-1z" />
    </svg>
  );
}

function StatusValue({ children, tone = 'neutral' }: { children: ReactNode; tone?: string }) {
  return <span className={`status-value status-value--${tone}`}>{children}</span>;
}

function runtimeLabel(status: RuntimeStatus | null, error: IpcError | null): string {
  if (error) return 'UNAVAILABLE';
  if (!status) return 'CHECKING';
  return runtimeStateLabel(status.state);
}

function runtimeStateLabel(state: RuntimeStatus['state']): string {
  switch (state) {
    case 'notInstalled':
      return 'NOT INSTALLED';
    case 'rollbackAvailable':
      return 'READY / ROLLBACK';
    case 'broken':
      return 'REPAIR REQUIRED';
    case 'installing':
      return 'INSTALLING';
    case 'updating':
      return 'UPDATING';
    case 'repairing':
      return 'REPAIRING';
    case 'ready':
      return 'READY';
  }
}

function coreLabel(system: SystemStatus): { text: string; tone: string } {
  if (system.core.policy.decision.kind === 'unresolved') {
    return { text: 'POLICY TBD', tone: 'warning' };
  }
  if (
    system.core.availability.runtimeState !== 'ready' &&
    system.core.availability.runtimeState !== 'rollbackAvailable'
  ) {
    return { text: 'MISSING', tone: 'warning' };
  }
  return system.core.availability.defaultCoreAvailable
    ? { text: 'READY', tone: 'good' }
    : { text: 'MISSING', tone: 'warning' };
}

function biosLabel(system: SystemStatus): { text: string; tone: string } {
  if (system.bios.policy === 'notRequired') {
    return { text: 'NOT REQUIRED', tone: 'good' };
  }
  const required = system.bios.requirements.filter((requirement) => requirement.required);
  const missing = required.filter((requirement) => requirement.state === 'missing').length;
  const invalid = required.filter((requirement) => requirement.state === 'presentInvalid').length;
  const uncovered = required.filter(
    (requirement) => requirement.state === 'notCoveredByCatalog',
  ).length;
  if (missing > 0) return { text: `${missing} REQUIRED BIOS MISSING`, tone: 'warning' };
  if (invalid > 0) return { text: `${invalid} BIOS INVALID`, tone: 'warning' };
  if (uncovered > 0) return { text: 'IDENTITY TBD', tone: 'warning' };
  if (system.bios.policy === 'optional') {
    return { text: 'OPTIONAL', tone: 'neutral' };
  }
  return { text: 'READY', tone: 'good' };
}

function reasonLabel(reason: ReadinessReason): string {
  switch (reason.kind) {
    case 'corePolicyUnresolved':
      return 'Default core decision is unresolved';
    case 'runtimeUnavailable':
      return `Managed runtime: ${runtimeStateLabel(reason.state)}`;
    case 'missingCore':
      return `Approved core missing: ${reason.coreId}`;
    case 'missingRequiredBios':
      return `Required BIOS missing: ${reason.requirementId}`;
    case 'invalidRequiredBios':
      return `Required BIOS invalid: ${reason.requirementId}`;
    case 'biosIdentityNotCovered':
      return `BIOS identity not covered: ${reason.requirementId}`;
  }
}

function SystemStatusCard({ system }: { system: SystemStatus }) {
  const core = coreLabel(system);
  const bios = biosLabel(system);
  return (
    <article className="system-card" data-system-id={system.id}>
      <div className="system-card-header">
        <span
          className="system-card-swatch"
          style={{ background: systemAccents[system.id] }}
          aria-hidden="true"
        />
        <div className="system-card-title">
          <h3>{system.displayName}</h3>
          <span>
            {system.manufacturer} · {system.aliases.slice(0, 2).join(' / ')}
          </span>
        </div>
        <StatusValue tone={system.readiness.ready ? 'good' : 'warning'}>
          {system.readiness.ready ? 'READY' : 'NOT READY'}
        </StatusValue>
      </div>

      <div className="system-card-checks">
        <div className="system-check-row">
          <span className="system-check-label">CORE</span>
          <StatusValue tone={core.tone}>{core.text}</StatusValue>
        </div>
        <div className="system-check-row">
          <span className="system-check-label">BIOS</span>
          <StatusValue tone={bios.tone}>{bios.text}</StatusValue>
        </div>
      </div>

      {system.readiness.reasons.length > 0 && (
        <ul className="system-card-reasons" aria-label={`${system.displayName} readiness reasons`}>
          {system.readiness.reasons.map((reason) => (
            <li
              key={`${reason.kind}-${reason.kind === 'corePolicyUnresolved' ? 'policy' : reason.kind === 'runtimeUnavailable' ? reason.state : reason.kind === 'missingCore' ? reason.coreId : reason.requirementId}`}
            >
              {reasonLabel(reason)}
            </li>
          ))}
        </ul>
      )}
    </article>
  );
}

export function AppShell() {
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus | null>(null);
  const [runtimeError, setRuntimeError] = useState<IpcError | null>(null);
  const [systemsResponse, setSystemsResponse] = useState<SystemsResponse | null>(null);
  const [systemsError, setSystemsError] = useState<IpcError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  }, [theme]);

  useEffect(() => {
    let cancelled = false;

    getAppInfo()
      .then((info) => {
        if (!cancelled) {
          setAppInfo(info);
          setError(null);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setError(
            reason instanceof IpcError
              ? reason
              : new IpcError('ipc_unavailable', 'The native foundation is unavailable.'),
          );
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    getSystems()
      .then((response) => {
        if (!cancelled) {
          setSystemsResponse(response);
          setSystemsError(null);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setSystemsError(
            reason instanceof IpcError
              ? reason
              : new IpcError('systems_unavailable', 'Supported systems are unavailable.'),
          );
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    getRuntimeStatus()
      .then((status) => {
        if (!cancelled) {
          setRuntimeStatus(status);
          setRuntimeError(null);
        }
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setRuntimeError(
            reason instanceof IpcError
              ? reason
              : new IpcError('runtime_unavailable', 'The managed runtime is unavailable.'),
          );
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const ipcTone = appInfo ? 'good' : error ? 'warning' : 'neutral';
  const databaseTone = appInfo?.databaseReady ? 'good' : error ? 'warning' : 'neutral';
  const runtimeTone = runtimeError
    ? 'warning'
    : runtimeStatus?.state === 'broken' || runtimeStatus?.state === 'notInstalled'
      ? 'warning'
      : runtimeStatus
        ? 'good'
        : 'neutral';
  const runtimeText = runtimeLabel(runtimeStatus, runtimeError);

  const showFoundationNotice = (message: string) => {
    setNotice(message);
  };

  const sidebarSystems = systemsResponse
    ? [
        { label: 'All systems', accent: 'var(--accent)' },
        ...systemsResponse.systems.map((system) => ({
          label: system.displayName,
          accent: systemAccents[system.id],
        })),
      ]
    : fallbackSystems;

  return (
    <div className="app-shell" data-theme={theme}>
      <header className="app-header">
        <div className="wordmark" aria-label="RetroFrontier">
          RETRO<span>FRONTIER</span>
        </div>
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
        <section className="sidebar-section" aria-labelledby="systems-heading">
          <h2 id="systems-heading" className="sidebar-label">
            // SYSTEMS
          </h2>
          <ul className="pixel-row-list">
            {sidebarSystems.map((system, index) => (
              <PixelRow
                key={system.label}
                label={system.label}
                count={0}
                accent={system.accent}
                active={index === 0}
                onClick={() =>
                  showFoundationNotice(
                    `${system.label} will be available with the library milestone.`,
                  )
                }
              />
            ))}
          </ul>
        </section>

        <section className="sidebar-section" aria-labelledby="menu-heading">
          <h2 id="menu-heading" className="sidebar-label">
            // MENU
          </h2>
          <ul className="pixel-row-list">
            <PixelRow
              label="Settings"
              count={0}
              accent="var(--text-dim)"
              onClick={() =>
                showFoundationNotice('Settings screens arrive after the application foundation.')
              }
            />
          </ul>
        </section>
      </aside>

      <main className="app-main">
        <div className="section-heading">
          <h1>▶ SYSTEMS</h1>
          <span aria-hidden="true" />
          <span className="section-meta">M3</span>
        </div>

        <section className="systems-panel" aria-labelledby="systems-panel-heading">
          <div className="panel-heading">
            <h2 id="systems-panel-heading">SUPPORTED SYSTEMS</h2>
            <span aria-hidden="true" />
            <span className="panel-meta">
              {systemsResponse ? `${systemsResponse.systems.length} SYSTEMS` : 'CHECKING'}
            </span>
          </div>
          <p className="systems-intro">
            Static platform policy lives in RetroFrontier. Core availability comes only from the
            verified managed runtime; BIOS files stay in your user-owned Documents folder.
          </p>
          <div className="systems-root-row">
            <span className="system-check-label">BIOS ROOT</span>
            <span className="systems-root-path">Documents / RetroFrontier / BIOS</span>
            {systemsResponse && (
              <StatusValue tone={systemsResponse.biosRootStatus === 'ready' ? 'good' : 'warning'}>
                {systemsResponse.biosRootStatus === 'ready' ? 'AVAILABLE' : 'NOT AVAILABLE'}
              </StatusValue>
            )}
          </div>
          {!systemsResponse && !systemsError && (
            <p className="systems-loading" role="status">
              READING SYSTEM CATALOG…
            </p>
          )}
          {systemsError && (
            <p className="foundation-error" role="alert">
              {systemsError.message}
            </p>
          )}
          {systemsResponse && (
            <div className="system-grid">
              {systemsResponse.systems.map((system) => (
                <SystemStatusCard key={system.id} system={system} />
              ))}
            </div>
          )}
        </section>

        <section className="empty-state" aria-labelledby="empty-title">
          <EmptyLibraryIcon />
          <div className="empty-copy">
            <h2 id="empty-title">LIBRARY IS EMPTY</h2>
            <p>No games found yet. Scan a folder to start building your library.</p>
          </div>
          <PixelButton
            type="button"
            onClick={() =>
              showFoundationNotice('Folder scanning is reserved for the library milestone.')
            }
          >
            <span aria-hidden="true">▣</span>
            SCAN A FOLDER
          </PixelButton>
        </section>

        <section className="foundation-panel" aria-labelledby="foundation-heading">
          <div className="panel-heading">
            <h2 id="foundation-heading">FOUNDATION STATUS</h2>
            <span aria-hidden="true" />
            <span className="panel-meta">M3</span>
          </div>
          <div className="status-grid">
            <div className="status-item">
              <span className="status-label">IPC</span>
              <StatusValue tone={ipcTone}>
                {appInfo ? 'CONNECTED' : error ? 'UNAVAILABLE' : 'CONNECTING'}
              </StatusValue>
            </div>
            <div className="status-item">
              <span className="status-label">DATABASE</span>
              <StatusValue tone={databaseTone}>
                {appInfo?.databaseReady ? 'READY' : error ? 'UNKNOWN' : 'STARTING'}
              </StatusValue>
            </div>
            <div className="status-item">
              <span className="status-label">APP</span>
              <StatusValue>
                {appInfo ? `${appInfo.appName} ${appInfo.version}` : 'LOADING'}
              </StatusValue>
            </div>
            <div className="status-item">
              <span className="status-label">HOST</span>
              <StatusValue>
                {appInfo ? `${appInfo.platform} / ${appInfo.architecture}` : 'DETECTING'}
              </StatusValue>
            </div>
            <div className="status-item">
              <span className="status-label">RUNTIME</span>
              <StatusValue tone={runtimeTone}>{runtimeText}</StatusValue>
            </div>
          </div>
          {error && (
            <p className="foundation-error" role="alert">
              {error.message}
            </p>
          )}
          {notice && (
            <p className="foundation-notice" role="status">
              {notice}
            </p>
          )}
        </section>
      </main>

      <footer className="app-footer">
        <span>RUNTIME: {runtimeText}</span>
        <span aria-hidden="true">·</span>
        <span>DATABASE: {appInfo?.databaseReady ? 'READY' : 'LOCAL'}</span>
        <span className="footer-spacer" />
        <span className="footer-key">A</span>
        <span>FOUNDATION STATUS</span>
      </footer>
    </div>
  );
}
