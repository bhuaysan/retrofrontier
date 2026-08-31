import { useEffect, useRef, useState, type FormEvent } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { useFocusNode, useFocusScope } from '../../focus/focusContext';
import { focusNodes, focusScopes } from '../../focus/focusNodes';
import { PixelButton } from '../../components/ui/PixelButton';
import { ExternalLinkIcon, FolderIcon, PixelArrow } from '../../components/ui/PixelIcon';
import { useManagedRuntime } from '../../hooks/useManagedRuntime';
import { useMetadataProvider, type MetadataProviderModel } from '../../hooks/useMetadataProvider';
import {
  normalizeIpcError,
  type ContentRoot,
  type IpcError,
  type ScanStatus,
  type ScanSummary,
} from '../../platform/ipc';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { RootActionError } from './RootActionError';
import { RuntimePanel } from './RuntimePanel';
import {
  accountStatusCopy,
  providerDeferralCopy,
  hasActiveProviderDeferral,
  providerStatusCopy,
  quotaSummary,
} from './metadataStatus';
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
  // The confirmation owns focus while it is open: navigation stays inside it and `back` cancels.
  // Entry and exit focus stay with this screen's existing, already-verified behaviour.
  const removalScopeRef = useFocusScope({
    id: focusScopes.rootRemoval(root.id),
    dismissLabel: 'CANCEL',
    initialFocus: 'none',
    restore: 'none',
    onDismiss: () => {
      if (!busy) onCancelRemoval();
    },
  });
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
            ref={removalScopeRef}
            role="alertdialog"
            aria-modal="false"
            aria-labelledby={`remove-root-title-${root.id}`}
            onKeyDown={(event) => {
              if (event.key === 'Escape' && !busy) {
                event.preventDefault();
                onCancelRemoval();
              }
            }}
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

