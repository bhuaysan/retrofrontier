import { useEffect, useRef, useState } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import { ExternalLinkIcon, FolderIcon, PixelArrow } from '../../components/ui/PixelIcon';
import {
  normalizeIpcError,
  type ContentRoot,
  type IpcError,
  type ScanStatus,
  type ScanSummary,
} from '../../platform/ipc';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { RootActionError } from './RootActionError';
import { rootAvailabilityLabel } from '../library/rootLabels';

interface SettingsPageProps {
  roots: ContentRoot[];
  rootsLoading: boolean;
  rootsError: IpcError | null;
  refreshRoots: () => Promise<ContentRoot[] | null>;
  removeExternalRoot: (rootId: number) => Promise<void>;
  updateRootEnabled: (rootId: number, enabled: boolean) => Promise<ContentRoot>;
  systems: SystemLabel[];
  scan: {
    status: ScanStatus | null;
    scanStartPending: boolean;
    scanStartError: IpcError | null;
    startScan: () => Promise<ScanSummary | null>;
  };
  refreshSummary: () => Promise<unknown>;
  onAddExternalFolder: () => Promise<boolean>;
  onOpenManagedFolder: () => Promise<void>;
  onBackToLibrary: () => void;
}

type RootOperation =
  | { kind: 'add' }
  | { kind: 'open' }
  | { kind: 'toggle'; rootId: number; enabled: boolean }
  | { kind: 'remove'; rootId: number }
  | { kind: 'scan' };

function systemHintLabel(root: ContentRoot, systems: SystemLabel[]) {
  if (!root.systemHint) return 'AUTO-DETECT SYSTEM';
  return systems.find((system) => system.id === root.systemHint)?.displayName ?? root.systemHint;
}

function RootCard({
  root,
  systems,
  busy,
  removalPending,
  onOpen,
  onToggle,
  onStartRemoval,
  onCancelRemoval,
  onConfirmRemoval,
  removeTriggerRef,
  confirmationButtonRef,
}: {
  root: ContentRoot;
  systems: SystemLabel[];
  busy: boolean;
  removalPending: boolean;
  onOpen?: () => void;
  onToggle: () => void;
  onStartRemoval: () => void;
  onCancelRemoval: () => void;
  onConfirmRemoval: () => void;
  removeTriggerRef?: (node: HTMLButtonElement | null) => void;
  confirmationButtonRef?: (node: HTMLButtonElement | null) => void;
}) {
  const managed = root.kind === 'managed';
  const enabled = root.enabled && root.availability !== 'disabled';
  return (
    <article className={`root-card${managed ? ' root-card--managed' : ''}`}>
      <div className="root-card-heading">
        <FolderIcon className="root-icon" />
        <div>
          <h3>{managed ? 'MANAGED ROM FOLDER' : 'EXTERNAL ROM FOLDER'}</h3>
          <span>{managed ? 'APPLICATION-OWNED' : systemHintLabel(root, systems)}</span>
        </div>
        <span
          className={`root-status root-status--${root.enabled ? root.availability : 'disabled'}`}
        >
          {rootAvailabilityLabel(root)}
        </span>
      </div>
      <p className="root-path" title={root.path}>
        {root.path}
      </p>
      <div className="root-card-actions">
        {onOpen && (
          <PixelButton type="button" variant="secondary" disabled={busy} onClick={onOpen}>
            <ExternalLinkIcon />
            OPEN FOLDER
          </PixelButton>
        )}
        {managed ? (
          <span className="root-protected">Managed content is never removed by RetroFrontier.</span>
        ) : removalPending ? (
          <div
            className="root-confirm-actions"
            role="alertdialog"
            aria-modal="false"
            aria-labelledby={`remove-root-title-${root.id}`}
          >
            <span id={`remove-root-title-${root.id}`}>
              Remove this root from RetroFrontier? Files stay on disk.
            </span>
            <PixelButton
              type="button"
              variant="secondary"
              disabled={busy}
              onClick={onCancelRemoval}
            >
              CANCEL
            </PixelButton>
            <PixelButton
              ref={confirmationButtonRef}
              type="button"
              disabled={busy}
              onClick={onConfirmRemoval}
            >
              REMOVE ROOT
            </PixelButton>
          </div>
        ) : (
          <>
            <PixelButton type="button" variant="secondary" disabled={busy} onClick={onToggle}>
              {enabled ? 'DISABLE ROOT' : 'ENABLE ROOT'}
            </PixelButton>
            <PixelButton
              ref={removeTriggerRef}
              type="button"
              variant="secondary"
              disabled={busy}
              onClick={onStartRemoval}
            >
              REMOVE ROOT
            </PixelButton>
          </>
        )}
      </div>
    </article>
  );
}

