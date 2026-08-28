import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import { PixelArrow } from '../../components/ui/PixelIcon';
import type { LibraryQueryModel } from '../../hooks/useLibraryQuery';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { GameCard } from './GameCard';
import { systemAccent } from './systemAccents';

interface LibraryBrowserProps {
  library: LibraryQueryModel;
  systems: SystemLabel[];
  onOpenGame: (gameId: number) => void;
}

function systemName(systemId: string, systems: SystemLabel[]) {
  return systems.find((system) => system.id === systemId)?.displayName ?? systemId;
}

function InitialLibraryLoading() {
  return (
    <section className="library-initial-loading" aria-live="polite" role="status">
      <span className="library-loading-copy">READING LOCAL LIBRARY…</span>
      <div aria-hidden="true" className="library-grid library-grid--skeleton">
        {Array.from({ length: 6 }, (_, index) => (
          <span className="game-card-skeleton" key={index}>
            <span className="game-card-skeleton-media" />
            <span className="game-card-skeleton-copy" />
          </span>
        ))}
      </div>
    </section>
  );
}

export function LibraryBrowser({ library, systems, onOpenGame }: LibraryBrowserProps) {
  const page = library.page;
  const hasActiveQuery =
    library.searchInput !== '' || library.systemId !== null || library.favoritesOnly;
  const firstVisible = page && page.total > 0 ? page.offset + 1 : 0;
  const lastVisible = page ? Math.min(page.total, page.offset + page.items.length) : 0;
  const resultRange = page
    ? page.total > 0
      ? `${firstVisible}–${lastVisible} OF ${page.total}`
      : '0 GAMES'
    : 'READING…';
  const committedSearch = library.debouncedSearch;
  const canEchoSearch = committedSearch !== '' && library.searchInput === committedSearch;

  return (
    <section aria-labelledby="library-browse-heading" className="library-browser">
      <div className="section-heading library-browser-heading">
        <h2 id="library-browse-heading">
          <PixelArrow className="heading-arrow" />
          BROWSE LIBRARY
        </h2>
        <span aria-hidden="true" />
        <span aria-live="polite" className="section-meta library-browser-count">
          {resultRange}
        </span>
        <span className="section-meta library-browser-system">
          {library.systemId
            ? systemName(library.systemId, systems).toLocaleUpperCase()
            : 'ALL SYSTEMS'}
        </span>
      </div>
      <div className="library-filter-bar" aria-label="Library filters">
        <span className="library-filter-label">// FILTER</span>
        <button
          aria-pressed={library.favoritesOnly}
          className={`library-filter${library.favoritesOnly ? ' library-filter--active' : ''}`}
          onClick={() => library.setFavoritesOnly(!library.favoritesOnly)}
          type="button"
        >
          FAVORITES ONLY
        </button>
        {hasActiveQuery && page?.items.length !== 0 ? (
          <button className="library-filter-reset" onClick={library.resetQuery} type="button">
            CLEAR SEARCH &amp; FILTERS
          </button>
        ) : !hasActiveQuery ? (
          <span className="library-filter-note">TITLE ORDER · LOCAL DATA</span>
        ) : null}
        {library.refreshing ? (
          <span aria-live="polite" className="library-refreshing" role="status">
            UPDATING…
          </span>
        ) : null}
      </div>

      {library.favoriteError ? (
        <InlineError
          title="FAVORITE NOT UPDATED"
          message="RetroFrontier could not save that favorite. The card still shows the last confirmed local state; try again."
        />
      ) : null}

      {library.error ? (
        <InlineError
          title={page ? 'LIBRARY REFRESH FAILED' : 'LIBRARY QUERY UNAVAILABLE'}
          message={
            page
              ? 'The visible bounded page may be out of date. Your local library remains available; retry this page.'
              : 'RetroFrontier could not read the bounded local library query. The shell and local scan controls remain available.'
          }
          actionLabel="RETRY LIBRARY"
          onAction={() => void library.retry()}
        />
      ) : null}

      {library.initialLoading && !page ? <InitialLibraryLoading /> : null}

      {page && page.items.length > 0 ? (
        <>
          <div className="library-grid">
            {page.items.map((item) => (
              <GameCard
                accent={systemAccent(item.systemId)}
                favoritePending={library.favoritePendingIds.has(item.gameId)}
                item={item}
                key={item.gameId}
                onOpenGame={onOpenGame}
                onToggleFavorite={(game) => void library.toggleFavorite(game)}
                systemName={systemName(item.systemId, systems)}
              />
            ))}
          </div>
          <nav aria-label="Library pages" className="library-pagination">
            <PixelButton
              disabled={page.offset === 0 || library.pageLoading}
              onClick={library.previousPage}
              type="button"
              variant="secondary"
            >
              PREVIOUS PAGE
            </PixelButton>
            <span aria-live="polite" className="library-page-status">
              {library.pageLoading
                ? 'LOADING PAGE…'
                : `PAGE ${Math.floor(page.offset / page.limit) + 1} OF ${Math.max(1, Math.ceil(page.total / page.limit))}`}
            </span>
            <PixelButton
              disabled={page.offset + page.items.length >= page.total || library.pageLoading}
              onClick={library.nextPage}
              type="button"
              variant="secondary"
            >
              NEXT PAGE
            </PixelButton>
          </nav>
        </>
      ) : null}

      {page && page.items.length === 0 && !library.initialLoading && !library.error ? (
        <section aria-labelledby="no-results-heading" className="library-no-results">
          <h2 id="no-results-heading">
            {canEchoSearch ? `NO MATCH FOR “${committedSearch}”` : 'NO GAMES MATCH FILTERS'}
          </h2>
          <p>
            Your local library is not empty. Check the spelling or clear the active filters;
            scanning is not required.
          </p>
          <PixelButton onClick={library.resetQuery} type="button" variant="secondary">
            CLEAR SEARCH &amp; FILTERS
          </PixelButton>
        </section>
      ) : null}
    </section>
  );
}
