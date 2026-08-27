import { useCallback, useEffect, useRef, useState } from 'react';

import { getSystems, normalizeIpcError, type IpcError, type SystemId } from '../platform/ipc';

export interface SystemLabel {
  id: SystemId;
  displayName: string;
}

export function useSystemCatalog() {
  const mounted = useRef(true);
  const [systems, setSystems] = useState<SystemLabel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<IpcError | null>(null);

  const refresh = useCallback(async () => {
    if (mounted.current) {
      setSystems([]);
      setLoading(true);
      setError(null);
    }

    try {
      const response = await getSystems();
      const nextSystems = response.systems.map(({ id, displayName }) => ({ id, displayName }));
      if (mounted.current) {
        setSystems(nextSystems);
        setError(null);
      }
      return nextSystems;
    } catch (reason: unknown) {
      if (mounted.current) {
        setSystems([]);
        setError(normalizeIpcError(reason));
      }
      return null;
    } finally {
      if (mounted.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void Promise.resolve().then(() => refresh());

    return () => {
      mounted.current = false;
    };
  }, [refresh]);

  return { systems, loading, error, refresh };
}
