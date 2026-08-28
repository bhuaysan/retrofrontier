import { useState, type CSSProperties } from 'react';

import type { LibraryListItem, LibraryMetadataMatchState } from '../../platform/ipc';

interface GameCardProps {
  item: LibraryListItem;
  systemName: string;
  accent: string;
  favoritePending: boolean;
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

function PixelStar({ filled }: { filled: boolean }) {
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
  onToggleFavorite,
}: GameCardProps) {
  const [failedCover, setFailedCover] = useState<{
    item: LibraryListItem;
    coverRef: string;
  } | null>(null);
  const title = item.displayTitle || item.localTitle;
  const headingId = `game-card-title-${item.gameId}`;
  const metadataLabel = METADATA_LABELS[item.metadataMatchState];
  const releaseYear = item.releaseDate?.match(/^\d{4}/)?.[0] ?? null;
  const coverFailed =
    item.coverRef !== null && failedCover?.item === item && failedCover.coverRef === item.coverRef;

  return (
    <article
      aria-labelledby={headingId}
      className={`game-card${item.availability === 'unavailable' ? ' game-card--unavailable' : ''}`}
      style={{ '--system-accent': accent } as CSSProperties}
    >
      <div className="game-card-media">
        {item.coverRef && !coverFailed ? (
          <img
            alt={`Cover art for ${title}`}
            className="game-card-cover"
            loading="lazy"
            onError={() => {
              if (item.coverRef) setFailedCover({ item, coverRef: item.coverRef });
            }}
            src={item.coverRef}
          />
        ) : (
          <div
            aria-label={`No cover available for ${title}`}
            className="game-card-placeholder"
            role="img"
            style={{ '--system-accent': accent } as CSSProperties}
          >
            <span>{title}</span>
          </div>
        )}
        <button
          aria-label={
            favoritePending
              ? `Updating favorite for ${title}`
              : item.favorite
                ? `Remove ${title} from favorites`
                : `Add ${title} to favorites`
          }
          aria-pressed={item.favorite}
          className="game-card-favorite"
          disabled={favoritePending}
          onClick={() => onToggleFavorite(item)}
          type="button"
        >
          <PixelStar filled={item.favorite} />
        </button>
      </div>

      <div className="game-card-copy">
        <h2 id={headingId} title={title}>
          {title}
        </h2>
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
