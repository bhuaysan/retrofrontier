import { useCallback, useEffect, useRef, useState } from 'react';

import {
  deleteSaveState,
  listSaveStates,
  loadSaveState,
  normalizeIpcError,
  type IpcError,
  type SaveStateFailure,
  type SaveStateView,
} from '../platform/ipc';

interface UseSaveStatesOptions {
  enabled: boolean;
  gameId: number | null;
  /** A managed process record exists whose identity could not be established. */
  launchBlocked: boolean;
  /** The backend reports a managed game as running. Never inferred here. */
  launchRunning: boolean;
  /** A launch request this frontend issued has not resolved yet. */
  launchPending: boolean;
}

/**
 * One channel for the whole Save-State surface.
 *
 * The list and the pending action deliberately share a single state object, because a successful
 * delete has to publish the reloaded list *and* the cleared pending id in one commit. Split state
 * would make the two visible in either order, and the section's post-delete focus decision — is the
 * removed row really gone? — would then read a list that has not caught up yet.
 */
interface SaveStatesChannel {
  key: string;
  states: SaveStateView[];
  loaded: boolean;
  loading: boolean;
  error: IpcError | null;
  loadPendingId: number | null;
  deletePendingId: number | null;
  actionFailure: SaveStateFailure | null;
}

export interface SaveStatesModel {
  states: SaveStateView[];
  loaded: boolean;
  loading: boolean;
  error: IpcError | null;
  /** The state whose delete request is in flight, or null. */
  deletePendingId: number | null;
  /** The normalized failure of the last load or delete attempt, or null. */
  actionFailure: SaveStateFailure | null;
  loadPendingId: number | null;
  retry: () => Promise<void>;
  load: (saveStateId: number) => Promise<void>;
  delete: (saveStateId: number) => Promise<void>;
  dismissActionFailure: () => void;
}

function emptyChannel(key: string, loading = false): SaveStatesChannel {
  return {
    key,
    states: [],
    loaded: false,
    loading,
    error: null,
    loadPendingId: null,
    deletePendingId: null,
    actionFailure: null,
  };
}

/**
 * Owns the Save-State projection of one Game Detail route.
 *
 * The backend is the only authority here: the list arrives in the order the backend chose and is
 * never re-sorted, every capability value is a UI snapshot rather than a permission this hook may
 * act on, and no action is ever attempted from a locally inferred state. The launch facts are
 * passed in rather than read again, so there is exactly one place in the application that decides
 * whether a managed game is in the way.
 */
