import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type {
  ContentRoot,
  MetadataProviderStatus,
  ProviderAccountStatus,
} from '../../platform/ipc';
import { SettingsPage } from './SettingsPage';

const mocks = vi.hoisted(() => ({
  getMetadataProviderStatus: vi.fn(),
  getMetadataProviderAccount: vi.fn(),
  setMetadataProviderCredentials: vi.fn(),
  clearMetadataProviderCredentials: vi.fn(),
}));

vi.mock('../../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../platform/ipc')>();
  return {
    ...actual,
    getMetadataProviderStatus: mocks.getMetadataProviderStatus,
    getMetadataProviderAccount: mocks.getMetadataProviderAccount,
    setMetadataProviderCredentials: mocks.setMetadataProviderCredentials,
    clearMetadataProviderCredentials: mocks.clearMetadataProviderCredentials,
  };
});

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

const providerStatus: MetadataProviderStatus = {
  providerId: 'screenScraper',
  credentialsConfigured: true,
  userAccount: 'notConfigured',
  userAccountName: null,
  quota: {
    maxThreads: 1,
    maxRequestsPerMinute: 60,
    maxRequestsPerDay: 1000,
    maxNegativeRequestsPerDay: 100,
    requestsToday: 4,
    negativeRequestsToday: 1,
  },
  quotaObservedAt: 100,
  deferredUntil: null,
  deferReason: null,
  offline: false,
  pendingJobs: 2,
  deferredJobs: 0,
  failedJobs: 0,
};

const notConfiguredAccount: ProviderAccountStatus = {
  configured: false,
  state: 'notConfigured',
  username: null,
};

