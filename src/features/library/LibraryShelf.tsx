import type { CSSProperties } from 'react';

import { useFocusNode } from '../../focus/focusContext';
import { focusNodes } from '../../focus/focusNodes';
import { PixelArrow } from '../../components/ui/PixelIcon';
import type { LibraryShelf as LibraryShelfData } from '../../platform/ipc';
import type { LibrarySelectionModel } from '../../hooks/useLibrarySelection';
import { GameCard } from './GameCard';
import { systemAccent, systemAccentKey } from './systemAccents';

interface LibraryShelfProps {
  shelf: LibraryShelfData;
  systemName: string;
  onOpenGame: (gameId: number, systemId: string) => void;
  onViewAll: (systemId: string) => void;
  selection: LibrarySelectionModel;
}

function count(value: number) {
  return value.toLocaleString('en-US');
}

/**
 * The View All control.
 *
 * A real focus target with the system in its accessible name, not decorative text: it is how a
 * controller leaves the preview for the system's complete library, and it must be findable by name
 * among a dozen other "View all" controls on the same screen.
 */
function ViewAllShelf({
  systemId,
  systemName,
  total,
  onViewAll,
}: {
  systemId: string;
  systemName: string;
  total: number;
  onViewAll: (systemId: string) => void;
}) {
  const focusRef = useFocusNode({
    id: focusNodes.libraryShelfViewAll(systemId),
    confirm: { label: 'VIEW ALL' },
  });

  return (
    <button
      aria-label={`View all ${count(total)} ${systemName} games`}
      className="library-shelf-view-all"
      onClick={() => onViewAll(systemId)}
      ref={focusRef}
      type="button"
    >
      <span aria-hidden="true" className="library-shelf-view-all-label">
        VIEW ALL
      </span>
      <span aria-hidden="true" className="library-shelf-view-all-count">
        {count(total)}
      </span>
    </button>
  );
}

/**
 * One system's shelf: a heading, a bounded preview, and View All.
 *
 * The track never wraps. On a narrow window it scrolls horizontally instead, which keeps every
 * bounded preview item and View All reachable by pointer and by controller — the browser scrolls a
 * focused card into view on its own. It stays a *bounded* preview either way: the system's whole
 * collection is behind View All, never behind a long horizontal scroll.
 */
export function LibraryShelf({
  shelf,
  systemName,
  onOpenGame,
  onViewAll,
  selection,
}: LibraryShelfProps) {
  const headingId = `library-shelf-heading-${shelf.systemId}`;
  const accent = systemAccent(shelf.systemId);

  return (
    <section
      aria-labelledby={headingId}
      className="library-shelf"
      data-system-accent={systemAccentKey(shelf.systemId)}
      style={{ '--system-accent': accent } as CSSProperties}
    >
      <div className="library-shelf-heading">
        {/* One level under the LIBRARY heading and one above a card title, so the browse view reads
            as LIBRARY → system → game rather than putting a shelf beside the games it introduces. */}
        <h2 id={headingId}>
          <PixelArrow className="heading-arrow" />
          {systemName.toLocaleUpperCase()}
        </h2>
        <span aria-hidden="true" />
        <span className="library-shelf-meta">
          {count(shelf.total)} {shelf.total === 1 ? 'GAME' : 'GAMES'}
        </span>
      </div>

      <div className="library-shelf-track">
        {shelf.items.map((item) => (
          <GameCard
            accent={systemAccent(item.systemId)}
            item={item}
            key={item.gameId}
            onOpenGame={onOpenGame}
            onToggleSelected={selection.toggle}
            selected={selection.isSelected(item.gameId)}
            systemName={systemName}
          />
        ))}
        <ViewAllShelf
          onViewAll={onViewAll}
          systemId={shelf.systemId}
          systemName={systemName}
          total={shelf.total}
        />
      </div>
    </section>
  );
}
