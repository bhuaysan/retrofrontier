import { useCallback, useEffect, useRef, useState } from 'react';

import {
  addExternalContentRoot,
  getContentRoots,
  normalizeIpcError,
  removeExternalContentRoot,
  setContentRootEnabled,
  type ContentRoot,
  type IpcError,
} from '../platform/ipc';

export function useContentRoots() {
  const mounted = useRef(true);
  const [roots, setRoots] = useState<ContentRoot[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<IpcError | null>(null);

  const refresh = useCallback(async () => {
    if (mounted.current) {
      setLoading(true);
      setError(null);
    }

    try {
      const nextRoots = await getContentRoots();
      if (mounted.current) {
        setRoots(nextRoots);
        setError(null);
      }
      return nextRoots;
    } catch (reason: unknown) {
      if (mounted.current) {
        setError(normalizeIpcError(reason));
      }
      return null;
    } finally {
      if (mounted.current) {
        setLoading(false);
      }
    }
  }, []);

  const addExternalRoot = useCallback(
    async (path: string) => {
      const root = await addExternalContentRoot({ path });
      await refresh();
      return root;
    },
    [refresh],
  );

  const removeExternalRoot = useCallback(
    async (rootId: number) => {
      await removeExternalContentRoot({ rootId });
      await refresh();
    },
    [refresh],
  );

  const updateRootEnabled = useCallback(
    async (rootId: number, enabled: boolean) => {
      const root = await setContentRootEnabled({ rootId, enabled });
      await refresh();
      return root;
    },
    [refresh],
  );

  useEffect(() => {
    mounted.current = true;
    void Promise.resolve().then(() => refresh());
    return () => {
      mounted.current = false;
    };
  }, [refresh]);

  return {
    roots,
    loading,
    error,
    refresh,
    addExternalRoot,
    removeExternalRoot,
    updateRootEnabled,
  };
}
