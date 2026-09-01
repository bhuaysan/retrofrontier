import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ControllerFooter } from '../../components/ui/ControllerFooter';
import { FocusProvider } from '../../focus/FocusProvider';
import { useFocusApi } from '../../focus/focusContext';
import { installRectStub, layoutColumn } from '../../test/geometry';
import type {
  ContentRoot,
  MetadataProviderStatus,
  ProviderAccountStatus,
  RuntimeInstallState,
} from '../../platform/ipc';
import { SettingsPage } from './SettingsPage';

const mocks = vi.hoisted(() => ({
  getMetadataProviderStatus: vi.fn(),
  getMetadataProviderAccount: vi.fn(),
  setMetadataProviderCredentials: vi.fn(),
  clearMetadataProviderCredentials: vi.fn(),
  getRuntimeInstallState: vi.fn(),
  installRuntime: vi.fn(),
  repairRuntime: vi.fn(),
}));

vi.mock('../../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../platform/ipc')>();
  return {
    ...actual,
    getMetadataProviderStatus: mocks.getMetadataProviderStatus,
    getMetadataProviderAccount: mocks.getMetadataProviderAccount,
    setMetadataProviderCredentials: mocks.setMetadataProviderCredentials,
    clearMetadataProviderCredentials: mocks.clearMetadataProviderCredentials,
    getRuntimeInstallState: mocks.getRuntimeInstallState,
    installRuntime: mocks.installRuntime,
    repairRuntime: mocks.repairRuntime,
  };
});

const runtimeState: RuntimeInstallState = {
  status: {
    state: 'ready',
    installationId: '01JRUNTIMEINSTALLATION0001',
    releaseId: 'rf-runtime-1.22.2-linux-x86_64-001',
    canRollback: false,
    repairRequired: false,
  },
  sourceConfigured: true,
  sourceOrigin: 'qualification',
  releaseTarget: 'rf-runtime-linux-x86_64-001.manifest.json',
  installing: false,
};

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

const externalRoot: ContentRoot = { ...managedRoot, id: 2, kind: 'external', path: '/roms/extra' };

const providerStatus: MetadataProviderStatus = {
  providerId: 'screenScraper',
  credentialsConfigured: true,
  userAccount: 'configured',
  userAccountName: 'test-user',
  quota: {
    maxThreads: 1,
    maxRequestsPerMinute: 60,
    maxRequestsPerDay: 1000,
    maxNegativeRequestsPerDay: 100,
    requestsToday: 0,
    negativeRequestsToday: 0,
  },
  quotaObservedAt: 100,
  deferredUntil: null,
  deferReason: null,
  offline: false,
  pendingJobs: 0,
  deferredJobs: 0,
  failedJobs: 0,
};

const configuredAccount: ProviderAccountStatus = {
  configured: true,
  state: 'configured',
  username: 'test-user',
};

