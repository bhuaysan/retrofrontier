import { useCallback, useEffect, useRef, useState } from 'react';

import {
  clearMetadataProviderCredentials,
  getMetadataProviderAccount,
  getMetadataProviderStatus,
  normalizeIpcError,
  setMetadataProviderCredentials,
  type IpcError,
  type MetadataProviderStatus,
  type ProviderAccountStatus,
} from '../platform/ipc';

export interface MetadataProviderModel {
  providerStatus: MetadataProviderStatus | null;
  account: ProviderAccountStatus | null;
  providerStatusLoading: boolean;
  accountLoading: boolean;
  providerStatusError: IpcError | null;
  accountError: IpcError | null;
  credentialUsername: string;
  credentialPassword: string;
  setCredentialUsername: (username: string) => void;
  setCredentialPassword: (password: string) => void;
  credentialsPending: boolean;
  accountActionError: IpcError | null;
  refresh: () => Promise<void>;
  saveCredentials: () => Promise<boolean>;
  clearCredentials: () => Promise<boolean>;
}

export function useMetadataProvider(): MetadataProviderModel {
  const mounted = useRef(true);
  const readGeneration = useRef(0);
  const credentialsMutationPending = useRef(false);
  const [providerStatus, setProviderStatus] = useState<MetadataProviderStatus | null>(null);
  const [account, setAccount] = useState<ProviderAccountStatus | null>(null);
  const [providerStatusLoading, setProviderStatusLoading] = useState(true);
  const [accountLoading, setAccountLoading] = useState(true);
  const [providerStatusError, setProviderStatusError] = useState<IpcError | null>(null);
  const [accountError, setAccountError] = useState<IpcError | null>(null);
  const [credentialUsername, setCredentialUsername] = useState('');
  const [credentialPassword, setCredentialPassword] = useState('');
  const [credentialsPending, setCredentialsPending] = useState(false);
  const [accountActionError, setAccountActionError] = useState<IpcError | null>(null);

  const refresh = useCallback(async () => {
    const generation = readGeneration.current + 1;
    readGeneration.current = generation;
    if (mounted.current) {
      setProviderStatusLoading(true);
      setAccountLoading(true);
      setProviderStatusError(null);
      setAccountError(null);
    }

    const [statusResult, accountResult] = await Promise.allSettled([
      getMetadataProviderStatus(),
      getMetadataProviderAccount(),
    ]);

    if (!mounted.current || readGeneration.current !== generation) return;

    if (statusResult.status === 'fulfilled') {
      setProviderStatus(statusResult.value);
      setProviderStatusError(null);
    } else {
      setProviderStatusError(normalizeIpcError(statusResult.reason));
    }
    setProviderStatusLoading(false);

    if (accountResult.status === 'fulfilled') {
      setAccount(accountResult.value);
      const returnedUsername = accountResult.value.username;
      if (returnedUsername) {
        setCredentialUsername((current) => current || returnedUsername);
      }
      setAccountError(null);
    } else {
      setAccountError(normalizeIpcError(accountResult.reason));
    }
    setAccountLoading(false);
  }, []);

  const saveCredentials = useCallback(async () => {
    if (!mounted.current || credentialsMutationPending.current) return false;

    credentialsMutationPending.current = true;
    setCredentialsPending(true);
    setAccountActionError(null);
    const username = credentialUsername;
    const password = credentialPassword;

    try {
      await setMetadataProviderCredentials({ username, password });
      if (!mounted.current) return false;
      setCredentialPassword('');
      await refresh();
      return true;
    } catch (reason: unknown) {
      if (mounted.current) {
        setCredentialPassword('');
        setAccountActionError(normalizeIpcError(reason));
      }
      return false;
    } finally {
      credentialsMutationPending.current = false;
      if (mounted.current) setCredentialsPending(false);
    }
  }, [credentialPassword, credentialUsername, refresh]);

  const clearCredentials = useCallback(async () => {
    if (!mounted.current || credentialsMutationPending.current) return false;

    credentialsMutationPending.current = true;
    setCredentialsPending(true);
    setAccountActionError(null);

    try {
      await clearMetadataProviderCredentials();
      if (!mounted.current) return false;
      setCredentialUsername('');
      setCredentialPassword('');
      await refresh();
      return true;
    } catch (reason: unknown) {
      if (mounted.current) setAccountActionError(normalizeIpcError(reason));
      return false;
    } finally {
      credentialsMutationPending.current = false;
      if (mounted.current) setCredentialsPending(false);
    }
  }, [refresh]);

  useEffect(() => {
    let disposed = false;
    mounted.current = true;
    void Promise.resolve().then(() => {
      if (!disposed && mounted.current) void refresh();
    });
    return () => {
      disposed = true;
      mounted.current = false;
      readGeneration.current += 1;
    };
  }, [refresh]);

  return {
    providerStatus,
    account,
    providerStatusLoading,
    accountLoading,
    providerStatusError,
    accountError,
    credentialUsername,
    credentialPassword,
    setCredentialUsername,
    setCredentialPassword,
    credentialsPending,
    accountActionError,
    refresh,
    saveCredentials,
    clearCredentials,
  };
}