const configuredAccount: ProviderAccountStatus = {
  configured: true,
  state: 'configured',
  username: 'test-user',
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

function renderPage() {
  return render(
    <SettingsPage
      roots={[managedRoot]}
      rootsLoading={false}
      rootsError={null}
      refreshRoots={vi.fn().mockResolvedValue([managedRoot])}
      removeExternalRoot={vi.fn().mockResolvedValue(undefined)}
      updateRootEnabled={vi.fn().mockResolvedValue(managedRoot)}
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
    />,
  );
}

function renderMetadataPage() {
  return renderPage();
}

describe('SettingsPage metadata/provider section', () => {
  beforeEach(() => {
    mocks.getMetadataProviderStatus.mockReset().mockResolvedValue(providerStatus);
    mocks.getMetadataProviderAccount.mockReset().mockResolvedValue(notConfiguredAccount);
    mocks.setMetadataProviderCredentials.mockReset().mockResolvedValue(undefined);
    mocks.clearMetadataProviderCredentials.mockReset().mockResolvedValue(undefined);
  });

  it('keeps root management visible while presenting provider and account state', async () => {
    renderPage();

    await waitFor(() => expect(screen.getByText('PROVIDER ONLINE')).toBeInTheDocument());

    expect(screen.getByRole('heading', { name: 'METADATA' })).toBeInTheDocument();
    expect(screen.getByText(managedRoot.path)).toBeInTheDocument();
    expect(screen.getByText('OPTIONAL ACCOUNT NOT CONFIGURED')).toBeInTheDocument();
    const providerSection = screen.getByRole('region', { name: 'METADATA' });
    expect(within(providerSection).getAllByRole('status')).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ textContent: expect.stringContaining('PROVIDER ONLINE') }),
        expect.objectContaining({ textContent: expect.stringContaining('OPTIONAL ACCOUNT') }),
      ]),
    );
    expect(screen.getByLabelText(/account name/i)).toHaveAttribute('autocomplete', 'username');
    expect(screen.getByLabelText(/account password/i)).toHaveAttribute(
      'autocomplete',
      'current-password',
    );
    expect(screen.getByLabelText(/account password/i)).toHaveAttribute('type', 'password');
    expect(screen.getByText('DAILY 4 / 1,000 · NO-MATCH 1 / 100')).toBeInTheDocument();
    expect(screen.getByText('2 PENDING · 0 DEFERRED · 0 NEED RETRY')).toBeInTheDocument();
    expect(screen.getByText('SNAPSHOT MAY BE STALE')).toBeInTheDocument();
    expect(screen.getByLabelText(/account name/i)).toHaveAttribute(
      'aria-describedby',
      'metadata-account-help',
    );
  });

  it('uses one continuous settings page without invented tab navigation', async () => {
    renderPage();

    await waitFor(() => expect(screen.getByText('PROVIDER ONLINE')).toBeInTheDocument());
    expect(screen.getByRole('heading', { name: 'LIBRARY' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'METADATA' })).toBeInTheDocument();
    expect(screen.getByText(managedRoot.path)).toBeInTheDocument();
    expect(screen.queryByRole('tablist')).not.toBeInTheDocument();
    expect(screen.queryByRole('tab')).not.toBeInTheDocument();
    expect(screen.queryByRole('tabpanel')).not.toBeInTheDocument();
    expect(screen.queryByText(/later milestones/i)).not.toBeInTheDocument();
  });

  it('submits a personal account write-only and clears the password after authoritative refresh', async () => {
    mocks.getMetadataProviderAccount
      .mockResolvedValueOnce(notConfiguredAccount)
      .mockResolvedValue(configuredAccount);
    renderMetadataPage();
    await waitFor(() =>
      expect(screen.getByText('OPTIONAL ACCOUNT NOT CONFIGURED')).toBeInTheDocument(),
    );

    fireEvent.change(screen.getByLabelText(/account name/i), { target: { value: 'test-user' } });
    fireEvent.change(screen.getByLabelText(/account password/i), {
      target: { value: 'fake-secret-never-render' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'SAVE ACCOUNT' }));

    await waitFor(() => expect(screen.getByText('PERSONAL ACCOUNT SAVED')).toBeInTheDocument());

    expect(mocks.setMetadataProviderCredentials).toHaveBeenCalledWith({
      username: 'test-user',
      password: 'fake-secret-never-render',
    });
    expect(screen.getByLabelText(/account password/i)).toHaveValue('');
    expect(screen.queryByDisplayValue('fake-secret-never-render')).not.toBeInTheDocument();
    expect(screen.queryByText('fake-secret-never-render')).not.toBeInTheDocument();
  });

  it('treats provider-status failure as local section failure and retries only that read', async () => {
    mocks.getMetadataProviderStatus
      .mockRejectedValueOnce({
        code: 'metadata_unavailable',
        message: 'https://provider.invalid?password=fake-secret-never-render',
      })
      .mockResolvedValueOnce(providerStatus);
    renderMetadataPage();

    await waitFor(() =>
      expect(screen.getByText('PROVIDER STATUS UNAVAILABLE')).toBeInTheDocument(),
    );
    expect(screen.getByText(managedRoot.path)).toBeInTheDocument();
    expect(
      screen.queryByText(/provider\.invalid|fake-secret-never-render/i),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'RETRY PROVIDER STATUS' }));
    await waitFor(() => expect(screen.getByText('PROVIDER ONLINE')).toBeInTheDocument());
    expect(mocks.getMetadataProviderStatus).toHaveBeenCalledTimes(2);
  });

  it('shows offline and quota deferral as enrichment states without blocking local roots', async () => {
    mocks.getMetadataProviderStatus.mockResolvedValue({
      ...providerStatus,
      offline: true,
      deferReason: 'dailyQuotaExceeded',
      deferredUntil: Date.now() + 60_000,
      pendingJobs: 0,
      deferredJobs: 3,
    });
    renderMetadataPage();

    const section = await screen.findByRole('region', { name: 'METADATA' });
    expect(within(section).getByText('PROVIDER OFFLINE')).toBeInTheDocument();
    expect(within(section).getByText(/cached metadata remains available/i)).toBeInTheDocument();
    expect(within(section).getByText('DAILY QUOTA DEFERRED')).toBeInTheDocument();
    expect(screen.getByText(managedRoot.path)).toBeInTheDocument();
    expect(screen.queryByText(/999999|countdown|429|430|431/i)).not.toBeInTheDocument();
  });

  it('requires an explicit second action before clearing a personal account', async () => {
    mocks.getMetadataProviderAccount.mockResolvedValue(configuredAccount);
    renderMetadataPage();
    await waitFor(() => expect(screen.getByText('PERSONAL ACCOUNT SAVED')).toBeInTheDocument());

    const clearTrigger = screen.getByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' });
    fireEvent.click(clearTrigger);
    expect(mocks.clearMetadataProviderCredentials).not.toHaveBeenCalled();
    expect(screen.getByText(/removes RetroFrontier's stored account only/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'CANCEL' }));
    const restoredClearTrigger = screen.getByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' });
    expect(document.activeElement).toBe(restoredClearTrigger);

    fireEvent.click(restoredClearTrigger);
    fireEvent.click(screen.getByRole('button', { name: 'CONFIRM CLEAR ACCOUNT' }));
    await waitFor(() => expect(mocks.clearMetadataProviderCredentials).toHaveBeenCalledTimes(1));
  });

  it('redacts credential mutation failures from the account surface', async () => {
    mocks.setMetadataProviderCredentials.mockRejectedValueOnce({
      code: 'metadata_unavailable',
      message: 'https://provider.invalid?password=fake-secret-never-render',
    });
    renderMetadataPage();
    await waitFor(() =>
      expect(screen.getByText('OPTIONAL ACCOUNT NOT CONFIGURED')).toBeInTheDocument(),
    );

    fireEvent.change(screen.getByLabelText(/account name/i), { target: { value: 'test-user' } });
    fireEvent.change(screen.getByLabelText(/account password/i), {
      target: { value: 'fake-secret-never-render' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'SAVE ACCOUNT' }));

    await waitFor(() => expect(screen.getByText('ACCOUNT UPDATE FAILED')).toBeInTheDocument());
    expect(
      screen.queryByText(/provider\.invalid|fake-secret-never-render/i),
    ).not.toBeInTheDocument();
    expect(screen.getByLabelText(/account password/i)).toHaveValue('');
    expect(screen.getByLabelText(/account name/i)).toHaveAttribute(
      'aria-describedby',
      'metadata-account-help metadata-account-error',
    );
    expect(screen.getByLabelText(/account password/i)).toHaveAttribute(
      'aria-describedby',
      'metadata-account-help metadata-account-error',
    );
  });

  it('locks credential fields while an account save is in flight', async () => {
    const save = deferred<void>();
    mocks.setMetadataProviderCredentials.mockReturnValue(save.promise);
    renderMetadataPage();
    await waitFor(() =>
      expect(screen.getByText('OPTIONAL ACCOUNT NOT CONFIGURED')).toBeInTheDocument(),
    );

    const username = screen.getByLabelText(/account name/i);
    const password = screen.getByLabelText(/account password/i);
    fireEvent.change(username, { target: { value: 'test-user' } });
    fireEvent.change(password, { target: { value: 'fake-secret-never-render' } });
    fireEvent.click(screen.getByRole('button', { name: 'SAVE ACCOUNT' }));

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'SAVING ACCOUNT…' })).toBeDisabled(),
    );
    expect(username).toBeDisabled();
    expect(password).toBeDisabled();

    await act(async () => {
      save.resolve();
    });
    await waitFor(() => expect(password).toHaveValue(''));
  });

  it('keeps account clearing reachable but disabled when the vault is unavailable', async () => {
    mocks.getMetadataProviderAccount.mockResolvedValue({
      configured: false,
      state: 'vaultUnavailable',
      username: null,
    } satisfies ProviderAccountStatus);
    renderMetadataPage();

    const clear = await screen.findByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' });
    expect(clear).toBeDisabled();
    expect(clear).toHaveAccessibleDescription(
      'Secure account storage is unavailable. Clearing the stored account is disabled until storage can be read.',
    );
  });

  it('keeps account clearing reachable but disabled when account status cannot be read', async () => {
    mocks.getMetadataProviderAccount.mockRejectedValueOnce({
      code: 'metadata_unavailable',
      message: 'internal account detail',
    });
    renderMetadataPage();

    const clear = await screen.findByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' });
    expect(clear).toBeDisabled();
    expect(clear).toHaveAccessibleDescription(
      'Account status is unavailable. Retry the account status read before clearing stored credentials.',
    );
  });

  it('closes the account confirmation with Escape and restores focus to its trigger', async () => {
    mocks.getMetadataProviderAccount.mockResolvedValue(configuredAccount);
    renderMetadataPage();
    const clear = await screen.findByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' });
    fireEvent.click(clear);
    const confirmation = await screen.findByRole('alertdialog', {
      name: /forget this personal account/i,
    });
    fireEvent.keyDown(confirmation, { key: 'Escape' });

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'CLEAR PERSONAL ACCOUNT' })).toHaveFocus(),
    );
    expect(
      screen.queryByRole('alertdialog', { name: /forget this personal account/i }),
    ).not.toBeInTheDocument();
  });
});