function Dispatcher() {
  const api = useFocusApi();
  return (
    <>
      <button
        aria-hidden="true"
        data-testid="dispatch-back"
        onClick={() => api.dispatch('back', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-down"
        onClick={() => api.dispatch('moveDown', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-confirm"
        onClick={() => api.dispatch('confirm', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-context"
        onClick={() => api.dispatch('context', 'gamepad')}
        type="button"
      />
    </>
  );
}

function renderSettings(removeExternalRoot = vi.fn().mockResolvedValue(undefined)) {
  const result = render(
    <FocusProvider>
      <Dispatcher />
      <SettingsPage
        roots={[managedRoot, externalRoot]}
        rootsLoading={false}
        rootsError={null}
        refreshRoots={vi.fn().mockResolvedValue([managedRoot, externalRoot])}
        removeExternalRoot={removeExternalRoot}
        updateRootEnabled={vi.fn().mockResolvedValue(externalRoot)}
        systems={[]}
        scan={{
          status: null,
          scanStartPending: false,
          scanStartError: null,
          startScan: vi.fn().mockResolvedValue(null),
        }}
        refreshSummary={vi.fn().mockResolvedValue(undefined)}
        onAddExternalFolder={vi.fn().mockResolvedValue(true)}
        onOpenManagedFolder={vi.fn().mockResolvedValue(undefined)}
        onBackToLibrary={vi.fn()}
      />
      <ControllerFooter controllerConnected gameRunning={false} interactive status="SCAN READY" />
    </FocusProvider>,
  );
  return { ...result, removeExternalRoot };
}

function footerHints() {
  return screen.getByRole('list', { name: 'Controller actions' });
}

function send(action: 'back' | 'down' | 'confirm' | 'context') {
  act(() => {
    fireEvent.click(screen.getByTestId(`dispatch-${action}`));
  });
}

beforeEach(() => {
  installRectStub();
  mocks.getRuntimeInstallState.mockReset().mockResolvedValue(runtimeState);
  mocks.installRuntime.mockReset();
  mocks.repairRuntime.mockReset();
  mocks.getMetadataProviderStatus.mockReset().mockResolvedValue(providerStatus);
  mocks.getMetadataProviderAccount.mockReset().mockResolvedValue(configuredAccount);
  mocks.setMetadataProviderCredentials.mockReset().mockResolvedValue(undefined);
  mocks.clearMetadataProviderCredentials.mockReset().mockResolvedValue(undefined);
});

function removalConfirmation() {
  return screen.getByRole('alertdialog', {
    name: 'Remove this root from RetroFrontier? Files stay on disk.',
  });
}

/**
 * A control whose activation semantics change while it can hold focus must make the footer
 * reactive. The managed-runtime action is the concrete case: nothing about the surrounding screen
 * rerenders when an installation starts, so an unregistered button left the footer claiming a
 * generic `CONFIRM` for a control that had just become disabled.
 */
describe('Settings dynamic action footer reactivity', () => {
  it('follows the managed-runtime action through its disabled transitions', async () => {
    let settleInstall: (() => void) | undefined;
    mocks.repairRuntime.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          settleInstall = resolve;
        }),
    );
    renderSettings();
    const action = await screen.findByRole('button', { name: 'REINSTALL RUNTIME' });
    act(() => action.focus());
    // Registered, so the hint names what the control really does rather than a generic CONFIRM.
    expect(within(footerHints()).getByText('REINSTALL RUNTIME')).toBeInTheDocument();

    // The installation starts: the same element becomes disabled without focus moving anywhere.
    await act(async () => {
      fireEvent.click(action);
    });
    expect(action).toHaveFocus();
    expect(within(footerHints()).queryByText('REINSTALL RUNTIME')).not.toBeInTheDocument();
    expect(within(footerHints()).queryByText('CONFIRM')).not.toBeInTheDocument();

    // It finishes and the action is offered again.
    await act(async () => {
      settleInstall?.();
    });
    await waitFor(() =>
      expect(within(footerHints()).getByText('REINSTALL RUNTIME')).toBeInTheDocument(),
    );
  });

  it('does not offer confirm for a runtime action this build cannot perform', async () => {
    mocks.getRuntimeInstallState.mockResolvedValue({
      ...runtimeState,
      sourceConfigured: false,
    });
    renderSettings();
    const action = await screen.findByRole('button', { name: 'REINSTALL RUNTIME' });
    act(() => action.focus());
    expect(within(footerHints()).queryByText('REINSTALL RUNTIME')).not.toBeInTheDocument();
    expect(within(footerHints()).queryByText('CONFIRM')).not.toBeInTheDocument();
  });
});

describe('SettingsPage focus scopes', () => {
  it('keeps the existing removal-confirmation focus behaviour', async () => {
    renderSettings();
    const trigger = await screen.findByRole('button', { name: 'REMOVE ROOT' });
    act(() => trigger.focus());
    fireEvent.click(trigger);

    await waitFor(() =>
      expect(
        within(removalConfirmation()).getByRole('button', { name: 'REMOVE ROOT' }),
      ).toHaveFocus(),
    );
  });

  it('cancels the removal confirmation with back and returns focus to its trigger', async () => {
    renderSettings();
    fireEvent.click(await screen.findByRole('button', { name: 'REMOVE ROOT' }));
    await waitFor(() => expect(removalConfirmation()).toBeInTheDocument());

    send('back');

    await waitFor(() =>
      expect(
        screen.queryByRole('alertdialog', {
          name: 'Remove this root from RetroFrontier? Files stay on disk.',
        }),
      ).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(screen.getByRole('button', { name: 'REMOVE ROOT' })).toHaveFocus());
  });

  it('contains directional navigation inside the removal confirmation', async () => {
    renderSettings();
    fireEvent.click(await screen.findByRole('button', { name: 'REMOVE ROOT' }));
    await waitFor(() => expect(removalConfirmation()).toBeInTheDocument());
    const dialog = removalConfirmation();
    const cancel = within(dialog).getByRole('button', { name: 'CANCEL' });
    const confirm = within(dialog).getByRole('button', { name: 'REMOVE ROOT' });
    // An ordinary page control laid out below the confirmation: without the scope, movement would
    // leave the confirmation and land on it.
    const outside = screen.getAllByRole('button', { name: /BACK TO LIBRARY/ })[0];
    layoutColumn([cancel, confirm, outside]);

    act(() => cancel.focus());
    send('down');
    expect(confirm).toHaveFocus();
    // Nothing outside the confirmation may be reached while it owns focus.
    send('down');
    expect(confirm).toHaveFocus();
  });

  it('cancels the metadata account confirmation with back', async () => {
    renderSettings();
    const clearTrigger = await screen.findByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' });
    fireEvent.click(clearTrigger);
    await screen.findByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' });

    send('back');

    await waitFor(() =>
      expect(
        screen.queryByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' }),
      ).not.toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' })).toHaveFocus(),
    );
  });
});

describe('SettingsPage scope activation boundary', () => {
  it('refuses controller activation outside the open removal confirmation', async () => {
    const { removeExternalRoot } = renderSettings();
    fireEvent.click(await screen.findByRole('button', { name: 'REMOVE ROOT' }));
    await waitFor(() => expect(removalConfirmation()).toBeInTheDocument());

    // Tab or a pointer can still leave this non-modal confirmation.
    const outside = screen.getAllByRole('button', { name: /BACK TO LIBRARY/ })[0];
    act(() => outside.focus());
    send('confirm');
    send('context');

    expect(removeExternalRoot).not.toHaveBeenCalled();
    expect(removalConfirmation()).toBeInTheDocument();
  });

  it('re-enters the removal confirmation on the next directional action', async () => {
    renderSettings();
    fireEvent.click(await screen.findByRole('button', { name: 'REMOVE ROOT' }));
    await waitFor(() => expect(removalConfirmation()).toBeInTheDocument());
    const dialog = removalConfirmation();
    const cancel = within(dialog).getByRole('button', { name: 'CANCEL' });
    const confirm = within(dialog).getByRole('button', { name: 'REMOVE ROOT' });
    const outside = screen.getAllByRole('button', { name: /BACK TO LIBRARY/ })[0];
    layoutColumn([cancel, confirm, outside]);

    act(() => outside.focus());
    send('down');
    expect(cancel).toHaveFocus();
  });

  it('refuses controller activation outside the metadata account confirmation', async () => {
    renderSettings();
    fireEvent.click(await screen.findByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' }));
    await screen.findByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' });

    const outside = screen.getAllByRole('button', { name: /BACK TO LIBRARY/ })[0];
    act(() => outside.focus());
    send('confirm');

    expect(mocks.clearMetadataProviderCredentials).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' })).toBeInTheDocument();
  });

  it('activates ordinary Settings controls again once a confirmation is dismissed', async () => {
    renderSettings();
    fireEvent.click(await screen.findByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' }));
    await screen.findByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' });
    send('back');
    await waitFor(() =>
      expect(
        screen.queryByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' }),
      ).not.toBeInTheDocument(),
    );

    const trigger = screen.getByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' });
    act(() => trigger.focus());
    send('confirm');
    expect(await screen.findByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' })).toBeVisible();
  });

  it('keeps Escape inside the Settings credential fields out of page navigation', async () => {
    renderSettings();
    const username = await screen.findByLabelText('ACCOUNT NAME');
    const password = screen.getByLabelText('ACCOUNT PASSWORD');
    for (const field of [username, password]) {
      act(() => field.focus());
      // `fireEvent` returns false when a handler called preventDefault: the platform must keep the
      // event, so the field's own Escape behaviour is not replaced by page-level navigation.
      expect(fireEvent.keyDown(field, { key: 'Escape' })).toBe(true);
      expect(field).toHaveFocus();
    }
    expect(screen.getByRole('heading', { level: 1, name: /SETTINGS/ })).toBeInTheDocument();
  });
});
