import { useEffect, useRef, useState } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { useFocusApi, useFocusNode } from '../../focus/focusContext';
import { focusNodes } from '../../focus/focusNodes';
import { PixelButton } from '../../components/ui/PixelButton';
import {
  ExternalLinkIcon,
  FolderIcon,
  LibraryIcon,
  PixelArrow,
} from '../../components/ui/PixelIcon';
import {
  normalizeIpcError,
  type ContentRoot,
  type IpcError,
  type LibrarySummary,
  type ScanIssue,
  type ScanProgress,
  type ScanSummary,
} from '../../platform/ipc';
import type { LibraryQueryModel } from '../../hooks/useLibraryQuery';
import { useLibrarySelection } from '../../hooks/useLibrarySelection';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import type { ScanStateModel } from '../../hooks/useScanState';
import { RootActionError } from '../settings/RootActionError';
import { LibraryBrowser } from './LibraryBrowser';
import { LibraryFilterBar } from './LibraryFilterBar';
import { LibrarySelectionBar } from './LibrarySelectionBar';
import { rootAvailabilityLabel } from './rootLabels';

interface LibraryPageProps {
  summary: LibrarySummary | null;
  summaryLoading: boolean;
  summaryError: IpcError | null;
  refreshSummary: () => Promise<LibrarySummary | null>;
  roots: ContentRoot[];
  rootsLoading: boolean;
  rootsError: IpcError | null;
  refreshRoots: () => Promise<ContentRoot[] | null>;
  systems: SystemLabel[];
  library: LibraryQueryModel;
  scan: ScanStateModel;
  onAddExternalFolder: () => Promise<boolean>;
  onOpenManagedFolder: () => Promise<void>;
  onManageRoots: () => void;
  onOpenGame: (gameId: number) => void;
  restoreFocusGameId: number | null;
  onFocusRestored: () => void;
  /**
   * Declares this screen's main content as the Library's main controller navigation zone. It is
   * owned by the shell, because the sidebar is the other half of the same boundary.
   */
  mainZoneRef?: (element: HTMLElement | null) => void;
}

const ISSUE_COPY: Record<ScanIssue['kind'], { label: string; description: string }> = {
  rootUnavailable: {
    label: 'Root unavailable',
    description: 'A content root could not be reached.',
  },
  unreadablePath: { label: 'Path unreadable', description: 'A folder or file could not be read.' },
  unsafePath: { label: 'Unsafe path', description: 'A path was rejected for safety.' },
  unrepresentablePath: {
    label: 'Path not representable',
    description: 'A path could not be represented safely.',
  },
  unsupportedSystem: {
    label: 'Unknown system',
    description: 'The file did not match a supported system.',
  },
  ambiguousSystem: {
    label: 'Ambiguous system',
    description: 'The file could belong to more than one system.',
  },
  incompatibleSystemHint: {
    label: 'System hint mismatch',
    description: 'The root hint does not match the content.',
  },
  malformedCue: { label: 'Malformed CUE', description: 'The CUE descriptor could not be parsed.' },
  malformedGdi: { label: 'Malformed GDI', description: 'The GDI descriptor could not be parsed.' },
  malformedM3u: {
    label: 'Malformed playlist',
    description: 'The M3U playlist could not be parsed.',
  },
  unsafeDescriptorReference: {
    label: 'Unsafe reference',
    description: 'A descriptor referenced an unsafe path.',
  },
  missingReferencedFile: {
    label: 'Missing referenced file',
    description: 'A descriptor referenced a file that was not found.',
  },
  referenceCycle: {
    label: 'Reference cycle',
    description: 'Descriptors referenced one another in a cycle.',
  },
  hashReadFailure: { label: 'Hashing failed', description: 'A file could not be hashed.' },
  duplicateContent: {
    label: 'Duplicate content',
    description: 'Matching content was found more than once.',
  },
  ambiguousReconciliation: {
    label: 'Ambiguous reconciliation',
    description: 'Existing content could not be matched unambiguously.',
  },
  overlappingContentRoot: {
    label: 'Overlapping root',
    description: 'A root overlaps another enabled root.',
  },
  watcherFailure: {
    label: 'Watcher unavailable',
    description: 'Automatic folder watching reported a problem.',
  },
};

