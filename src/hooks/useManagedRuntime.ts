import { useCallback, useEffect, useRef, useState } from 'react';

import {
  getRuntimeInstallState,
  installRuntime,
  normalizeIpcError,
  repairRuntime,
  type IpcError,
  type RuntimeInstallFailure,
  type RuntimeInstallState,
} from '../platform/ipc';

export interface ManagedRuntimeModel {
  state: RuntimeInstallState | null;
  loading: boolean;
  /** The state query itself failed. Distinct from an installation that ran and was refused. */
  stateError: IpcError | null;
  /** The last installation attempt's normalized refusal, or `null` after a success. */
  installError: RuntimeInstallFailure | null;
  pending: boolean;
  refresh: () => Promise<void>;
  install: () => Promise<void>;
  repair: () => Promise<void>;
}

/**
 * Owns the managed runtime's install state for Settings.
 *
 * Installation is a long, single-flight operation: the button is disabled while one is running,
 * and the authoritative state is re-read afterwards rather than inferred from the response alone,
 * so a partially successful attempt cannot leave the UI claiming something the backend does not.
 */
export function useManagedRuntime(): ManagedRuntimeModel {
  const mounted = useRef(true);
  const pendingRef = useRef(false);
  const [state, setState] = useState<RuntimeInstallState | null>(null);
  const [loading, setLoading] = useState(true);
  const [stateError, setStateError] = useState<IpcError | null>(null);
  const [installError, setInstallError] = useState<RuntimeInstallFailure | null>(null);
  const [pending, setPending] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const next = await getRuntimeInstallState();
      if (!mounted.current) {
        return;
      }
      setState(next);
      setStateError(null);
    } catch (error: unknown) {
      if (mounted.current) {
        setStateError(normalizeIpcError(error));
      }
    } finally {
      if (mounted.current) {
        setLoading(false);
      }
    }
  }, []);

  // Deferring the first read out of the effect body keeps the initial fetch from setting state
  // synchronously during render, which is the pattern the other IPC-backed hooks already use.
  useEffect(() => {
    let disposed = false;
    mounted.current = true;
    void Promise.resolve().then(() => {
      if (!disposed && mounted.current) void refresh();
    });
    return () => {
      disposed = true;
      mounted.current = false;
    };
  }, [refresh]);

  const run = useCallback(
    async (operation: () => Promise<{ error: RuntimeInstallFailure | null }>) => {
      if (pendingRef.current) {
        return;
      }
      pendingRef.current = true;
      setPending(true);
      setInstallError(null);
      try {
        const response = await operation();
        if (mounted.current) {
          setInstallError(response.error);
        }
      } catch (error: unknown) {
        if (mounted.current) {
          setStateError(normalizeIpcError(error));
        }
      } finally {
        pendingRef.current = false;
        if (mounted.current) {
          setPending(false);
        }
        // Always re-read: only the backend knows whether the managed tree ended up verified.
        await refresh();
      }
    },
    [refresh],
  );

  const install = useCallback(() => run(installRuntime), [run]);
  const repair = useCallback(() => run(repairRuntime), [run]);

  return {
    state,
    loading,
    stateError,
    installError,
    pending,
    refresh,
    install,
    repair,
  };
}
