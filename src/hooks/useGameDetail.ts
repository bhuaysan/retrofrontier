import { useCallback, useEffect, useRef, useState } from 'react';

import {
  clearGameMetadataCandidate,
  getGameMetadata,
  getLibraryGameDetail,
  normalizeIpcError,
  onMetadataStateChanged,
  refreshGameMetadata,
  requestGameMetadata,
  selectGameMetadataCandidate,
  setGameFavorite,
  type GameMetadataState,
  type IpcError,
  type LibraryGameDetail,
} from '../platform/ipc';

const METADATA_INVALIDATION_DEBOUNCE_MS = 180;

interface UseGameDetailOptions {
  enabled: boolean;
  gameId: number | null;
  scanCompletionRunId?: number | null;
  onFavoriteCommitted?: () => void | Promise<void>;
}

interface DetailChannel<T> {
  key: string;
  value: T | null;
  loaded: boolean;
  loading: boolean;
  error: IpcError | null;
}

interface FavoriteChannel {
  key: string;
  pending: boolean;
  error: IpcError | null;
}

export type MetadataOperationKind = 'request' | 'refresh' | 'select' | 'clear';

interface MetadataOperationChannel {
  key: string;
  pending: boolean;
  kind: MetadataOperationKind | null;
  error: IpcError | null;
}

function emptyDetailChannel<T>(key: string, loading = false): DetailChannel<T> {
  return { key, value: null, loaded: false, loading, error: null };
}

function emptyFavoriteChannel(key: string): FavoriteChannel {
  return { key, pending: false, error: null };
}

function emptyMetadataOperationChannel(key: string): MetadataOperationChannel {
  return { key, pending: false, kind: null, error: null };
}

export interface GameDetailModel {
  localDetail: LibraryGameDetail | null;
  metadata: GameMetadataState | null;
  localLoaded: boolean;
  metadataLoaded: boolean;
  localLoading: boolean;
  metadataLoading: boolean;
  localError: IpcError | null;
  metadataError: IpcError | null;
  favoritePending: boolean;
  favoriteError: IpcError | null;
  metadataActionPending: boolean;
  metadataActionKind: MetadataOperationKind | null;
  metadataActionError: IpcError | null;
  refresh: () => Promise<void>;
  retryLocal: () => Promise<void>;
  retryMetadata: () => Promise<void>;
  requestMetadata: () => Promise<void>;
  refreshMetadata: () => Promise<void>;
  selectMetadataCandidate: (providerGameId: string) => Promise<void>;
  clearMetadataSelection: () => Promise<void>;
  toggleFavorite: () => Promise<void>;
}