function number(value: number) {
  return value.toLocaleString('en-US');
}

function formatDuration(durationMs: number) {
  return `${Math.round(durationMs / 1000)}s`;
}

function SectionHeading({ id, title, meta }: { id: string; title: string; meta: string }) {
  // The heading is a programmatic focus target, not a navigation candidate: it is where focus lands
  // when the game a return was requested for no longer exists.
  const headingRef = useFocusNode({ id: focusNodes.libraryHeading });
  return (
    <div className="section-heading">
      <h1 id={id} ref={headingRef} tabIndex={-1}>
        <PixelArrow className="heading-arrow" />
        {title}
      </h1>
      <span aria-hidden="true" />
      <span className="section-meta">{meta}</span>
    </div>
  );
}

interface ActionRunnerProps {
  onAddExternalFolder: () => Promise<boolean>;
  onOpenManagedFolder: () => Promise<void>;
  onStartScan: () => Promise<ScanSummary | null>;
  onManageRoots: () => void;
  managedRoot: ContentRoot | undefined;
  rootsLoading: boolean;
  rootsError: IpcError | null;
  refreshRoots: () => Promise<ContentRoot[] | null>;
  scanRunning: boolean;
}

function EmptyLibraryState({
  onAddExternalFolder,
  onOpenManagedFolder,
  onStartScan,
  onManageRoots,
  managedRoot,
  rootsLoading,
  rootsError,
  refreshRoots,
  scanRunning,
}: ActionRunnerProps) {
  const [pendingAction, setPendingAction] = useState<'add' | 'open' | 'scan' | null>(null);
  const [actionError, setActionError] = useState<IpcError | null>(null);
  const [lastAction, setLastAction] = useState<'add' | 'open' | 'scan' | null>(null);
  const addButton = useRef<HTMLButtonElement>(null);

  const runAction = async (action: 'add' | 'open' | 'scan', callback: () => Promise<unknown>) => {
    setPendingAction(action);
    setLastAction(action);
    setActionError(null);
    try {
      await callback();
    } catch (reason: unknown) {
      setActionError(normalizeIpcError(reason));
    } finally {
      setPendingAction(null);
      if (action === 'add') {
        addButton.current?.focus();
      }
    }
  };

  const retryAction = () => {
    if (actionError?.code === 'content_root_invalid_operation') {
      onManageRoots();
    } else if (lastAction === 'add') {
      void runAction('add', onAddExternalFolder);
    } else if (lastAction === 'open') {
      void runAction('open', onOpenManagedFolder);
    } else if (lastAction === 'scan') {
      void runAction('scan', onStartScan);
    }
  };

  return (
    <section className="library-empty" aria-labelledby="empty-library-title">
      <div className="empty-hero">
        <LibraryIcon className="empty-library-icon" />
        <div className="empty-copy">
          <h2 id="empty-library-title">LIBRARY IS EMPTY</h2>
          <p>No games have been found yet. Scan a folder to start building your library.</p>
        </div>
        <div className="empty-actions">
          <PixelButton
            type="button"
            disabled={pendingAction !== null || scanRunning}
            onClick={() => void runAction('scan', onStartScan)}
          >
            <FolderIcon />
            SCAN MANAGED FOLDER
          </PixelButton>
          <PixelButton
            ref={addButton}
            type="button"
            variant="secondary"
            disabled={pendingAction !== null}
            onClick={() => void runAction('add', onAddExternalFolder)}
          >
            ADD EXTERNAL FOLDER
          </PixelButton>
        </div>
        {pendingAction && (
          <p className="action-progress" role="status">
            {pendingAction === 'scan'
              ? 'SCAN REQUESTED…'
              : pendingAction === 'add'
                ? 'OPENING FOLDER PICKER…'
                : 'OPENING MANAGED FOLDER…'}
          </p>
        )}
      </div>

      {actionError && (
        <RootActionError
          error={actionError}
          onAction={retryAction}
          actionLabel={lastAction === 'open' ? 'RETRY OPEN' : undefined}
        />
      )}

      <section className="root-overview" aria-labelledby="managed-root-heading">
        <div className="panel-heading">
          <h2 id="managed-root-heading">MANAGED ROM FOLDER</h2>
          <span aria-hidden="true" />
          <span className="panel-meta">APPLICATION-OWNED</span>
        </div>
        <div className="root-overview-row">
          <FolderIcon className="root-icon" />
          <div className="root-overview-copy">
            <strong>
              {managedRoot
                ? rootAvailabilityLabel(managedRoot)
                : rootsLoading
                  ? 'CHECKING'
                  : 'NOT AVAILABLE'}
            </strong>
            <span className="root-path" title={managedRoot?.path}>
              {managedRoot?.path ??
                (rootsLoading ? 'Reading managed location…' : 'Managed location unavailable')}
            </span>
          </div>
          <PixelButton
            type="button"
            variant="secondary"
            disabled={!managedRoot || pendingAction !== null}
            onClick={() => void runAction('open', onOpenManagedFolder)}
          >
            <ExternalLinkIcon />
            OPEN FOLDER
          </PixelButton>
        </div>
        <div className="root-overview-footer">
          <span>
            ROM files stay where you put them. RetroFrontier only records this location for
            scanning.
          </span>
          <button type="button" className="text-link" onClick={onManageRoots}>
            MANAGE ALL ROOTS
          </button>
        </div>
      </section>

      {rootsError && (
        <InlineError
          title="CONTENT ROOTS UNAVAILABLE"
          message="RetroFrontier could not read the configured folders. The library can still be opened; try again when ready."
          actionLabel="RETRY ROOTS"
          onAction={() => void refreshRoots()}
        />
      )}
    </section>
  );
}

