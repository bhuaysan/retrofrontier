import type { CSSProperties, MouseEvent } from 'react';

import type { LibraryListItem, LibraryMetadataMatchState } from '../../platform/ipc';
import { gameRoute, routePath } from '../../app/routes';
import { GameCover } from './GameCover';

interface GameCardProps {
  item: LibraryListItem;
  systemName: string;
  accent: string;
  favoritePending: boolean;
  onOpenGame: (gameId: number) => void;
  onToggleFavorite: (item: LibraryListItem) => void;
}

const METADATA_LABELS: Partial<Record<LibraryMetadataMatchState, string>> = {
  pending: 'METADATA PENDING',
  noMatch: 'NO METADATA MATCH',
  ambiguous: 'MATCH REVIEW NEEDED',
  deferred: 'METADATA DEFERRED',
  failed: 'METADATA UNAVAILABLE',
  stale: 'METADATA STALE',
};

export function PixelStar({ filled }: { filled: boolean }) {
  return (
    <svg aria-hidden="true" shapeRendering="crispEdges" viewBox="0 0 14 14">
      <path
        d="M6 0h2v3h3v2h3v2h-2v2h1v5H8v-2H6v2H1V9h1V7H0V5h3V3h3z"
        fill={filled ? 'currentColor' : 'none'}
        stroke="currentColor"
        strokeWidth="1.5"
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
      className={`game-card${item.availability === 'unavailable' ? ' game-card--unavailable' : ''}`}
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
        <h3 aria-label={title} id={headingId} title={title}>
          <a
            aria-label={`Open ${title} details`}
            data-game-detail-link={item.gameId}
            href={routePath(gameRoute(item.gameId))}
            onClick={handleDetailClick}
          >
            {title}
          </a>
        </h3>
        <div className="game-card-system-row">
          <span className="game-card-system" title={systemName}>
            {systemName}
          </span>
          {releaseYear ? <time dateTime={item.releaseDate ?? undefined}>{releaseYear}</time> : null}
        </div>
        <div className="game-card-state-row">
          <span
            className={`availability-badge availability-badge--${item.availability}`}
            title={
              item.availability === 'available'
                ? 'Local content is available'
                : 'Local content is unavailable or missing'
            }
          >
            {item.availability === 'available' ? 'LOCAL' : 'LOCAL FILE MISSING'}
          </span>
          {metadataLabel ? <span className="metadata-state">{metadataLabel}</span> : null}
        </div>
        {item.genre || item.region ? (
          <p className="game-card-meta">{[item.genre, item.region].filter(Boolean).join(' · ')}</p>
        ) : null}
      </div>
    </article>
  );
}
