import type { LibraryShelf } from '../../platform/ipc';
import type { SystemLabel } from '../../hooks/useSystemCatalog';

/**
 * Orders shelves the way the sidebar orders systems.
 *
 * The system catalog is the single authority on system presentation order, and the frontend already
 * receives it as an ordered list. Re-deriving one here — a second hard-coded table of SNES, NES,
 * N64, … — would be a copy that silently disagrees with the sidebar the first time the catalog
 * changes, so the catalog list is used directly.
 *
 * A shelf whose system the catalog does not contain is **appended, never dropped**. That happens for
 * a real reason: a newer backend can return a system this build's catalog has not heard of, and the
 * catalog query can itself fail, leaving the list empty. Losing those games silently would be worse
 * than showing them last. Unknown shelves keep the backend's own deterministic order relative to one
 * another, so the view does not reshuffle between renders.
 */
export function orderShelvesByCatalog(
  shelves: readonly LibraryShelf[],
  systems: readonly SystemLabel[],
): LibraryShelf[] {
  const catalogRank = new Map(systems.map((system, index) => [system.id as string, index]));
  const known: LibraryShelf[] = [];
  const unknown: LibraryShelf[] = [];

  for (const shelf of shelves) {
    if (catalogRank.has(shelf.systemId)) known.push(shelf);
    else unknown.push(shelf);
  }

  known.sort((left, right) => catalogRank.get(left.systemId)! - catalogRank.get(right.systemId)!);

  return [...known, ...unknown];
}
