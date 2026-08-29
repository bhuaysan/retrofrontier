import type { LibraryQueryModel } from '../../hooks/useLibraryQuery';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { systemName } from './libraryLabels';

interface LibraryFilterBarProps {
  library: LibraryQueryModel;
  systems: SystemLabel[];
}

export function LibraryFilterBar({ library, systems }: LibraryFilterBarProps) {
  const hasActiveQuery =
    library.searchInput !== '' || library.systemId !== null || library.favoritesOnly;
  const page = library.page;
  const firstVisible = page && page.total > 0 ? page.offset + 1 : 0;
  const lastVisible = page ? Math.min(page.total, page.offset + page.items.length) : 0;
  const resultRange = page
    ? page.total > 0
      ? `${firstVisible}–${lastVisible} OF ${page.total}`
      : '0 GAMES'
    : 'READING…';

  return (
    <div aria-label="Library filters" className="library-filter-bar" role="group">
      <span className="library-filter-label">// FILTER</span>
      <button
        aria-pressed={library.favoritesOnly}
        className={`library-filter${library.favoritesOnly ? ' library-filter--active' : ''}`}
        onClick={() => library.setFavoritesOnly(!library.favoritesOnly)}
        type="button"
      >
        <span aria-hidden="true">★</span> FAVORITES ONLY
      </button>
      {hasActiveQuery && page?.items.length !== 0 ? (
        <button className="library-filter-reset" onClick={library.resetQuery} type="button">
          CLEAR SEARCH &amp; FILTERS
        </button>
      ) : null}
      <span aria-hidden="true" className="library-filter-spacer" />
      {library.refreshing ? (
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
    </div>
  );
}
