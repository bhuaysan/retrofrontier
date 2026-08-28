import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

import {
  normalizeIpcError,
  onMetadataStateChanged,
  queryLibrary,
  setGameFavorite,
  type IpcError,
  type LibraryListItem,
  type LibraryPage,
  type LibraryQueryRequest,
  type SystemId,
} from '../platform/ipc';

const SEARCH_DEBOUNCE_MS = 200;
const METADATA_INVALIDATION_DEBOUNCE_MS = 180;

interface UseLibraryQueryOptions {
  enabled: boolean;
  scanCompletionRunId?: number | null;
  onFavoriteCommitted?: () => void | Promise<void>;
}

export interface LibraryQueryModel {
  searchInput: string;
  setSearchInput: (value: string) => void;
  systemId: SystemId | null;
  setSystemId: (systemId: SystemId | null) => void;
  favoritesOnly: boolean;
  setFavoritesOnly: (favoritesOnly: boolean) => void;
  page: LibraryPage | null;
  initialLoading: boolean;
  refreshing: boolean;
  pageLoading: boolean;
  error: IpcError | null;
  favoriteError: IpcError | null;
  favoritePendingIds: ReadonlySet<number>;
  retry: () => Promise<void>;
  clearSearch: () => void;
  resetQuery: () => void;
  previousPage: () => void;
  nextPage: () => void;
  toggleFavorite: (item: LibraryListItem) => Promise<void>;
}

type LoadingChannel = 'initial' | 'refresh' | 'page';