function MetadataProviderPanel({ provider }: { provider: MetadataProviderModel }) {
  const [confirmingClear, setConfirmingClear] = useState(false);
  const clearTrigger = useRef<HTMLButtonElement>(null);
  const clearConfirmation = useRef<HTMLButtonElement>(null);
  const accountHeading = useRef<HTMLHeadingElement>(null);
  const focusClearTrigger = useRef(false);
  const status = provider.providerStatus;
  const [now, setNow] = useState(() => Date.now());
  const clearScopeRef = useFocusScope({
    id: focusScopes.metadataAccountClear,
    dismissLabel: 'CANCEL',
    initialFocus: 'none',
    restore: 'none',
    onDismiss: () => {
      if (provider.credentialsPending) return;
      focusClearTrigger.current = true;
      setConfirmingClear(false);
    },
  });
  const statusCopy = status ? providerStatusCopy(status, now) : null;
  const deferCopy =
    status && hasActiveProviderDeferral(status, now) && status.deferReason !== null
      ? providerDeferralCopy(status.deferReason)
      : null;
  const accountCopy = accountStatusCopy(provider.account);
  const quota = status ? quotaSummary(status, now) : null;
  const clearUnavailableReason = provider.accountError
    ? 'Account status is unavailable. Retry the account status read before clearing stored credentials.'
    : provider.account?.state === 'vaultUnavailable'
      ? 'Secure account storage is unavailable. Clearing the stored account is disabled until storage can be read.'
      : null;
  const showClearAccount = Boolean(provider.account?.configured || clearUnavailableReason);
  const saveDisabled =
    provider.credentialsPending ||
    provider.credentialUsername.trim().length === 0 ||
    provider.credentialPassword.length === 0 ||
    provider.account?.state === 'vaultUnavailable';

  useEffect(() => {
    const deferredUntil = status?.deferredUntil;
    if (deferredUntil === null || deferredUntil === undefined || deferredUntil <= now) return;
    const timer = window.setTimeout(() => setNow(Date.now()), deferredUntil - now);
    return () => window.clearTimeout(timer);
  }, [now, status?.deferredUntil]);

  useEffect(() => {
    if (confirmingClear) {
      clearConfirmation.current?.focus();
    } else if (focusClearTrigger.current) {
      focusClearTrigger.current = false;
      (clearTrigger.current ?? accountHeading.current)?.focus();
    }
  }, [confirmingClear]);

  const submitCredentials = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void provider.saveCredentials();
  };

  const clearAccount = async () => {
    const cleared = await provider.clearCredentials();
    if (cleared) {
      focusClearTrigger.current = true;
      setConfirmingClear(false);
    }
  };

  return (
    <section
      aria-busy={
        provider.providerStatusLoading || provider.accountLoading || provider.credentialsPending
      }
      aria-labelledby="metadata-heading"
      className="settings-panel metadata-provider-panel settings-group"
      role="region"
    >
      <div className="panel-heading">
        <h2 id="metadata-heading">METADATA</h2>
        <span aria-hidden="true" />
        <span className="panel-meta">SETTINGS</span>
      </div>
      <section aria-labelledby="metadata-provider-heading" className="metadata-provider-section">
        <div className="metadata-row-heading">
          <h3 id="metadata-provider-heading">SCREENSCRAPER</h3>
          <span className="panel-meta">PROVIDER</span>
        </div>

        {provider.providerStatusError ? (
          <InlineError
            title="PROVIDER STATUS UNAVAILABLE"
            message="RetroFrontier could not read ScreenScraper status. Local content roots and cached metadata remain available."
            actionLabel="RETRY PROVIDER STATUS"
            onAction={() => void provider.refresh()}
          />
        ) : null}
        {provider.providerStatusLoading && !status ? (
          <p className="loading-inline" role="status">
            READING PROVIDER STATUS…
          </p>
        ) : null}
        {status && statusCopy ? (
          <div className="metadata-provider-status-area">
            <div
              aria-live="polite"
              className={`metadata-provider-status metadata-provider-status--${statusCopy.tone}`}
              role="status"
            >
              <strong>{statusCopy.label}</strong>
              <p>{statusCopy.description}</p>
            </div>
            {deferCopy &&
            (deferCopy.label !== statusCopy.label ||
              deferCopy.description !== statusCopy.description) ? (
              <div className="metadata-provider-defer-note">
                <strong>{deferCopy.label}</strong>
                <p>{deferCopy.description}</p>
              </div>
            ) : null}
            {quota ? (
              <dl className="metadata-provider-summary">
                <div>
                  <dt>QUOTA SNAPSHOT</dt>
                  <dd>{quota.quota}</dd>
                </div>
                <div>
                  <dt>BACKGROUND WORK</dt>
                  <dd>{quota.jobs}</dd>
                </div>
                <div>
                  <dt>QUOTA RECENCY</dt>
                  <dd>{quota.observed}</dd>
                </div>
              </dl>
            ) : null}
          </div>
        ) : null}
      </section>

      <section aria-labelledby="metadata-account-heading" className="metadata-account-section">
        <div className="metadata-account-heading">
          <div>
            <span className="settings-panel-kicker">PERSONAL ACCOUNT</span>
            <h3 id="metadata-account-heading" ref={accountHeading} tabIndex={-1}>
              OPTIONAL SCREENSCRAPER ACCOUNT
            </h3>
          </div>
          {provider.accountLoading && !provider.account ? (
            <span className="panel-meta">CHECKING</span>
          ) : null}
        </div>
        {provider.accountError ? (
          <InlineError
            title="ACCOUNT STATUS UNAVAILABLE"
            message="RetroFrontier could not read the optional personal account state. The local library remains available."
            actionLabel="RETRY ACCOUNT STATUS"
            onAction={() => void provider.refresh()}
          />
        ) : null}
        {provider.accountLoading && !provider.account ? (
          <p className="loading-inline" role="status">
            READING ACCOUNT STATE…
          </p>
        ) : null}
        {provider.account ? (
          <div
            aria-live="polite"
            className={`metadata-account-status metadata-account-status--${accountCopy.tone}`}
            role="status"
          >
            <strong>{accountCopy.label}</strong>
            <p>{accountCopy.description}</p>
          </div>
        ) : null}
        {provider.accountActionError ? (
          <div id="metadata-account-error">
            <InlineError
              title="ACCOUNT UPDATE FAILED"
              message="The personal account change could not be completed. No credential value was returned or displayed."
            />
          </div>
        ) : null}
        <form
          aria-labelledby="metadata-account-heading"
          className="metadata-account-form"
          onSubmit={submitCredentials}
        >
          <p className="metadata-account-help" id="metadata-account-help">
            Credentials are stored securely and are never displayed again.
          </p>
          <div className="metadata-account-fields">
            <label htmlFor="metadata-account-username">ACCOUNT NAME</label>
            <input
              autoComplete="username"
              disabled={provider.credentialsPending}
              id="metadata-account-username"
              name="username"
              onChange={(event) => provider.setCredentialUsername(event.target.value)}
              aria-describedby={
                provider.accountActionError
                  ? 'metadata-account-help metadata-account-error'
                  : 'metadata-account-help'
              }
              spellCheck="false"
              type="text"
              value={provider.credentialUsername}
            />
            <label htmlFor="metadata-account-password">ACCOUNT PASSWORD</label>
            <input
              autoComplete="current-password"
              disabled={provider.credentialsPending}
              id="metadata-account-password"
              name="password"
              onChange={(event) => provider.setCredentialPassword(event.target.value)}
              aria-describedby={
                provider.accountActionError
                  ? 'metadata-account-help metadata-account-error'
                  : 'metadata-account-help'
              }
              type="password"
              value={provider.credentialPassword}
            />
          </div>
          <div className="metadata-account-actions">
            <PixelButton disabled={saveDisabled} type="submit">
              {provider.credentialsPending ? 'SAVING ACCOUNT…' : 'SAVE ACCOUNT'}
            </PixelButton>
            {showClearAccount ? (
              confirmingClear ? (
                <div
                  aria-describedby="metadata-clear-account-copy"
                  aria-labelledby="metadata-clear-account-title"
                  className="metadata-clear-confirmation"
                  ref={clearScopeRef}
                  onKeyDown={(event) => {
                    if (event.key === 'Escape' && !provider.credentialsPending) {
                      event.preventDefault();
                      focusClearTrigger.current = true;
                      setConfirmingClear(false);
                    }
                  }}
                  role="alertdialog"
                >
                  <span id="metadata-clear-account-title">FORGET THIS PERSONAL ACCOUNT?</span>
                  <p id="metadata-clear-account-copy">
                    This removes RetroFrontier&apos;s stored account only. It does not delete local
                    games or provider metadata already cached.
                  </p>
                  <PixelButton
                    onClick={() => {
                      focusClearTrigger.current = true;
                      setConfirmingClear(false);
                    }}
                    type="button"
                    variant="secondary"
                  >
                    CANCEL
                  </PixelButton>
                  <PixelButton
                    ref={clearConfirmation}
                    disabled={provider.credentialsPending || clearUnavailableReason !== null}
                    onClick={() => void clearAccount()}
                    type="button"
                  >
                    CONFIRM CLEAR ACCOUNT
                  </PixelButton>
                </div>
              ) : (
                <PixelButton
                  aria-describedby={
                    clearUnavailableReason ? 'metadata-clear-account-unavailable' : undefined
                  }
                  ref={clearTrigger}
                  disabled={provider.credentialsPending || clearUnavailableReason !== null}
                  onClick={() => setConfirmingClear(true)}
                  type="button"
                  variant="secondary"
                >
                  CLEAR PERSONAL ACCOUNT
                </PixelButton>
              )
            ) : null}
          </div>
          {clearUnavailableReason ? (
            <p
              className="metadata-account-help metadata-account-error"
              id="metadata-clear-account-unavailable"
            >
              {clearUnavailableReason}
            </p>
          ) : null}
          {provider.credentialsPending ? (
            <p className="game-detail-inline-status" role="status">
              UPDATING PERSONAL ACCOUNT…
            </p>
          ) : null}
        </form>
      </section>
    </section>
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
  const metadataProvider = useMetadataProvider();
  const managedRuntime = useManagedRuntime();
  const [pendingOperation, setPendingOperation] = useState<RootOperation | null>(null);
  const [pendingRemoval, setPendingRemoval] = useState<number | null>(null);
  const [removalFocusTarget, setRemovalFocusTarget] = useState<
    { kind: 'trigger'; rootId: number } | { kind: 'heading' } | null
  >(null);
  const [actionError, setActionError] = useState<IpcError | null>(null);
  const [lastOperation, setLastOperation] = useState<RootOperation | null>(null);
  const addButton = useRef<HTMLButtonElement>(null);
  const rootsHeading = useRef<HTMLHeadingElement>(null);
  const settingsHeadingRef = useFocusNode({ id: focusNodes.settings('heading') });
  const confirmationButton = useRef<HTMLButtonElement>(null);
  const removeTriggers = useRef(new Map<number, HTMLButtonElement>());

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
    <main aria-labelledby="settings-heading" className="app-main" id="main-content">
      <div className="settings-content">
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
          {/* A programmatic focus target only, exactly like the Library heading: it is the
              deterministic Settings fallback for a focus return, and directional movement never
              lands on it. */}
          <h1 id="settings-heading" ref={settingsHeadingRef} tabIndex={-1}>
            <PixelArrow className="heading-arrow" />
            SETTINGS
          </h1>
          <span aria-hidden="true" />
        </div>
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

        <section className="settings-panel settings-group" aria-labelledby="roots-heading">
          <div className="panel-heading">
            <h2 id="roots-heading" ref={rootsHeading} tabIndex={-1}>
              LIBRARY
            </h2>
            <span aria-hidden="true" />
            <span className="panel-meta">
              {rootsLoading ? 'CHECKING' : `${roots.length} FOLDERS`}
            </span>
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
                  void runOperation(
                    { kind: 'toggle', rootId: root.id, enabled: !root.enabled },
                    () => updateRootEnabled(root.id, !root.enabled),
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
                  setRemovalFocusTarget({ kind: 'heading' });
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
        </section>
        <RuntimePanel runtime={managedRuntime} />
        <MetadataProviderPanel provider={metadataProvider} />
      </div>
    </main>
  );
}
