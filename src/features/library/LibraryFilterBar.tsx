import type { LibraryQueryModel } from '../../hooks/useLibraryQuery';
import type { LibraryShelvesModel } from '../../hooks/useLibraryShelves';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { PixelStar } from '../../components/ui/PixelIcon';
import { systemName } from './libraryLabels';

interface LibraryFilterBarProps {
  library: LibraryQueryModel;
  shelves: LibraryShelvesModel;
  systems: SystemLabel[];
}

export function LibraryFilterBar({ library, shelves, systems }: LibraryFilterBarProps) {
  const hasActiveQuery =
    library.searchInput !== '' ||
    library.systemId !== null ||
    library.favoritesOnly ||
    library.hideMissing ||
    library.needsMetadataReview;
  // All Systems browses shelves, which have no page and therefore no honest visible range: the
  // meaningful number there is how many games the active filters match across every system.
  const showsShelves = library.systemId === null;
  const page = library.page;
  const firstVisible = page && page.total > 0 ? page.offset + 1 : 0;
  const lastVisible = page ? Math.min(page.total, page.offset + page.items.length) : 0;
  const shelfTotal = shelves.shelves?.shelves.reduce((sum, shelf) => sum + shelf.total, 0) ?? null;
  const gridRange = page
    ? page.total > 0
      ? `${firstVisible}–${lastVisible} OF ${page.total}`
      : '0 GAMES'
    : 'READING…';
  const shelfRange =
    shelfTotal === null
      ? 'READING…'
      : shelfTotal === 1
        ? '1 GAME'
        : `${shelfTotal.toLocaleString('en-US')} GAMES`;
  const resultRange = showsShelves ? shelfRange : gridRange;
  const hasVisibleResults = showsShelves ? shelfTotal !== 0 : page?.items.length !== 0;

  return (
    <div aria-label="Library filters" className="library-filter-bar" role="group">
      <span className="library-filter-label">// FILTER</span>
      <button
        aria-pressed={library.favoritesOnly}
        className={`library-filter${library.favoritesOnly ? ' library-filter--active' : ''}`}
        onClick={() => library.setFavoritesOnly(!library.favoritesOnly)}
        type="button"
      >
        <PixelStar filled /> FAVORITES ONLY
      </button>
      <button
        aria-pressed={library.hideMissing}
        className={`library-filter${library.hideMissing ? ' library-filter--active' : ''}`}
        onClick={() => library.setHideMissing(!library.hideMissing)}
        title="Hides games whose local content the last scan found missing. Nothing is deleted."
        type="button"
      >
        HIDE MISSING
      </button>
      <button
        aria-pressed={library.needsMetadataReview}
        className={`library-filter${library.needsMetadataReview ? ' library-filter--active' : ''}`}
        onClick={() => library.setNeedsMetadataReview(!library.needsMetadataReview)}
        type="button"
      >
        NEEDS REVIEW
      </button>
      <span aria-hidden="true" className="library-filter-spacer" />
      {(showsShelves ? shelves.refreshing : library.refreshing) ? (
        <span aria-live="polite" className="library-refreshing" role="status">
          UPDATING…
        </span>
      ) : null}
      <p className="library-result-meta">
        <span aria-live="polite" className="library-result-range">
          {resultRange}
        </span>
        <span aria-hidden="true">·</span>
        <span className="library-result-system">
          {library.systemId
            ? systemName(library.systemId, systems).toLocaleUpperCase()
            : 'ALL SYSTEMS'}
        </span>
      </p>
      {hasActiveQuery && hasVisibleResults ? (
        <button className="library-filter-reset" onClick={library.resetQuery} type="button">
          CLEAR SEARCH &amp; FILTERS
        </button>
      ) : null}
    </div>
  );
}