export function SettingsPage({
  roots,
  rootsLoading,
  rootsError,
  refreshRoots,
  updateRootEnabled,
  removeExternalRoot,
  systems,
  scan,
  refreshSummary,
  onAddExternalFolder,
  onOpenManagedFolder,
  onBackToLibrary,
}: SettingsPageProps) {
  const [pendingOperation, setPendingOperation] = useState<RootOperation | null>(null);
  const [pendingRemoval, setPendingRemoval] = useState<number | null>(null);
  const [removalFocusTarget, setRemovalFocusTarget] = useState<
    { kind: 'trigger'; rootId: number } | { kind: 'heading' } | null
  >(null);
  const [actionError, setActionError] = useState<IpcError | null>(null);
  const [lastOperation, setLastOperation] = useState<RootOperation | null>(null);
  const addButton = useRef<HTMLButtonElement>(null);
  const rootsHeading = useRef<HTMLHeadingElement>(null);
  const confirmationButton = useRef<HTMLButtonElement>(null);
  const removeTriggers = useRef(new Map<number, HTMLButtonElement>());
  const managedRoot = roots.find((root) => root.kind === 'managed');

  useEffect(() => {
    if (pendingRemoval !== null) {
      confirmationButton.current?.focus();
      return;
    }

    if (removalFocusTarget?.kind === 'trigger') {
      removeTriggers.current.get(removalFocusTarget.rootId)?.focus();
    } else if (removalFocusTarget?.kind === 'heading') {
      rootsHeading.current?.focus();
    }
  }, [pendingRemoval, removalFocusTarget]);

  const runOperation = async (operation: RootOperation, callback: () => Promise<unknown>) => {
    setPendingOperation(operation);
    setLastOperation(operation);
    setActionError(null);
    try {
      await callback();
      if (operation.kind === 'toggle' || operation.kind === 'remove') {
        await refreshSummary();
      }
      if (operation.kind === 'remove') {
        setRemovalFocusTarget({ kind: 'heading' });
      }
    } catch (reason: unknown) {
      setActionError(normalizeIpcError(reason));
      if (operation.kind === 'remove') {
        setRemovalFocusTarget({ kind: 'trigger', rootId: operation.rootId });
      }
    } finally {
      setPendingOperation(null);
      if (operation.kind === 'add') {
        addButton.current?.focus();
      }
    }
  };

  const retryOperation = () => {
    if (!lastOperation) return;
    if (lastOperation.kind === 'add') {
      void runOperation(lastOperation, onAddExternalFolder);
    } else if (lastOperation.kind === 'open') {
      void runOperation(lastOperation, onOpenManagedFolder);
    } else if (lastOperation.kind === 'scan') {
      void runOperation(lastOperation, scan.startScan);
    } else if (lastOperation.kind === 'toggle') {
      void runOperation(lastOperation, () =>
        updateRootEnabled(lastOperation.rootId, lastOperation.enabled),
      );
    } else {
      void runOperation(lastOperation, () => removeExternalRoot(lastOperation.rootId));
    }
  };

  const runAdd = () => void runOperation({ kind: 'add' }, onAddExternalFolder);
  const runOpen = () => void runOperation({ kind: 'open' }, onOpenManagedFolder);
  const runScan = () => void runOperation({ kind: 'scan' }, scan.startScan);

  return (
    <main className="app-main settings-main" id="main-content">
      <PixelButton
        type="button"
        variant="secondary"
        className="back-button"
        onClick={onBackToLibrary}
      >
        <PixelArrow direction="left" />
        BACK TO LIBRARY
      </PixelButton>
      <div className="section-heading">
        <h1 id="settings-heading">
          <PixelArrow className="heading-arrow" />
          SETTINGS
        </h1>
        <span aria-hidden="true" />
        <span className="section-meta">LIBRARY ROOTS</span>
      </div>
      <p className="settings-intro">
        Manage the folders RetroFrontier is allowed to scan. Folder validation stays in the local
        application service, and removing a root never deletes its files.
      </p>

      {rootsError && (
        <InlineError
          title="CONTENT ROOTS UNAVAILABLE"
          message="RetroFrontier could not read the configured folders. Try again without changing any files."
          actionLabel="RETRY ROOTS"
          onAction={() => void refreshRoots()}
        />
      )}

      {actionError && (
        <RootActionError
          error={actionError}
          onAction={retryOperation}
          actionLabel={lastOperation?.kind === 'open' ? 'RETRY OPEN' : undefined}
        />
      )}
      {scan.scanStartError && (
        <InlineError
          title="SCAN COULD NOT START"
          message="RetroFrontier could not start the local scan. Check the configured folders and try again."
          actionLabel="TRY SCAN AGAIN"
          onAction={runScan}
        />
      )}

      <section className="settings-panel" aria-labelledby="roots-heading">
        <div className="panel-heading">
          <h2 id="roots-heading" ref={rootsHeading} tabIndex={-1}>
            CONTENT ROOTS
          </h2>
          <span aria-hidden="true" />
          <span className="panel-meta">{rootsLoading ? 'CHECKING' : `${roots.length} ROOTS`}</span>
        </div>
        <div className="root-list">
          {rootsLoading && roots.length === 0 && (
            <p className="loading-inline" role="status">
              READING CONTENT ROOTS…
            </p>
          )}
          {!rootsLoading && roots.length === 0 && (
            <p className="empty-inline" role="status">
              No content roots are available yet.
            </p>
          )}
          {roots.map((root) => (
            <RootCard
              key={root.id}
              root={root}
              systems={systems}
              busy={pendingOperation !== null}
              removalPending={pendingRemoval === root.id}
              onOpen={root.kind === 'managed' ? runOpen : undefined}
              removeTriggerRef={(node) => {
                if (node) {
                  removeTriggers.current.set(root.id, node);
                } else {
                  removeTriggers.current.delete(root.id);
                }
              }}
              confirmationButtonRef={
                pendingRemoval === root.id
                  ? (node) => {
                      confirmationButton.current = node;
                    }
                  : undefined
              }
              onToggle={() =>
                void runOperation({ kind: 'toggle', rootId: root.id, enabled: !root.enabled }, () =>
                  updateRootEnabled(root.id, !root.enabled),
                )
              }
              onStartRemoval={() => {
                setRemovalFocusTarget(null);
                setPendingRemoval(root.id);
              }}
              onCancelRemoval={() => {
                setRemovalFocusTarget({ kind: 'trigger', rootId: root.id });
                setPendingRemoval(null);
              }}
              onConfirmRemoval={() => {
                setPendingRemoval(null);
                void runOperation({ kind: 'remove', rootId: root.id }, () =>
                  removeExternalRoot(root.id),
                );
              }}
            />
          ))}
        </div>
        <div className="settings-panel-actions">
          <PixelButton
            ref={addButton}
            type="button"
            disabled={pendingOperation !== null}
            onClick={runAdd}
          >
            <FolderIcon />
            ADD EXTERNAL FOLDER
          </PixelButton>
          <PixelButton
            type="button"
            variant="secondary"
            disabled={pendingOperation !== null || scan.status?.running === true}
            onClick={runScan}
          >
            {scan.scanStartPending ? 'SCAN REQUESTED…' : 'RESCAN LIBRARY'}
          </PixelButton>
        </div>
        <p className="settings-scope-note">
          Metadata providers, emulator cores, display, controllers, and saves are configured in
          later milestones.
        </p>
      </section>

      {managedRoot && (
        <p className="settings-managed-note" role="status">
          Managed folder status: {rootAvailabilityLabel(managedRoot)} at{' '}
          <span title={managedRoot.path}>{managedRoot.path}</span>.
        </p>
      )}
    </main>
  );
}
