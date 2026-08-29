import type { CSSProperties, MouseEvent } from 'react';

import type { LibraryListItem, LibraryMetadataMatchState } from '../../platform/ipc';
import { gameRoute, routePath } from '../../app/routes';
import { GameCover } from './GameCover';
import { systemAccentKey } from './systemAccents';
import { systemShortLabel } from './systemLabels';

interface GameCardProps {
  item: LibraryListItem;
  systemName: string;
  accent: string;
  favoritePending: boolean;
  onOpenGame: (gameId: number) => void;
  onToggleFavorite: (item: LibraryListItem) => void;
}

/**
 * Coarse metadata lifecycle state. B1/C4 tiles are deliberately compact, so this is no longer a
 * visible card row; it stays in the card's accessible description, and Game Detail remains the
 * surface that renders the full metadata state.
 */
const METADATA_LABELS: Partial<Record<LibraryMetadataMatchState, string>> = {
  pending: 'METADATA PENDING',
  noMatch: 'NO METADATA MATCH',
  ambiguous: 'MATCH REVIEW NEEDED',
  deferred: 'METADATA DEFERRED',
  failed: 'METADATA UNAVAILABLE',
  stale: 'METADATA STALE',
};

export function PixelStar({ filled }: { filled: boolean }) {
  // Hard-edged 12×12 pixel star. The unfilled variant is the same silhouette with a one-pixel
  // interior cut out through `evenodd`, so both states stay crisp and unmistakably a star at the
  // small overlay size instead of closing into a blob the way a stroked outline did.
  const silhouette =
    'M5 0h2v2h-2zM4 2h4v2h-4zM0 4h12v1h-12zM1 5h10v1h-10zM2 6h8v1h-8z' +
    'M3 7h6v1h-6zM2 8h8v1h-8zM1 9h3v1h-3zM8 9h3v1h-3zM0 10h3v1h-3z' +
    'M9 10h3v1h-3zM0 11h2v1h-2zM10 11h2v1h-2z';
  const interior =
    'M5 2h2v2h-2zM4 4h4v1h-4zM2 5h8v1h-8zM3 6h6v1h-6zM4 7h4v1h-4z' +
    'M3 8h1v1h-1zM8 8h1v1h-1zM2 9h1v1h-1zM9 9h1v1h-1zM1 10h1v1h-1zM10 10h1v1h-1z';

  return (
    <svg aria-hidden="true" shapeRendering="crispEdges" viewBox="0 0 12 12">
      <path
        d={filled ? silhouette : `${silhouette}${interior}`}
        fill="currentColor"
        fillRule="evenodd"
      />
    </svg>
  );
}

export function GameCard({
  item,
  systemName,
  accent,
  favoritePending,
  onOpenGame,
  onToggleFavorite,
}: GameCardProps) {
  const title = item.displayTitle.trim() || item.localTitle.trim() || 'UNTITLED GAME';
  const headingId = `game-card-title-${item.gameId}`;
  const metadataLabel = METADATA_LABELS[item.metadataMatchState];
  const releaseYear = item.releaseDate?.match(/^\d{4}/)?.[0] ?? null;
  const unavailable = item.availability === 'unavailable';
  const shortSystem = systemShortLabel(item.systemId, systemName);

  const handleDetailClick = (event: MouseEvent<HTMLAnchorElement>) => {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    ) {
      return;
    }
    event.preventDefault();
    onOpenGame(item.gameId);
  };

  return (
    <article
      aria-labelledby={headingId}
      className={`game-card${unavailable ? ' game-card--unavailable' : ''}`}
      data-system-accent={systemAccentKey(item.systemId)}
      style={{ '--system-accent': accent } as CSSProperties}
    >
      <div className="game-card-media">
        <GameCover
          accent={accent}
          className={item.coverRef ? 'game-card-cover' : 'game-card-placeholder'}
          coverRef={item.coverRef}
          placeholderClassName="game-card-placeholder"
          resetKey={item}
          title={title}
        />
        {unavailable ? (
          <p className="game-card-flag" title="Local content is unavailable or missing">
            <span aria-hidden="true">!</span>
            <span>MISSING</span>
            <span className="visually-hidden">{' local content'}</span>
          </p>
        ) : null}
        <button
          aria-busy={favoritePending || undefined}
          aria-label={
            item.favorite ? `Remove ${title} from favorites` : `Add ${title} to favorites`
          }
          aria-pressed={item.favorite}
          className="game-card-favorite"
          onClick={() => onToggleFavorite(item)}
          type="button"
        >
          <PixelStar filled={item.favorite} />
        </button>
      </div>

      <div className="game-card-copy">
        <h3 aria-label={title} className="game-card-title" id={headingId} title={title}>
          <a
            aria-label={`Open ${title} details`}
            className="game-card-title-link"
            data-game-detail-link={item.gameId}
            href={routePath(gameRoute(item.gameId))}
            onClick={handleDetailClick}
          >
            {title}
          </a>
        </h3>
        <div className="game-card-system-row">
          <span className="game-card-system" title={systemName}>
            <span aria-hidden="true">{shortSystem}</span>
            <span className="visually-hidden">{systemName}</span>
          </span>
          {releaseYear ? (
            <time className="game-card-year" dateTime={item.releaseDate ?? undefined}>
              {releaseYear}
            </time>
          ) : null}
        </div>
      </div>

      {metadataLabel ? <p className="visually-hidden">{metadataLabel}</p> : null}
    </article>
  );
}
