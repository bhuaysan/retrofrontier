import { useCallback, useEffect, useRef, useState } from 'react';

import {
  getMetadataScrapeStatus,
  normalizeIpcError,
  previewMetadataScrape,
  startMetadataScrape,
  stopMetadataScrape,
  type IpcError,
  type MetadataScrapeMode,
  type MetadataScrapeStatus,
} from '../platform/ipc';

/**
 * How often an active run's aggregate status is re-read.
 *
 * Whole-run progress is deliberately *not* driven by per-game `metadata-state-changed` events: a
 * 20,000-game run would emit tens of thousands of them, and the screen only ever shows eight
 * numbers. One compact aggregate query per second costs a single indexed group-by and is bounded no
 * matter how large the run is. Per-game invalidation stays exactly where it belongs — Game Detail
 * and the visible Library page.
 */
export const SCRAPE_POLL_INTERVAL_MS = 1_000;

export interface MetadataScrapeModel {
  status: MetadataScrapeStatus | null;
  statusLoading: boolean;
  statusError: IpcError | null;
  mode: MetadataScrapeMode;
  setMode: (mode: MetadataScrapeMode) => void;
  /** Games the selected mode would target, or `null` while it is being counted. */
  eligibleGames: number | null;
  previewLoading: boolean;
  previewError: IpcError | null;
  actionPending: boolean;
  actionError: IpcError | null;
  /** True while a run still owns the provider. */
  active: boolean;
  start: () => Promise<boolean>;
  stop: () => Promise<boolean>;
  refresh: () => Promise<void>;
}

export function useMetadataScrape(): MetadataScrapeModel {
  const mounted = useRef(true);
  const previewGeneration = useRef(0);
  const actionInFlight = useRef(false);
  const [status, setStatus] = useState<MetadataScrapeStatus | null>(null);
  const [statusLoading, setStatusLoading] = useState(true);
  const [statusError, setStatusError] = useState<IpcError | null>(null);
  const [mode, setMode] = useState<MetadataScrapeMode>('missingMetadata');
  const [eligibleGames, setEligibleGames] = useState<number | null>(null);
  const [previewLoading, setPreviewLoading] = useState(true);
  const [previewError, setPreviewError] = useState<IpcError | null>(null);
  const [actionPending, setActionPending] = useState(false);
  const [actionError, setActionError] = useState<IpcError | null>(null);

  const active = status?.active ?? false;

  const readStatus = useCallback(async () => {
    try {
      const next = await getMetadataScrapeStatus();
      if (!mounted.current) return;
      setStatus(next);
      setStatusError(null);
    } catch (reason: unknown) {
      if (mounted.current) setStatusError(normalizeIpcError(reason));
    } finally {
      if (mounted.current) setStatusLoading(false);
    }
  }, []);

  const readPreview = useCallback(async (target: MetadataScrapeMode) => {
    const generation = previewGeneration.current + 1;
    previewGeneration.current = generation;
    if (mounted.current) {
      setPreviewLoading(true);
      setPreviewError(null);
    }

    try {
      const preview = await previewMetadataScrape({ mode: target });
      // A slower earlier request must never overwrite a newer mode's count.
      if (!mounted.current || previewGeneration.current !== generation) return;
      setEligibleGames(preview.eligibleGames);
      setPreviewError(null);
    } catch (reason: unknown) {
      if (!mounted.current || previewGeneration.current !== generation) return;
      setEligibleGames(null);
      setPreviewError(normalizeIpcError(reason));
    } finally {
      if (mounted.current && previewGeneration.current === generation) {
        setPreviewLoading(false);
      }
    }
  }, []);

  const refresh = useCallback(async () => {
    await Promise.all([readStatus(), readPreview(mode)]);
  }, [mode, readPreview, readStatus]);

  const runAction = useCallback(
    async (action: () => Promise<MetadataScrapeStatus>, nextMode: MetadataScrapeMode) => {
      if (!mounted.current || actionInFlight.current) return false;

      actionInFlight.current = true;
      setActionPending(true);
      setActionError(null);
      try {
        const next = await action();
        if (!mounted.current) return false;
        setStatus(next);
        setStatusError(null);
        // Eligibility moves the moment a run starts or stops, so the idle count is re-read rather
        // than left showing what was true before the action.
        void readPreview(nextMode);
        return true;
      } catch (reason: unknown) {
        if (mounted.current) setActionError(normalizeIpcError(reason));
        return false;
      } finally {
        actionInFlight.current = false;
        if (mounted.current) setActionPending(false);
      }
    },
    [readPreview],
  );

  const start = useCallback(
    () => runAction(() => startMetadataScrape({ mode }), mode),
    [mode, runAction],
  );

  const stop = useCallback(() => runAction(() => stopMetadataScrape(), mode), [mode, runAction]);

  useEffect(() => {
    mounted.current = true;
    void readStatus();
    return () => {
      mounted.current = false;
    };
  }, [readStatus]);

  useEffect(() => {
    void readPreview(mode);
  }, [mode, readPreview]);

  // Poll only while a run actually owns the provider. A completed or stopped run is a static
  // summary; continuing to poll it would query forever for an answer that cannot change.
  useEffect(() => {
    if (!active) return;
    const timer = window.setInterval(() => {
      void readStatus();
    }, SCRAPE_POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [active, readStatus]);

  return {
    status,
    statusLoading,
    statusError,
    mode,
    setMode,
    eligibleGames,
    previewLoading,
    previewError,
    actionPending,
    actionError,
    active,
    start,
    stop,
    refresh,
  };
}