export function ScanProgressPanel({ progress }: { progress: ScanProgress | null }) {
  const counters = progress?.counters;
  const determinate = Boolean(
    progress && progress.phase !== 'discovery' && counters && counters.filesDiscovered > 0,
  );
  const percentage =
    determinate && counters
      ? Math.min(100, Math.round((counters.filesProcessed / counters.filesDiscovered) * 100))
      : undefined;
  const phaseLabel = progress ? scanPhaseLabel(progress.phase) : 'Waiting for the scanner…';

  return (
    <section className="scan-panel scan-panel--active" aria-labelledby="scan-active-heading">
      <div className="panel-heading">
        <h2 id="scan-active-heading">
          <span className="status-pulse" aria-hidden="true" />
          LIVE PROGRESS
        </h2>
        <span aria-hidden="true" />
        <span className="panel-meta">{progress ? `RUN #${progress.runId}` : 'STARTING'}</span>
      </div>
      <div className="scan-phase-row">
        <strong role="status" aria-live="polite">
          {phaseLabel}
        </strong>
        <span aria-hidden="true">
          {progress ? `${number(progress.counters.filesProcessed)} PROCESSED` : 'NO COUNTERS YET'}
        </span>
      </div>
      {determinate ? (
        <progress
          className="scan-progress"
          max={100}
          value={percentage}
          aria-label="Library scan progress"
        />
      ) : (
        <div
          className="scan-progress scan-progress--indeterminate"
          role="progressbar"
          aria-label="Library scan progress"
        >
          <span />
        </div>
      )}
      <div className="scan-counter-grid">
        <div>
          <span>FILES FOUND</span>
          <strong>{number(counters?.filesDiscovered ?? 0)}</strong>
        </div>
        <div>
          <span>FILES PROCESSED</span>
          <strong>{number(counters?.filesProcessed ?? 0)}</strong>
        </div>
        <div>
          <span>FILES HASHED</span>
          <strong>{number(counters?.filesHashed ?? 0)}</strong>
        </div>
        <div>
          <span>ISSUES</span>
          <strong>{number(counters?.issuesFound ?? 0)}</strong>
        </div>
      </div>
      <p className="scan-truth-note">
        {determinate
          ? `${percentage}% reflects processed files against the files discovered so far.`
          : 'The scanner is still discovering content; total progress is not known yet.'}
      </p>
    </section>
  );
}

