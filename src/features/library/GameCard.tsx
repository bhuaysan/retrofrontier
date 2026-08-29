import type { CSSProperties, MouseEvent } from 'react';

import type { LibraryListItem, LibraryMetadataMatchState } from '../../platform/ipc';
import { gameRoute, routePath } from '../../app/routes';
import { PixelCheck } from '../../components/ui/PixelIcon';
import { GameCover } from './GameCover';
import { systemAccentKey } from './systemAccents';
import { systemShortLabel } from './systemLabels';

interface GameCardProps {
  item: LibraryListItem;
  systemName: string;
  accent: string;
  selected: boolean;
  onOpenGame: (gameId: number) => void;
  onToggleSelected: (gameId: number) => void;
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

export function GameCard({
  item,
  systemName,
  accent,
  selected,
  onOpenGame,
  onToggleSelected,
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
      className={`game-card${unavailable ? ' game-card--unavailable' : ''}${
        selected ? ' game-card--selected' : ''
      }`}
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
        {/* B1 selection control. It is a sibling of the stretched detail anchor, never a child of
            it, and it is layered above that anchor's hit area, so pointer and keyboard activation
            reach the selection toggle instead of the detail route. */}
        <button
          aria-label={selected ? `Deselect ${title}` : `Select ${title}`}
          aria-pressed={selected}
          className="game-card-select"
          onClick={() => onToggleSelected(item.gameId)}
          type="button"
        >
          {selected ? <PixelCheck /> : null}
        </button>
      </div>

      <div className="game-card-copy">
        <h3 aria-label={title} className="game-card-title" id={headingId} title={title}>
          <a
            aria-label={`Open ${title} details`}
            className="game-card-title-link game-card-detail-target"
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
