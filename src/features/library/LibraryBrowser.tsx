import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import type { LibraryQueryModel } from '../../hooks/useLibraryQuery';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { GameCard } from './GameCard';
import { systemName } from './libraryLabels';
import { systemAccent } from './systemAccents';

interface LibraryBrowserProps {
  library: LibraryQueryModel;
  systems: SystemLabel[];
  onOpenGame: (gameId: number) => void;
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
  const committedSearch = library.debouncedSearch;
  const canEchoSearch = committedSearch !== '' && library.searchInput === committedSearch;
  // A single bounded page cannot be navigated, so the pagination row is pure vertical cost there.
  // `offset > 0` keeps the control present after a total shrink, until the clamped page commits.
  const canPaginate = page !== null && (page.total > page.limit || page.offset > 0);

  return (
    <section aria-label="Library results" className="library-browser">
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
          {canPaginate ? (
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
          ) : null}
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