function ScanResultPanel({ result, totalGames }: { result: ScanSummary; totalGames: number }) {
  const failed = result.state === 'failed';
  return (
    <section
      className={`scan-panel scan-panel--result${failed ? ' scan-panel--failed' : ''}`}
      aria-labelledby="scan-result-heading"
      role="status"
      aria-live="polite"
    >
      <div className="panel-heading">
        <h2 id="scan-result-heading">{failed ? 'SCAN FINISHED WITH ERRORS' : 'SCAN COMPLETE'}</h2>
        <span aria-hidden="true" />
        <span className="panel-meta">RUN #{result.runId}</span>
      </div>
      <div className="scan-result-summary">
        <strong>
          {number(totalGames)} {totalGames === 1 ? 'GAME' : 'GAMES'} IN LIBRARY
        </strong>
        <span>
          {formatDuration(result.durationMs)} · {number(result.counters.issuesFound)}{' '}
          {result.counters.issuesFound === 1 ? 'ISSUE' : 'ISSUES'}
        </span>
      </div>
      <p>
        {failed
          ? 'The scanner could not finish this run. Existing library content remains available; try scanning again.'
          : result.counters.issuesFound > 0
            ? 'The scan finished. Some content needs your attention below.'
            : 'The scan finished without recorded issues. Your library is ready.'}
      </p>
    </section>
  );
}

function ScanIssuesPanel({ scan, roots }: { scan: ScanStateModel; roots: ContentRoot[] }) {
  const page = scan.issuePage;
  const showPreviousRunContext = Boolean(scan.status?.running && page?.scanRunId !== null);
  const currentRunId = scan.status?.progress?.runId;
  const hasMore = Boolean(page && page.issues.length < page.total);

  if (scan.issueError && !page) {
    return (
      <InlineError
        title="SCAN ISSUES UNAVAILABLE"
        message="The saved issue list could not be loaded. Your library and scanner are still available."
        actionLabel="RETRY ISSUES"
        onAction={() => void scan.refreshIssues()}
      />
    );
  }

  if (!page) {
    if (scan.issueLoading) {
      return (
        <section className="scan-issues" aria-labelledby="scan-issues-heading">
          <div className="panel-heading">
            <h2 id="scan-issues-heading">SCAN ISSUES</h2>
            <span aria-hidden="true" />
            <span className="panel-meta">CHECKING</span>
          </div>
          <p className="loading-inline" role="status" aria-live="polite">
            READING SAVED ISSUES…
          </p>
        </section>
      );
    }
    return null;
  }

  return (
    <section className="scan-issues" aria-labelledby="scan-issues-heading">
      <div className="panel-heading">
        <h2 id="scan-issues-heading">SCAN ISSUES</h2>
        <span aria-hidden="true" />
        <span className="panel-meta">{number(page.total)} TOTAL</span>
      </div>
      {showPreviousRunContext && (
        <p className="issue-context" role="status">
          Showing saved issues from terminal run #{page.scanRunId}. The current scan
          {currentRunId ? ` #${currentRunId}` : ''} is still running.
        </p>
      )}
      {!scan.status?.running && page.scanRunId !== null && (
        <p className="issue-context">Saved issues from terminal run #{page.scanRunId}.</p>
      )}
      {scan.issueError && (
        <InlineError
          title="SAVED ISSUES REFRESH FAILED"
          message="The saved issue list could not be refreshed. The issues already shown are unchanged."
          actionLabel="RETRY ISSUES"
          onAction={() => void scan.refreshIssues()}
        />
      )}
      {scan.issueLoadMoreError && (
        <InlineError
          title="MORE ISSUES UNAVAILABLE"
          message="The next saved issue page could not be loaded. The issues already shown are unchanged."
          actionLabel="RETRY PAGE"
          onAction={() => void scan.loadMoreIssues()}
        />
      )}
      {page.issues.length === 0 ? (
        <p className="issue-empty" role="status">
          No persisted scan issues were recorded for the latest terminal run.
        </p>
      ) : (
        <ul className="issue-list">
          {page.issues.map((issue, index) => (
            <ScanIssueRow
              key={issue.id ?? `${issue.kind}-${issue.relativePath ?? 'root'}-${index}`}
              issue={issue}
              roots={roots}
            />
          ))}
        </ul>
      )}
      {hasMore && (
        <PixelButton
          type="button"
          variant="secondary"
          disabled={scan.issueLoadingMore}
          onClick={() => void scan.loadMoreIssues()}
        >
          {scan.issueLoadingMore
            ? 'LOADING ISSUES…'
            : `LOAD MORE ISSUES (${number(page.total - page.issues.length)} LEFT)`}
        </PixelButton>
      )}
      {scan.issueLoading && page.issues.length > 0 && (
        <p className="issue-refresh" role="status">
          Refreshing saved issues…
        </p>
      )}
    </section>
  );
}

