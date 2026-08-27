import { useCallback, useEffect, useRef, useState } from 'react';

import {
  getLibrarySummary,
  normalizeIpcError,
  type IpcError,
  type LibrarySummary,
} from '../platform/ipc';

export function useLibrarySummary() {
  const mounted = useRef(true);
  const [summary, setSummary] = useState<LibrarySummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<IpcError | null>(null);

  const refresh = useCallback(async () => {
    if (mounted.current) {
      setLoading(true);
      setError(null);
    }

    try {
      const nextSummary = await getLibrarySummary();
      if (mounted.current) {
        setSummary(nextSummary);
        setError(null);
      }
      return nextSummary;
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

  useEffect(() => {
    mounted.current = true;
    void Promise.resolve().then(() => refresh());
    return () => {
      mounted.current = false;
    };
  }, [refresh]);

  return { summary, loading, error, refresh };
}