export function useLibraryQuery({
  enabled,
  scanCompletionRunId = null,
  onFavoriteCommitted,
}: UseLibraryQueryOptions): LibraryQueryModel {
  const mounted = useRef(true);
  const pageRef = useRef<LibraryPage | null>(null);
  const requestGeneration = useRef(0);
  const initialLoadingOwner = useRef(0);
  const refreshingOwner = useRef(0);
  const pageLoadingOwner = useRef(0);
  const favoritePendingRef = useRef(new Set<number>());
  const lastScanCompletionRunId = useRef<number | null>(null);
  // `page`/`pageRef` is the committed rendered position. The target is the latest logical
  // request position used by retries and bounded refreshes; it is not committed until success.
  const latestQueryState = useRef({ favoritesOnly: false, targetOffset: 0 });
  const queryIdentityRef = useRef<string | null>(null);
  const latestRunQuery = useRef<
    (requestedOffset?: number, requestedChannel?: LoadingChannel) => Promise<void>
  >(async () => {});
  const favoriteCommittedHandler = useRef(onFavoriteCommitted);

  const [searchInput, setSearchInputState] = useState('');
  const [debouncedSearch, setDebouncedSearch] = useState('');
  const [systemId, setSystemIdState] = useState<SystemId | null>(null);
  const [favoritesOnly, setFavoritesOnlyState] = useState(false);
  const [page, setPage] = useState<LibraryPage | null>(null);
  // The browser is only mounted for a populated summary, but `enabled` can turn
  // true after that summary resolves. Start owned by the initial-load channel so
  // the first committed browser frame cannot briefly appear idle and empty.
  const [initialLoading, setInitialLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [pageLoading, setPageLoading] = useState(false);
  const [error, setError] = useState<IpcError | null>(null);
  const [favoriteError, setFavoriteError] = useState<IpcError | null>(null);
  const [favoritePendingIds, setFavoritePendingIds] = useState<ReadonlySet<number>>(new Set());

  useEffect(() => {
    pageRef.current = page;
  }, [page]);

  useEffect(() => {
    favoriteCommittedHandler.current = onFavoriteCommitted;
  }, [onFavoriteCommitted]);

  const setChannelLoading = useCallback((channel: LoadingChannel, value: boolean) => {
    if (channel === 'initial') setInitialLoading(value);
    else if (channel === 'refresh') setRefreshing(value);
    else setPageLoading(value);
  }, []);

  const runQuery = useCallback(
    async (
      requestedOffset = latestQueryState.current.targetOffset,
      requestedChannel?: LoadingChannel,
    ) => {
      if (!enabled) return;

      latestQueryState.current.targetOffset = requestedOffset;
      const generation = requestGeneration.current + 1;
      requestGeneration.current = generation;
      const currentPage = pageRef.current;
      const channel =
        requestedChannel ??
        (currentPage === null
          ? 'initial'
          : requestedOffset !== currentPage.offset
            ? 'page'
            : 'refresh');
      if (channel === 'initial') initialLoadingOwner.current = generation;
      else if (channel === 'refresh') refreshingOwner.current = generation;
      else pageLoadingOwner.current = generation;

      if (mounted.current) {
        setChannelLoading(channel, true);
        setError(null);
      }

      const request: LibraryQueryRequest = { sort: 'titleAsc', offset: requestedOffset };
      if (debouncedSearch !== '') request.search = debouncedSearch;
      if (systemId !== null) request.systemId = systemId;
      if (favoritesOnly) request.favoritesOnly = true;

      try {
        const nextPage = await queryLibrary(request);
        if (mounted.current && requestGeneration.current === generation) {
          if (
            nextPage.total > 0 &&
            nextPage.items.length === 0 &&
            nextPage.offset >= nextPage.total
          ) {
            const effectiveLimit = Math.max(1, nextPage.limit);
            void latestRunQuery.current(
              Math.floor((nextPage.total - 1) / effectiveLimit) * effectiveLimit,
              'page',
            );
            return;
          }
          latestQueryState.current.targetOffset = nextPage.offset;
          pageRef.current = nextPage;
          setPage(nextPage);
          setError(null);
        }
      } catch (reason: unknown) {
        if (mounted.current && requestGeneration.current === generation) {
          setError(normalizeIpcError(reason));
        }
      } finally {
        const ownsLoading =
          (channel === 'initial' && initialLoadingOwner.current === generation) ||
          (channel === 'refresh' && refreshingOwner.current === generation) ||
          (channel === 'page' && pageLoadingOwner.current === generation);
        if (mounted.current && ownsLoading) setChannelLoading(channel, false);
      }
    },
    [debouncedSearch, enabled, favoritesOnly, setChannelLoading, systemId],
  );

  useLayoutEffect(() => {
    latestRunQuery.current = runQuery;
    const queryIdentity = `${debouncedSearch}\u0000${systemId ?? ''}\u0000${favoritesOnly}`;
    if (queryIdentityRef.current !== queryIdentity) {
      queryIdentityRef.current = queryIdentity;
      latestQueryState.current = { favoritesOnly, targetOffset: 0 };
    } else {
      latestQueryState.current.favoritesOnly = favoritesOnly;
    }
  }, [debouncedSearch, favoritesOnly, runQuery, systemId]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      requestGeneration.current += 1;
    };
  }, []);

  useEffect(() => {
    if (searchInput === debouncedSearch) return;
    const timer = window.setTimeout(() => {
      latestQueryState.current.targetOffset = 0;
      setDebouncedSearch(searchInput);
    }, SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [debouncedSearch, searchInput]);

  useEffect(() => {
    if (enabled) void runQuery();
  }, [enabled, runQuery]);

  const setSearchInput = useCallback((value: string) => setSearchInputState(value), []);
  const clearSearch = useCallback(() => {
    setSearchInputState('');
    setDebouncedSearch('');
    latestQueryState.current.targetOffset = 0;
  }, []);
  const setSystemId = useCallback((value: SystemId | null) => {
    setSystemIdState(value);
    latestQueryState.current.targetOffset = 0;
  }, []);
  const setFavoritesOnly = useCallback((value: boolean) => {
    setFavoritesOnlyState(value);
    latestQueryState.current.targetOffset = 0;
  }, []);
  const resetQuery = useCallback(() => {
    setSearchInputState('');
    setDebouncedSearch('');
    setSystemIdState(null);
    setFavoritesOnlyState(false);
    latestQueryState.current.targetOffset = 0;
  }, []);

  const previousPage = useCallback(() => {
    const current = pageRef.current;
    if (!current || current.offset <= 0 || pageLoading) return;
    void runQuery(Math.max(0, current.offset - current.limit), 'page');
  }, [pageLoading, runQuery]);

  const nextPage = useCallback(() => {
    const current = pageRef.current;
    if (!current || current.offset + current.items.length >= current.total || pageLoading) return;
    void runQuery(current.offset + current.limit, 'page');
  }, [pageLoading, runQuery]);

  const retry = useCallback(
    () => latestRunQuery.current(latestQueryState.current.targetOffset),
    [],
  );

  const toggleFavorite = useCallback(async (item: LibraryListItem) => {
    if (favoritePendingRef.current.has(item.gameId)) return;
    favoritePendingRef.current.add(item.gameId);
    setFavoritePendingIds(new Set(favoritePendingRef.current));
    setFavoriteError(null);
    try {
      await setGameFavorite({ gameId: item.gameId, favorite: !item.favorite });
      if (!mounted.current) return;
      const currentQuery = latestQueryState.current;
      const resetFilteredPage =
        currentQuery.favoritesOnly && item.favorite && currentQuery.targetOffset !== 0;
      await latestRunQuery.current(resetFilteredPage ? 0 : currentQuery.targetOffset, 'refresh');
      await favoriteCommittedHandler.current?.();
    } catch (reason: unknown) {
      if (mounted.current) setFavoriteError(normalizeIpcError(reason));
    } finally {
      favoritePendingRef.current.delete(item.gameId);
      if (mounted.current) setFavoritePendingIds(new Set(favoritePendingRef.current));
    }
  }, []);

  useEffect(() => {
    if (!enabled || scanCompletionRunId === null) return;
    if (lastScanCompletionRunId.current === scanCompletionRunId) return;
    lastScanCompletionRunId.current = scanCompletionRunId;
    if (pageRef.current === null) return;
    void latestRunQuery.current(latestQueryState.current.targetOffset, 'refresh');
  }, [enabled, runQuery, scanCompletionRunId]);

  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let timer: number | undefined;
    const affectedIds = new Set<number>();
    const subscription = onMetadataStateChanged(({ gameId }) => {
      if (disposed || !pageRef.current?.items.some((item) => item.gameId === gameId)) return;
      affectedIds.add(gameId);
      if (timer !== undefined) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        timer = undefined;
        if (disposed || affectedIds.size === 0) return;
        affectedIds.clear();
        void latestRunQuery.current(latestQueryState.current.targetOffset, 'refresh');
      }, METADATA_INVALIDATION_DEBOUNCE_MS);
    })
      .then((nextUnlisten) => {
        if (disposed) nextUnlisten();
        else unlisten = nextUnlisten;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      affectedIds.clear();
      if (timer !== undefined) window.clearTimeout(timer);
      unlisten?.();
      void subscription;
    };
  }, [enabled, runQuery]);

  return {
    searchInput,
    setSearchInput,
    systemId,
    setSystemId,
    favoritesOnly,
    setFavoritesOnly,
    page,
    initialLoading,
    refreshing,
    pageLoading,
    error,
    favoriteError,
    favoritePendingIds,
    retry,
    clearSearch,
    resetQuery,
    previousPage,
    nextPage,
    toggleFavorite,
  };
}