function ScanIssueRow({ issue, roots }: { issue: ScanIssue; roots: ContentRoot[] }) {
  const copy = ISSUE_COPY[issue.kind] ?? {
    label: 'Scan issue',
    description: 'The scanner recorded an issue that this version cannot classify yet.',
  };
  const root =
    issue.rootId === null ? undefined : roots.find((candidate) => candidate.id === issue.rootId);
  return (
    <li className="issue-row">
      <div className="issue-row-heading">
        <strong>{copy.label}</strong>
        <span>
          {root ? root.path : issue.rootId === null ? 'Library-wide' : `Root #${issue.rootId}`}
        </span>
      </div>
      <p>{copy.description}</p>
      {issue.detail && <p className="issue-detail">{issue.detail}</p>}
      {(issue.relativePath || issue.relatedPath) && (
        <div className="issue-paths">
          {issue.relativePath && <code>{issue.relativePath}</code>}
          {issue.relatedPath && <code>related: {issue.relatedPath}</code>}
        </div>
      )}
    </li>
  );
}

function scanPhaseLabel(phase: ScanProgress['phase']) {
  switch (phase) {
    case 'discovery':
      return 'Discovering folders and files';
    case 'relationshipResolution':
      return 'Resolving multi-file content';
    case 'hashing':
      return 'Hashing content';
    case 'reconciliation':
      return 'Updating the library';
    case 'completed':
      return 'Finalizing scan';
  }
}

