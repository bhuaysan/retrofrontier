import { act, renderHook, waitFor } from '@testing-library/react';
import { StrictMode, type ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { MetadataProviderStatus, ProviderAccountStatus } from '../platform/ipc';
import { useMetadataProvider } from './useMetadataProvider';

const mocks = vi.hoisted(() => ({
  getMetadataProviderStatus: vi.fn(),
  getMetadataProviderAccount: vi.fn(),
  setMetadataProviderCredentials: vi.fn(),
  clearMetadataProviderCredentials: vi.fn(),
}));

vi.mock('../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../platform/ipc')>();
  return {
    ...actual,
    getMetadataProviderStatus: mocks.getMetadataProviderStatus,
    getMetadataProviderAccount: mocks.getMetadataProviderAccount,
    setMetadataProviderCredentials: mocks.setMetadataProviderCredentials,
    clearMetadataProviderCredentials: mocks.clearMetadataProviderCredentials,
  };
});

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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

describe('useMetadataProvider', () => {
  beforeEach(() => {
    mocks.getMetadataProviderStatus.mockReset().mockResolvedValue(providerStatus);
    mocks.getMetadataProviderAccount.mockReset().mockResolvedValue(notConfiguredAccount);
    mocks.setMetadataProviderCredentials.mockReset().mockResolvedValue(undefined);
    mocks.clearMetadataProviderCredentials.mockReset().mockResolvedValue(undefined);
  });

  it('loads provider and account state through separate bounded reads', async () => {
    const { result } = renderHook(() => useMetadataProvider());

    expect(result.current.providerStatusLoading).toBe(true);
    expect(result.current.accountLoading).toBe(true);

    await waitFor(() => {
      expect(result.current.providerStatus).toEqual(providerStatus);
      expect(result.current.account).toEqual(notConfiguredAccount);
    });

    expect(mocks.getMetadataProviderStatus).toHaveBeenCalledTimes(1);
    expect(mocks.getMetadataProviderAccount).toHaveBeenCalledTimes(1);
    expect(result.current.providerStatusError).toBeNull();
    expect(result.current.accountError).toBeNull();
  });

  it('does not duplicate initial reads when React replays effects in StrictMode', async () => {
    const wrapper = ({ children }: { children: ReactNode }) => <StrictMode>{children}</StrictMode>;
    const { result } = renderHook(() => useMetadataProvider(), { wrapper });

    await waitFor(() => expect(result.current.account).toEqual(notConfiguredAccount));

    expect(mocks.getMetadataProviderStatus).toHaveBeenCalledTimes(1);
    expect(mocks.getMetadataProviderAccount).toHaveBeenCalledTimes(1);
  });

  it('keeps the account surface available when provider status fails and retries the failed read', async () => {
    mocks.getMetadataProviderStatus
      .mockRejectedValueOnce({ code: 'metadata_unavailable', message: 'internal provider detail' })
      .mockResolvedValueOnce(providerStatus);
    const { result } = renderHook(() => useMetadataProvider());

    await waitFor(() =>
      expect(result.current.providerStatusError?.code).toBe('metadata_unavailable'),
    );
    expect(result.current.account).toEqual(notConfiguredAccount);

    await act(async () => result.current.refresh());

    expect(result.current.providerStatus).toEqual(providerStatus);
    expect(result.current.providerStatusError).toBeNull();
    expect(mocks.getMetadataProviderStatus).toHaveBeenCalledTimes(2);
    expect(mocks.getMetadataProviderAccount).toHaveBeenCalledTimes(2);
  });

  it('submits credentials write-only, clears the password, and refetches authoritative state', async () => {
    const { result } = renderHook(() => useMetadataProvider());

    await waitFor(() => expect(result.current.account).toEqual(notConfiguredAccount));
    act(() => {
      result.current.setCredentialUsername('test-user');
      result.current.setCredentialPassword('fake-secret-never-render');
    });

    await act(async () => {
      await result.current.saveCredentials();
    });

    expect(mocks.setMetadataProviderCredentials).toHaveBeenCalledWith({
      username: 'test-user',
      password: 'fake-secret-never-render',
    });
    expect(result.current.credentialUsername).toBe('test-user');
    expect(result.current.credentialPassword).toBe('');
    expect(mocks.getMetadataProviderStatus).toHaveBeenCalledTimes(2);
    expect(mocks.getMetadataProviderAccount).toHaveBeenCalledTimes(2);
  });

  it('allows only one credential mutation while a save is in flight', async () => {
    const save = deferred<void>();
    mocks.setMetadataProviderCredentials.mockReturnValue(save.promise);
    const { result } = renderHook(() => useMetadataProvider());
    await waitFor(() => expect(result.current.account).toEqual(notConfiguredAccount));

    act(() => {
      result.current.setCredentialUsername('test-user');
      result.current.setCredentialPassword('fake-secret-never-render');
    });

    let first: Promise<boolean>;
    let second: Promise<boolean>;
    act(() => {
      first = result.current.saveCredentials();
      second = result.current.saveCredentials();
    });

    expect(mocks.setMetadataProviderCredentials).toHaveBeenCalledTimes(1);
    expect(result.current.credentialsPending).toBe(true);

    await act(async () => {
      save.resolve();
      await Promise.all([first!, second!]);
    });
    expect(result.current.credentialsPending).toBe(false);
  });

  it('clears the account only after the native command succeeds and then refetches state', async () => {
    const configuredAccount: ProviderAccountStatus = {
      configured: true,
      state: 'configured',
      username: 'test-user',
    };
    mocks.getMetadataProviderAccount.mockResolvedValue(configuredAccount);
    const { result } = renderHook(() => useMetadataProvider());
    await waitFor(() => expect(result.current.account).toEqual(configuredAccount));

    await act(async () => {
      await result.current.clearCredentials();
    });

    expect(mocks.clearMetadataProviderCredentials).toHaveBeenCalledTimes(1);
    expect(mocks.getMetadataProviderStatus).toHaveBeenCalledTimes(2);
    expect(mocks.getMetadataProviderAccount).toHaveBeenCalledTimes(2);
  });

  it('does not start a refetch or commit settings state after unmount', async () => {
    const save = deferred<void>();
    mocks.setMetadataProviderCredentials.mockReturnValue(save.promise);
    const { result, unmount } = renderHook(() => useMetadataProvider());
    await waitFor(() => expect(result.current.account).toEqual(notConfiguredAccount));
    act(() => {
      result.current.setCredentialUsername('test-user');
      result.current.setCredentialPassword('fake-secret-never-render');
    });

    let operation: Promise<boolean>;
    act(() => {
      operation = result.current.saveCredentials();
    });
    unmount();

    await act(async () => {
      save.resolve();
      await operation!;
    });

    expect(mocks.getMetadataProviderStatus).toHaveBeenCalledTimes(1);
    expect(mocks.getMetadataProviderAccount).toHaveBeenCalledTimes(1);
  });
});
