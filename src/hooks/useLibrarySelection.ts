import { useCallback, useMemo, useState } from 'react';

import type { LibraryListItem } from '../platform/ipc';

const EMPTY_SELECTION: ReadonlySet<number> = new Set<number>();

export interface LibrarySelectionModel {
  selectedIds: ReadonlySet<number>;
  count: number;
  isSelected: (gameId: number) => boolean;
  toggle: (gameId: number) => void;
  clear: () => void;
}

/**
 * B1 multi-select for the Library grid.
 *
 * Selection is transient frontend presentation state, not Favorites and not a domain state: it is
 * never persisted, never sent over IPC, and never written into `game_user_state`. Identity is the
 * authoritative `gameId`.
 *
 * Lifetime is scoped to the committed visible result set. Any committed result that no longer
 * contains a selected game drops it, which keeps search commits, system/favorite filter changes,
 * filter reset, and page navigation from leaving an invisible "N SELECTED" behind — without
 * inventing cross-page bulk-selection semantics. A refresh that returns the same games (metadata
 * invalidation, scan completion) keeps the selection, because those games are still visible.
 *
 * The input is the visible items rather than a page, because "what is on screen" is the only thing
 * this reconciliation ever reads. That lets the paginated grid and the All Systems shelves share
 * one selection model instead of growing a second one with subtly different lifetime rules.
 */
export function useLibrarySelection(
  visibleItems: readonly LibraryListItem[] | null,
): LibrarySelectionModel {
  const [selectedIds, setSelectedIds] = useState<ReadonlySet<number>>(EMPTY_SELECTION);
  const [reconciledAgainst, setReconciledAgainst] = useState<ReadonlySet<number> | null>(null);

  const visibleIds = useMemo(
    () => (visibleItems === null ? null : new Set(visibleItems.map((item) => item.gameId))),
    [visibleItems],
  );

  // Reconciled during render, not in an effect, so a newly committed result set can never paint a
  // stale count for a frame, and a game that leaves and later returns to the visible page cannot
  // resurrect an old selection.
  if (visibleIds !== reconciledAgainst) {
    setReconciledAgainst(visibleIds);
    setSelectedIds((current) => {
      if (current.size === 0) return current;
      if (visibleIds === null) return EMPTY_SELECTION;
      const next = new Set<number>();
      let dropped = false;
      for (const gameId of current) {
        if (visibleIds.has(gameId)) next.add(gameId);
        else dropped = true;
      }
      return dropped ? next : current;
    });
  }

  const isSelected = useCallback((gameId: number) => selectedIds.has(gameId), [selectedIds]);

  const toggle = useCallback((gameId: number) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (!next.delete(gameId)) next.add(gameId);
      return next;
    });
  }, []);

  const clear = useCallback(() => setSelectedIds(EMPTY_SELECTION), []);

  return { selectedIds, count: selectedIds.size, isSelected, toggle, clear };
}