export function useSaveStates({
  enabled,
  gameId,
  launchBlocked,
  launchRunning,
  launchPending,
}: UseSaveStatesOptions): SaveStatesModel {
  const mounted = useRef(true);
  const listGeneration = useRef(0);
  const channelKey = `${enabled ? 'enabled' : 'disabled'}:${gameId ?? 'invalid'}`;
  // Whether an action of this hook is unresolved, kept in a ref because the guard has to reject a
  // second action inside the same tick, before any state update could have been committed.
  const actionPending = useRef(false);
  const initialLoading = enabled && gameId !== null;

  // AppShell keeps this hook mounted across route changes, so each game's data is keyed. A late
  // response for the previous game can then neither be rendered nor mistaken for this game's.
  const [channelState, setChannelState] = useState<SaveStatesChannel>(() =>
    emptyChannel(channelKey, initialLoading),
  );
  const channel =
    channelState.key === channelKey ? channelState : emptyChannel(channelKey, initialLoading);

  /**
   * Whether a managed game is in the way.
   *
   * A load or a delete is not attempted at all in that state. The backend refuses both, and
   * spending a launch attempt or a destructive request to be told so would be a request made in
   * the knowledge that it cannot succeed.
   */
  const launchInTheWay = launchBlocked || launchRunning || launchPending;

  const loadStates = useCallback(async () => {
    if (!enabled || gameId === null) return;

    const generation = listGeneration.current + 1;
    listGeneration.current = generation;
    const owns = () => mounted.current && listGeneration.current === generation;
    if (mounted.current) {
      setChannelState((current) => {
        const base = current.key === channelKey ? current : emptyChannel(channelKey);
        return { ...base, loading: true, error: null };
      });
    }

    try {
      const states = await listSaveStates({ gameId });
      if (!owns()) return;
      setChannelState((current) =>
        current.key === channelKey
          ? { ...current, states, loaded: true, loading: false, error: null }
          : current,
      );
    } catch (reason: unknown) {
      if (!owns()) return;
      const error = normalizeIpcError(reason);
      setChannelState((current) =>
        current.key === channelKey ? { ...current, loaded: true, loading: false, error } : current,
      );
    }
  }, [channelKey, enabled, gameId]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      listGeneration.current += 1;
    };
  }, []);

  useEffect(() => {
    listGeneration.current += 1;
    actionPending.current = false;
    let disposed = false;

    void Promise.resolve().then(() => {
      if (disposed) return;
      void loadStates();
    });

    return () => {
      disposed = true;
      listGeneration.current += 1;
    };
  }, [loadStates]);

  // A managed session that just ended may have produced new states, and the backend is the only
  // thing that knows it ended. Nothing here is inferred from a timer or from a process check.
  const previousRunning = useRef(launchRunning);
  useEffect(() => {
    const wasRunning = previousRunning.current;
    previousRunning.current = launchRunning;
    if (wasRunning && !launchRunning) void loadStates();
  }, [launchRunning, loadStates]);

  /**
   * Runs one save-state action, or nothing.
   *
   * Only one load or delete may be unresolved at a time: both end in a durable backend decision,
   * and a second request issued before the first resolved would make that first answer irrelevant
   * to the UI while it stays entirely real on disk.
   */
  const runAction = useCallback(
    async (
      begin: (channel: SaveStatesChannel) => SaveStatesChannel,
      command: () => Promise<SaveStateFailure | 'reload' | null>,
    ) => {
      if (!enabled || gameId === null) return;
      if (launchInTheWay) return;
      if (actionPending.current) return;

      actionPending.current = true;
      const actionKey = channelKey;
      if (mounted.current) {
        setChannelState((current) => {
          const base = current.key === actionKey ? current : emptyChannel(actionKey);
          return begin({ ...base, actionFailure: null });
        });
      }

      let outcome: SaveStateFailure | 'reload' | null;
      try {
        outcome = await command();
      } catch (reason: unknown) {
        // Only a transport-level rejection reaches this branch: every anticipated save-state
        // problem is a status-tagged response. The generic message is the transport's own.
        outcome = { code: 'unavailable', message: normalizeIpcError(reason).message };
      }

      // A successful delete republishes the authoritative list, so the removed row and the cleared
      // pending id become visible in the same commit.
      const states =
        outcome === 'reload' ? await listSaveStates({ gameId }).catch(() => null) : null;
      actionPending.current = false;
      if (!mounted.current) return;
      const failure = outcome === 'reload' || outcome === null ? null : outcome;
      setChannelState((current) =>
        current.key === actionKey
          ? {
              ...current,
              states: states ?? current.states,
              loadPendingId: null,
              deletePendingId: null,
              actionFailure: failure,
            }
          : current,
      );
    },
    [channelKey, enabled, gameId, launchInTheWay],
  );

  const load = useCallback(
    (saveStateId: number) =>
      runAction(
        (current) => ({ ...current, loadPendingId: saveStateId }),
        async () => {
          const response = await loadSaveState({ saveStateId });
          switch (response.status) {
            case 'started':
              return null;
            case 'refused':
              // A Save-State verdict, already normalized and already carrying the code that says
              // which one it is. It is surfaced verbatim: re-coding it here would throw away the
              // difference between "this state is gone" and "the launch failed".
              return response.error;
            case 'launchFailed':
              // The launch pipeline's own verdict about a load it was allowed to attempt. Its
              // message is carried through unchanged under the one save-state code that describes
              // what happened, so nothing is parsed and no copy is invented.
              return { code: 'launchFailed', message: response.error.message };
          }
        },
      ),
    [runAction],
  );

  const remove = useCallback(
    (saveStateId: number) =>
      runAction(
        (current) => ({ ...current, deletePendingId: saveStateId }),
        async () => {
          const response = await deleteSaveState({ saveStateId });
          return response.status === 'deleted' ? 'reload' : response.error;
        },
      ),
    [runAction],
  );

  const dismissActionFailure = useCallback(() => {
    setChannelState((current) =>
      current.key === channelKey ? { ...current, actionFailure: null } : current,
    );
  }, [channelKey]);

  return {
    states: channel.states,
    loaded: channel.loaded,
    loading: channel.loading,
    error: channel.error,
    deletePendingId: channel.deletePendingId,
    actionFailure: channel.actionFailure,
    loadPendingId: channel.loadPendingId,
    retry: loadStates,
    load,
    delete: remove,
    dismissActionFailure,
  };
}
