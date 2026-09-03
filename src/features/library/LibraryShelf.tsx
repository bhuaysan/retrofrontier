import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react';

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

function games(total: number) {
  return `${count(total)} ${total === 1 ? 'GAME' : 'GAMES'}`;
}

/**
 * The View All control.
 *
 * A real focus target with the system in its accessible name, not decorative text: it is how a
 * controller leaves the preview for the system's complete library, and it must be findable by name
 * among a dozen other "View all" controls on the same screen.
 *
 * It is deliberately card-sized so shelf focus geometry stays predictable, but it must not *read*
 * as a card. Beside a missing-cover placeholder a bare outlined box says "this game failed to
 * load"; the arrow, the solid structural border, and a counted noun say "the rest of this system
 * is through here" instead.
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
        <PixelArrow className="library-shelf-view-all-arrow" />
      </span>
      {/* The count is copy, not a bare numeral: "4" beside a game card reads as a badge on the
          card, "4 GAMES" reads as the size of the library this control opens. */}
      <span aria-hidden="true" className="library-shelf-view-all-count">
        {games(total)}
      </span>
    </button>
  );
}

/**
 * Truthful horizontal overflow state for one shelf track.
 *
 * The edge affordance describes *actual* hidden content rather than the mere fact that the
 * container can scroll. At the scroll origin nothing is hidden to the left, so nothing is drawn
 * there and the first card keeps the hard edge that lines it up under the system heading.
 */
function useShelfOverflow(itemCount: number) {
  const trackRef = useRef<HTMLDivElement | null>(null);
  const [overflow, setOverflow] = useState({ left: false, right: false });

  useLayoutEffect(() => {
    const track = trackRef.current;
    if (track === null) {
      return;
    }

    const measure = () => {
      // Sub-pixel track widths make an exact comparison claim a phantom edge, so a whole pixel of
      // hidden content is the threshold for saying any content is hidden at all.
      const hiddenRight = track.scrollWidth - track.clientWidth - track.scrollLeft;
      const next = { left: track.scrollLeft > 1, right: hiddenRight > 1 };
      setOverflow((current) =>
        current.left === next.left && current.right === next.right ? current : next,
      );
    };

    measure();
    track.addEventListener('scroll', measure, { passive: true });
    // A width change hides or reveals content without any scrolling: a resized window, or the
    // shell giving the content column a different width. Absent in jsdom, which has no layout to
    // observe in the first place — there the scroll listener alone carries the contract.
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
    observer?.observe(track);

    return () => {
      track.removeEventListener('scroll', measure);
      observer?.disconnect();
    };
    // A shelf that gains or loses preview items changes what is hidden without resizing its track.
  }, [itemCount]);

  return { trackRef, overflow };
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
  const { trackRef, overflow } = useShelfOverflow(shelf.items.length);

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
        <span className="library-shelf-meta">{games(shelf.total)}</span>
      </div>

      <div
        className="library-shelf-track"
        data-overflow-left={String(overflow.left)}
        data-overflow-right={String(overflow.right)}
        ref={trackRef}
      >
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