export function useGameDetail({
  enabled,
  gameId,
  scanCompletionRunId = null,
  onFavoriteCommitted,
}: UseGameDetailOptions): GameDetailModel {
  const mounted = useRef(true);
  const localRequestGeneration = useRef(0);
  const metadataRequestGeneration = useRef(0);
  const lastScanKey = useRef<string | null>(null);
  const latestScanCompletionRunId = useRef(scanCompletionRunId);
  const detailKey = `${enabled ? 'enabled' : 'disabled'}:${gameId ?? 'invalid'}`;
  const favoriteOperation = useRef({ key: detailKey, pending: false });
  const metadataOperation = useRef({ key: detailKey, pending: false });
  const favoriteCommittedHandler = useRef(onFavoriteCommitted);
  const initialLoading = enabled && gameId !== null;

  // AppShell keeps this hook mounted while routes change. Keying each channel prevents a
  // previous game's data from being rendered during the next game's bounded fetch without a
  // synchronous state-reset effect.
  const [localChannelState, setLocalChannelState] = useState<DetailChannel<LibraryGameDetail>>(() =>
    emptyDetailChannel(detailKey, initialLoading),
  );
  const [metadataChannelState, setMetadataChannelState] = useState<
    DetailChannel<GameMetadataState>
  >(() => emptyDetailChannel(detailKey, initialLoading));
  const [favoriteChannelState, setFavoriteChannelState] = useState<FavoriteChannel>(() =>
    emptyFavoriteChannel(detailKey),
  );
  const [metadataOperationState, setMetadataOperationState] = useState<MetadataOperationChannel>(
    () => emptyMetadataOperationChannel(detailKey),
  );

  const localChannel =
    localChannelState.key === detailKey
      ? localChannelState
      : emptyDetailChannel<LibraryGameDetail>(detailKey, initialLoading);
  const metadataChannel =
    metadataChannelState.key === detailKey
      ? metadataChannelState
      : emptyDetailChannel<GameMetadataState>(detailKey, initialLoading);
  const favoriteChannel =
    favoriteChannelState.key === detailKey ? favoriteChannelState : emptyFavoriteChannel(detailKey);
  const metadataOperationChannel =
    metadataOperationState.key === detailKey
      ? metadataOperationState
      : emptyMetadataOperationChannel(detailKey);

  useEffect(() => {
    favoriteCommittedHandler.current = onFavoriteCommitted;
  }, [onFavoriteCommitted]);

  useEffect(() => {
    latestScanCompletionRunId.current = scanCompletionRunId;
  }, [scanCompletionRunId]);

  const loadLocal = useCallback(async () => {
    if (!enabled || gameId === null) return;

    const generation = localRequestGeneration.current + 1;
    localRequestGeneration.current = generation;
    if (mounted.current) {
      setLocalChannelState((current) => {
        const base =
          current.key === detailKey ? current : emptyDetailChannel<LibraryGameDetail>(detailKey);
        return { ...base, loading: true, error: null };
      });
    }

    try {
      const nextDetail = await getLibraryGameDetail({ gameId });
      if (mounted.current && localRequestGeneration.current === generation) {
        setLocalChannelState((current) =>
          current.key === detailKey
            ? { ...current, value: nextDetail, loaded: true, error: null }
            : current,
        );
      }
    } catch (reason: unknown) {
      if (mounted.current && localRequestGeneration.current === generation) {
        const error = normalizeIpcError(reason);
        setLocalChannelState((current) =>
          current.key === detailKey ? { ...current, loaded: true, error } : current,
        );
      }
    } finally {
      if (mounted.current && localRequestGeneration.current === generation) {
        setLocalChannelState((current) =>
          current.key === detailKey ? { ...current, loading: false } : current,
        );
      }
    }
  }, [detailKey, enabled, gameId]);

  const loadMetadata = useCallback(async () => {
    if (!enabled || gameId === null) return;

    const generation = metadataRequestGeneration.current + 1;
    metadataRequestGeneration.current = generation;
    if (mounted.current) {
      setMetadataChannelState((current) => {
        const base =
          current.key === detailKey ? current : emptyDetailChannel<GameMetadataState>(detailKey);
        return { ...base, loading: true, error: null };
      });
    }

    try {
      const nextMetadata = await getGameMetadata({ gameId });
      if (mounted.current && metadataRequestGeneration.current === generation) {
        setMetadataChannelState((current) =>
          current.key === detailKey
            ? { ...current, value: nextMetadata, loaded: true, error: null }
            : current,
        );
        setMetadataOperationState((current) =>
          current.key === detailKey ? { ...current, error: null } : current,
        );
      }
    } catch (reason: unknown) {
      if (mounted.current && metadataRequestGeneration.current === generation) {
        const error = normalizeIpcError(reason);
        setMetadataChannelState((current) =>
          current.key === detailKey ? { ...current, loaded: true, error } : current,
        );
      }
    } finally {
      if (mounted.current && metadataRequestGeneration.current === generation) {
        setMetadataChannelState((current) =>
          current.key === detailKey ? { ...current, loading: false } : current,
        );
      }
    }
  }, [detailKey, enabled, gameId]);

  useEffect(() => {
    localRequestGeneration.current += 1;
    metadataRequestGeneration.current += 1;
    favoriteOperation.current = { key: detailKey, pending: false };
    metadataOperation.current = { key: detailKey, pending: false };
    if (enabled && gameId !== null && latestScanCompletionRunId.current !== null) {
      lastScanKey.current = `${gameId}:${latestScanCompletionRunId.current}`;
    }
    let disposed = false;

    void Promise.resolve().then(() => {
      if (disposed || !enabled || gameId === null) return;
      void loadLocal();
      void loadMetadata();
    });

    return () => {
      disposed = true;
      localRequestGeneration.current += 1;
      metadataRequestGeneration.current += 1;
    };
  }, [detailKey, enabled, gameId, loadLocal, loadMetadata]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      localRequestGeneration.current += 1;
      metadataRequestGeneration.current += 1;
    };
  }, []);

  useEffect(() => {
    if (!enabled || gameId === null) return;

    const scanKey = scanCompletionRunId === null ? null : `${gameId}:${scanCompletionRunId}`;
    if (scanKey === null || lastScanKey.current === scanKey) return;
    lastScanKey.current = scanKey;
    void loadLocal();
    void loadMetadata();
  }, [enabled, gameId, loadLocal, loadMetadata, scanCompletionRunId]);

  useEffect(() => {
    if (!enabled || gameId === null) return;

    let disposed = false;
    let unlisten: (() => void) | undefined;
    let timer: number | undefined;
    const subscription = onMetadataStateChanged(({ gameId: changedGameId }) => {
      if (disposed || changedGameId !== gameId) return;
      if (timer !== undefined) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        timer = undefined;
        if (!disposed) void loadMetadata();
      }, METADATA_INVALIDATION_DEBOUNCE_MS);
    })
      .then((nextUnlisten) => {
        if (disposed) nextUnlisten();
        else unlisten = nextUnlisten;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      if (timer !== undefined) window.clearTimeout(timer);
      unlisten?.();
      void subscription;
    };
  }, [enabled, gameId, loadMetadata]);

  const refresh = useCallback(async () => {
    await Promise.allSettled([loadLocal(), loadMetadata()]);
  }, [loadLocal, loadMetadata]);

  const runMetadataMutation = useCallback(
    async (kind: MetadataOperationKind, command: () => Promise<void>) => {
      if (!enabled || gameId === null) return;

      if (metadataOperation.current.key !== detailKey) {
        metadataOperation.current = { key: detailKey, pending: false };
      }
      if (metadataOperation.current.pending) return;

      metadataOperation.current.pending = true;
      const operationKey = detailKey;
      if (mounted.current) {
        setMetadataOperationState((current) => {
          const base =
            current.key === detailKey ? current : emptyMetadataOperationChannel(detailKey);
          return { ...base, pending: true, kind, error: null };
        });
      }

      try {
        await command();
        if (!mounted.current || metadataOperation.current.key !== operationKey) return;
        // Clear-selection has no durable metadata event. A bounded authoritative read after
        // every successful command also keeps fast local feedback independent of event timing;
        // the existing metadata-state-changed listener remains authoritative for worker writes.
        await loadMetadata();
      } catch (reason: unknown) {
        if (mounted.current && metadataOperation.current.key === operationKey) {
          const error = normalizeIpcError(reason);
          setMetadataOperationState((current) =>
            current.key === operationKey ? { ...current, error } : current,
          );
        }
      } finally {
        if (metadataOperation.current.key === operationKey) {
          metadataOperation.current.pending = false;
          if (mounted.current) {
            setMetadataOperationState((current) =>
              current.key === operationKey ? { ...current, pending: false, kind: null } : current,
            );
          }
        }
      }
    },
    [detailKey, enabled, gameId, loadMetadata],
  );

  const requestMetadata = useCallback(
    () => runMetadataMutation('request', () => requestGameMetadata({ gameId: gameId! })),
    [gameId, runMetadataMutation],
  );

  const refreshMetadata = useCallback(
    () => runMetadataMutation('refresh', () => refreshGameMetadata({ gameId: gameId! })),
    [gameId, runMetadataMutation],
  );

  const selectMetadataCandidate = useCallback(
    (providerGameId: string) =>
      runMetadataMutation('select', () =>
        selectGameMetadataCandidate({ gameId: gameId!, providerGameId }),
      ),
    [gameId, runMetadataMutation],
  );

  const clearMetadataSelection = useCallback(
    () => runMetadataMutation('clear', () => clearGameMetadataCandidate({ gameId: gameId! })),
    [gameId, runMetadataMutation],
  );

  const toggleFavorite = useCallback(async () => {
    if (!enabled || gameId === null || localChannel.value === null) {
      return;
    }

    if (favoriteOperation.current.key !== detailKey) {
      favoriteOperation.current = { key: detailKey, pending: false };
    }
    if (favoriteOperation.current.pending) return;

    favoriteOperation.current.pending = true;
    if (mounted.current) {
      setFavoriteChannelState((current) => {
        const base = current.key === detailKey ? current : emptyFavoriteChannel(detailKey);
        return { ...base, pending: true, error: null };
      });
    }
    const operationKey = detailKey;

    try {
      const result = await setGameFavorite({
        gameId,
        favorite: !localChannel.value.favorite,
      });
      if (!mounted.current || favoriteOperation.current.key !== operationKey) {
        return;
      }
      setLocalChannelState((current) =>
        current.key === operationKey && current.value?.gameId === result.gameId
          ? { ...current, value: { ...current.value, favorite: result.favorite } }
          : current,
      );
      try {
        await favoriteCommittedHandler.current?.();
      } catch {
        // A summary refresh must not turn a committed favorite into a false failure.
      }
    } catch (reason: unknown) {
      if (mounted.current && favoriteOperation.current.key === operationKey) {
        const error = normalizeIpcError(reason);
        setFavoriteChannelState((current) =>
          current.key === operationKey ? { ...current, error } : current,
        );
      }
    } finally {
      if (favoriteOperation.current.key === operationKey) {
        favoriteOperation.current.pending = false;
        if (mounted.current) {
          setFavoriteChannelState((current) =>
            current.key === operationKey ? { ...current, pending: false } : current,
          );
        }
      }
    }
  }, [detailKey, enabled, gameId, localChannel.value]);

  return {
    localDetail: localChannel.value,
    metadata: metadataChannel.value,
    localLoaded: localChannel.loaded,
    metadataLoaded: metadataChannel.loaded,
    localLoading: localChannel.loading,
    metadataLoading: metadataChannel.loading,
    localError: localChannel.error,
    metadataError: metadataChannel.error,
    favoritePending: favoriteChannel.pending,
    favoriteError: favoriteChannel.error,
    metadataActionPending: metadataOperationChannel.pending,
    metadataActionKind: metadataOperationChannel.kind,
    metadataActionError: metadataOperationChannel.error,
    refresh,
    retryLocal: loadLocal,
    retryMetadata: loadMetadata,
    requestMetadata,
    refreshMetadata,
    selectMetadataCandidate,
    clearMetadataSelection,
    toggleFavorite,
  };
}