export function LibraryPage({
  summary,
  summaryLoading,
  summaryError,
  refreshSummary,
  roots,
  rootsLoading,
  rootsError,
  refreshRoots,
  systems,
  library,
  scan,
  onAddExternalFolder,
  onOpenManagedFolder,
  onManageRoots,
  onOpenGame,
  restoreFocusGameId,
  onFocusRestored,
  mainZoneRef,
}: LibraryPageProps) {
  // B1 multi-select is transient presentation state owned by the Library composition, so the
  // selection bar, the count, and every card's selected state all read one authority.
  const focus = useFocusApi();
  const selection = useLibrarySelection(library.page);
  const managedRoot = roots.find((root) => root.kind === 'managed');
  const isRunning = scan.status?.running === true;
  const lastResult = scan.status?.lastResult;
  const issuePage = scan.issuePage;
  const populated = summary !== null && summary.totalGames > 0;
  const terminalResult = !isRunning && lastResult ? lastResult : null;
  // A failure, or a run that recorded issues, still needs the prominent panel next to its workflow.
  // Everything else about a healthy populated library belongs behind the grid.
  const resultNeedsAttention =
    terminalResult !== null &&
    (terminalResult.state === 'failed' || terminalResult.counters.issuesFound > 0);
  const showResultPanel = terminalResult !== null && (resultNeedsAttention || !populated);
  // An empty issue page is not a diagnostic worth a panel; loading and error states still are.
  const showIssues = Boolean(
    scan.issueError ||
    (issuePage && issuePage.total > 0) ||
    (scan.issueLoading && lastResult && !issuePage),
  );

  // Library return focus. The request names the originating game by its stable `GameId`; it is not
  // a DOM query and it is never resolved against a stale page. It stays pending until this screen
  // reports that its bounded query settled, at which point the card is focused if it is really
  // present and the Library heading is used otherwise.
  const settleVersion = useRef<number | null>(null);
  useEffect(() => {
    if (restoreFocusGameId === null) return;
    focus.requestFocus(focusNodes.libraryGame(restoreFocusGameId), {
      awaitSettle: true,
      fallback: focusNodes.libraryHeading,
    });
    settleVersion.current = library.resultVersion;
    onFocusRestored();
  }, [focus, library.resultVersion, onFocusRestored, restoreFocusGameId]);

  useEffect(() => {
    if (settleVersion.current === null) return;
    if (library.resultVersion === settleVersion.current) return;
    if (library.pageLoading || library.refreshing || library.initialLoading) return;
    settleVersion.current = null;
    focus.settleFocusRequest();
  }, [
    focus,
    library.initialLoading,
    library.pageLoading,
    library.refreshing,
    library.resultVersion,
  ]);

  return (
    <main
      aria-labelledby="library-heading"
      className="app-main"
      id="main-content"
      ref={mainZoneRef}
    >
      {populated && <LibraryFilterBar library={library} systems={systems} />}
      {populated && selection.count > 0 ? (
        <LibrarySelectionBar count={selection.count} onClear={selection.clear} />
      ) : null}
      <SectionHeading
        id="library-heading"
        title="LIBRARY"
        meta={
          summary
            ? `${number(summary.totalGames)} ${summary.totalGames === 1 ? 'GAME' : 'GAMES'}`
            : 'CHECKING'
        }
      />

      {summaryLoading && !summary && (
        <section className="loading-panel" role="status" aria-live="polite">
          <span className="loading-block" />
          <span>READING LIBRARY SUMMARY…</span>
        </section>
      )}

      {summaryError && (
        <InlineError
          title={summary ? 'LIBRARY SUMMARY REFRESH FAILED' : 'LIBRARY SUMMARY UNAVAILABLE'}
          message={
            summary
              ? 'The displayed summary may be out of date. No full library data was loaded; try again.'
              : 'RetroFrontier could not read the bounded library summary. No full library data was loaded.'
          }
          actionLabel="RETRY SUMMARY"
          onAction={() => void refreshSummary()}
        />
      )}

      {summary && (
        <>
          {scan.statusError && (
            <InlineError
              title="SCAN STATUS UNAVAILABLE"
              message="Live scan status could not be read. The library summary remains available; try again before scanning."
              actionLabel="RETRY SCAN STATUS"
              onAction={() => void scan.refreshStatus()}
            />
          )}
          {scan.scanStartError && (
            <InlineError
              title="SCAN COULD NOT START"
              message="RetroFrontier could not start the local scan. Check the configured folders and try again."
              actionLabel="TRY SCAN AGAIN"
              onAction={() => void scan.startScan()}
            />
          )}
          {scan.statusLoading && !scan.status && (
            <p className="request-status" role="status" aria-live="polite">
              CHECKING SCAN STATUS…
            </p>
          )}
          {scan.scanStartPending && !isRunning && (
            <p className="request-status" role="status" aria-live="polite">
              SENDING SCAN REQUEST…
            </p>
          )}
          {showResultPanel && terminalResult && (
            <ScanResultPanel result={terminalResult} totalGames={summary.totalGames} />
          )}

          {populated &&
            terminalResult?.state === 'completed' &&
            terminalResult.counters.issuesFound === 0 && (
              <p aria-atomic="true" aria-live="polite" className="visually-hidden" role="status">
                Scan finished successfully. Library refreshed: {number(summary.totalGames)}{' '}
                {summary.totalGames === 1 ? 'GAME' : 'GAMES'} AVAILABLE;{' '}
                {number(terminalResult.counters.issuesFound)} ISSUES.
              </p>
            )}

          {summary.totalGames === 0 ? (
            <EmptyLibraryState
              onAddExternalFolder={onAddExternalFolder}
              onOpenManagedFolder={onOpenManagedFolder}
              onStartScan={scan.startScan}
              onManageRoots={onManageRoots}
              managedRoot={managedRoot}
              rootsLoading={rootsLoading}
              rootsError={rootsError}
              refreshRoots={refreshRoots}
              scanRunning={isRunning}
            />
          ) : (
            <LibraryBrowser
              library={library}
              onOpenGame={onOpenGame}
              selection={selection}
              systems={systems}
            />
          )}

          {showIssues && <ScanIssuesPanel scan={scan} roots={roots} />}
        </>
      )}
    </main>
  );
}
