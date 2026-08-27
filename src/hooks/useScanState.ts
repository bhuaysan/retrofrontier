import { useCallback, useEffect, useRef, useState } from 'react';

import {
  getScanIssuePage,
  getScanStatus,
  normalizeIpcError,
  onLibraryScanCompleted,
  onLibraryScanProgress,
  rescanLibrary,
  type IpcError,
  type ScanIssuePage,
  type ScanProgress,
  type ScanStatus,
  type ScanSummary,
} from '../platform/ipc';

const ISSUE_PAGE_SIZE = 50;

interface UseScanStateOptions {
  onCompleted?: (summary: ScanSummary) => void;
}

export interface ScanStateModel {
  status: ScanStatus | null;
  statusLoading: boolean;
  statusError: IpcError | null;
  issuePage: ScanIssuePage | null;
  issueLoading: boolean;
  issueLoadingMore: boolean;
  issueError: IpcError | null;
  issueLoadMoreError: IpcError | null;
  scanStartPending: boolean;
  scanStartError: IpcError | null;
  refreshStatus: () => Promise<void>;
  refreshIssues: () => Promise<void>;
  loadMoreIssues: () => Promise<void>;
  startScan: () => Promise<ScanSummary | null>;
}

export function useScanState({ onCompleted }: UseScanStateOptions = {}): ScanStateModel {
  const mounted = useRef(true);
  const completionHandler = useRef(onCompleted);
  const eventVersion = useRef(0);
  const handledCompletionRunId = useRef<number | null>(null);
  const statusRequestStarted = useRef(false);
  const issueRequestVersion = useRef(0);

  const [status, setStatus] = useState<ScanStatus | null>(null);
  const [statusLoading, setStatusLoading] = useState(true);
  const [statusError, setStatusError] = useState<IpcError | null>(null);
  const [eventError, setEventError] = useState<IpcError | null>(null);
  const [issuePage, setIssuePage] = useState<ScanIssuePage | null>(null);
  const [issueLoading, setIssueLoading] = useState(true);
  const [issueLoadingMore, setIssueLoadingMore] = useState(false);
  const [issueError, setIssueError] = useState<IpcError | null>(null);
  const [issueLoadMoreError, setIssueLoadMoreError] = useState<IpcError | null>(null);
  const [scanStartPending, setScanStartPending] = useState(false);
  const [scanStartError, setScanStartError] = useState<IpcError | null>(null);

  useEffect(() => {
    completionHandler.current = onCompleted;
  }, [onCompleted]);

  const refreshIssues = useCallback(async () => {
    const requestVersion = issueRequestVersion.current + 1;
    issueRequestVersion.current = requestVersion;
    if (mounted.current) {
      setIssueLoading(true);
      setIssueError(null);
      setIssueLoadMoreError(null);
    }

    try {
      const nextPage = await getScanIssuePage({ offset: 0, limit: ISSUE_PAGE_SIZE });
      if (mounted.current && issueRequestVersion.current === requestVersion) {
        setIssuePage(nextPage);
        setIssueError(null);
        setIssueLoadMoreError(null);
      }
    } catch (reason: unknown) {
      if (mounted.current && issueRequestVersion.current === requestVersion) {
        setIssueError(normalizeIpcError(reason));
      }
    } finally {
      if (mounted.current) {
        setIssueLoading(false);
      }
    }
  }, []);

  const handleCompleted = useCallback(
    (summary: ScanSummary) => {
      eventVersion.current += 1;
      if (!mounted.current) {
        return;
      }

      setStatus({ running: false, progress: null, lastResult: summary });
      setStatusError(null);
      setScanStartError(null);

      if (handledCompletionRunId.current === summary.runId) {
        return;
      }
      handledCompletionRunId.current = summary.runId;
      completionHandler.current?.(summary);
      void refreshIssues();
    },
    [refreshIssues],
  );

  const handleProgress = useCallback((progress: ScanProgress) => {
    if (handledCompletionRunId.current === progress.runId) {
      return;
    }

    eventVersion.current += 1;
    if (!mounted.current) {
      return;
    }

    setStatus((current) => {
      if (current?.progress && current.progress.runId > progress.runId) {
        return current;
      }
      return {
        running: true,
        progress,
        lastResult: current?.lastResult ?? null,
      };
    });
    setStatusError(null);
    setScanStartError(null);
  }, []);

  const refreshStatus = useCallback(async () => {
    const requestVersion = eventVersion.current;
    if (mounted.current && !statusRequestStarted.current) {
      statusRequestStarted.current = true;
      setStatusLoading(true);
    }

    try {
      const nextStatus = await getScanStatus();
      if (mounted.current) {
        setStatusError(null);
        setStatus((current) =>
          eventVersion.current === requestVersion ? nextStatus : (current ?? nextStatus),
        );
      }
    } catch (reason: unknown) {
      if (mounted.current) {
        setStatusError(normalizeIpcError(reason));
      }
    } finally {
      if (mounted.current) {
        setStatusLoading(false);
      }
    }
  }, []);

  const loadMoreIssues = useCallback(async () => {
    if (!issuePage || issueLoadingMore || issuePage.issues.length >= issuePage.total) {
      return;
    }

    const expectedOffset = issuePage.issues.length;
    const requestVersion = issueRequestVersion.current + 1;
    issueRequestVersion.current = requestVersion;
    setIssueLoadingMore(true);
    setIssueLoadMoreError(null);

    try {
      const nextPage = await getScanIssuePage({
        offset: expectedOffset,
        limit: issuePage.limit || ISSUE_PAGE_SIZE,
      });
      if (mounted.current && issueRequestVersion.current === requestVersion) {
        setIssuePage((current) => {
          if (
            !current ||
            current.scanRunId !== nextPage.scanRunId ||
            current.issues.length !== expectedOffset
          ) {
            return current;
          }
          return {
            ...nextPage,
            offset: current.offset,
            issues: [...current.issues, ...nextPage.issues],
          };
        });
      }
    } catch (reason: unknown) {
      if (mounted.current && issueRequestVersion.current === requestVersion) {
        setIssueLoadMoreError(normalizeIpcError(reason));
      }
    } finally {
      if (mounted.current) {
        setIssueLoadingMore(false);
      }
    }
  }, [issueLoadingMore, issuePage]);

  const startScan = useCallback(async () => {
    if (mounted.current) {
      setScanStartPending(true);
      setScanStartError(null);
    }

    try {
      const result = await rescanLibrary();
      if (mounted.current) {
        if (result.state === 'completed' || result.state === 'failed') {
          handleCompleted(result);
        } else {
          void refreshStatus();
        }
      }
      return result;
    } catch (reason: unknown) {
      const error = normalizeIpcError(reason);
      if (mounted.current) {
        setScanStartError(error);
      }
      return null;
    } finally {
      if (mounted.current) {
        setScanStartPending(false);
      }
    }
  }, [handleCompleted, refreshStatus]);

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let progressUnlisten: (() => void) | undefined;
    let completedUnlisten: (() => void) | undefined;

    const progressSubscription = onLibraryScanProgress(handleProgress)
      .then((unlisten) => {
        if (!disposed) {
          progressUnlisten = unlisten;
        } else {
          unlisten();
        }
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setEventError(normalizeIpcError(reason));
        }
      });

    const completedSubscription = onLibraryScanCompleted(handleCompleted)
      .then((unlisten) => {
        if (!disposed) {
          completedUnlisten = unlisten;
        } else {
          unlisten();
        }
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setEventError(normalizeIpcError(reason));
        }
      });

    void Promise.allSettled([progressSubscription, completedSubscription]).then(() => {
      if (disposed) {
        return;
      }
      void refreshStatus();
      void refreshIssues();
    });

    return () => {
      disposed = true;
      mounted.current = false;
      progressUnlisten?.();
      completedUnlisten?.();
    };
  }, [handleCompleted, handleProgress, refreshIssues, refreshStatus]);

  return {
    status,
    statusLoading,
    statusError: statusError ?? eventError,
    issuePage,
    issueLoading,
    issueLoadingMore,
    issueError,
    issueLoadMoreError,
    scanStartPending,
    scanStartError,
    refreshStatus,
    refreshIssues,
    loadMoreIssues,
    startScan,
  };
}
