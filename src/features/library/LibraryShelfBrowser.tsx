import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import type { LibraryShelvesModel } from '../../hooks/useLibraryShelves';
import type { LibrarySelectionModel } from '../../hooks/useLibrarySelection';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { LibraryShelf } from './LibraryShelf';
import { systemName } from './libraryLabels';
import { orderShelvesByCatalog } from './shelfOrder';

interface LibraryShelfBrowserProps {
  model: LibraryShelvesModel;
  systems: SystemLabel[];
  selection: LibrarySelectionModel;
  /** The committed search this view is showing, for honest no-match copy. */
  committedSearch: string;
  hasActiveFilters: boolean;
  onOpenGame: (gameId: number, systemId: string) => void;
  onViewAll: (systemId: string) => void;
  onResetQuery: () => void;
}

function InitialShelfLoading() {
  return (
    <section aria-live="polite" className="library-initial-loading" role="status">
      <span className="library-loading-copy">READING LOCAL LIBRARY…</span>
      <div aria-hidden="true" className="library-shelf-skeleton">
        {Array.from({ length: 2 }, (_, shelfIndex) => (
          <div className="library-shelf-skeleton-row" key={shelfIndex}>
            <span className="library-shelf-skeleton-heading" />
            <div className="library-shelf-skeleton-track">
              {Array.from({ length: 4 }, (_, cardIndex) => (
                <span className="game-card-skeleton" key={cardIndex}>
                  <span className="game-card-skeleton-media" />
                  <span className="game-card-skeleton-copy" />
                </span>
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

/**
 * The All Systems browse view: one bounded shelf per system that has a match.
 *
 * This is discovery, not traversal. Each shelf shows a short preview of its system and ends with
 * View All, which switches the Library's own system filter and hands the user the ordinary
 * paginated grid. There is deliberately no pagination here: a bounded preview has no pages, and
 * offering navigation for a view that cannot be navigated would be a lie.
 */
export function LibraryShelfBrowser({
  model,
  systems,
  selection,
  committedSearch,
  hasActiveFilters,
  onOpenGame,
  onViewAll,
  onResetQuery,
}: LibraryShelfBrowserProps) {
  const shelves = model.shelves;
  const ordered = shelves === null ? [] : orderShelvesByCatalog(shelves.shelves, systems);
  const canEchoSearch = committedSearch !== '';

  return (
    <>
      {model.error ? (
        <InlineError
          title={shelves ? 'LIBRARY REFRESH FAILED' : 'LIBRARY QUERY UNAVAILABLE'}
          message={
            shelves
              ? 'The visible shelves may be out of date. Your local library remains available; retry the shelves.'
              : 'RetroFrontier could not read the bounded local library query. The shell and local scan controls remain available.'
          }
          actionLabel="RETRY LIBRARY"
          onAction={() => void model.retry()}
        />
      ) : null}

      {model.initialLoading && shelves === null ? <InitialShelfLoading /> : null}

      {ordered.length > 0 ? (
        <div className="library-shelf-browser">
          {ordered.map((shelf) => (
            <LibraryShelf
              key={shelf.systemId}
              onOpenGame={onOpenGame}
              onViewAll={onViewAll}
              selection={selection}
              shelf={shelf}
              systemName={systemName(shelf.systemId, systems)}
            />
          ))}
        </div>
      ) : null}

      {shelves !== null && ordered.length === 0 && !model.initialLoading && !model.error ? (
        <section aria-labelledby="no-results-heading" className="library-no-results">
          <h2 id="no-results-heading">
            {canEchoSearch ? `NO MATCH FOR “${committedSearch}”` : 'NO GAMES MATCH FILTERS'}
          </h2>
          <p>
            Your local library is not empty. Check the spelling or clear the active filters;
            scanning is not required.
          </p>
          {hasActiveFilters ? (
            <PixelButton onClick={onResetQuery} type="button" variant="secondary">
              CLEAR SEARCH &amp; FILTERS
            </PixelButton>
          ) : null}
        </section>
      ) : null}
    </>
  );
}
