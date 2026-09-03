import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { focusNodes } from '../../focus/focusNodes';
import type { LibraryListItem, LibraryShelf as LibraryShelfData } from '../../platform/ipc';
import { LibraryShelf } from './LibraryShelf';

function gameCubeItem(gameId: number, displayTitle: string): LibraryListItem {
  return {
    gameId,
    systemId: 'nintendo_gamecube',
    localTitle: displayTitle,
    metadataTitle: displayTitle,
    displayTitle,
    sortTitle: displayTitle.toLowerCase(),
    availability: 'available',
    favorite: false,
    metadataMatchState: 'matched',
    releaseDate: '2006-12-02',
    genre: 'Action',
    region: 'US',
    coverRef: null,
  };
}

const shelf: LibraryShelfData = {
  systemId: 'nintendo_gamecube',
  total: 4,
  items: [gameCubeItem(1, 'The Legend of Zelda: Twilight Princess'), gameCubeItem(2, 'Pikmin')],
};

const selection = {
  isSelected: () => false,
  toggle: vi.fn(),
} as unknown as Parameters<typeof LibraryShelf>[0]['selection'];

function renderShelf(overrides: Partial<LibraryShelfData> = {}) {
  const onViewAll = vi.fn();
  const onOpenGame = vi.fn();
  render(
    <LibraryShelf
      onOpenGame={onOpenGame}
      onViewAll={onViewAll}
      selection={selection}
      shelf={{ ...shelf, ...overrides }}
      systemName="Nintendo GameCube"
    />,
  );
  return { onViewAll, onOpenGame };
}

function viewAll() {
  return screen.getByRole('button', { name: 'View all 4 Nintendo GameCube games' });
}

function track() {
  const element = document.querySelector<HTMLDivElement>('.library-shelf-track');
  if (element === null) throw new Error('the shelf rendered no track');
  return element;
}

/** jsdom performs no layout, so the track's scroll geometry is supplied directly. */
function setTrackGeometry({
  scrollLeft,
  scrollWidth,
  clientWidth,
}: {
  scrollLeft: number;
  scrollWidth: number;
  clientWidth: number;
}) {
  const element = track();
  for (const [name, value] of Object.entries({ scrollLeft, scrollWidth, clientWidth })) {
    Object.defineProperty(element, name, { configurable: true, value });
  }
  return element;
}

describe('LibraryShelf View All', () => {
  it('keeps its semantic focus identity and system-named accessible label', () => {
    renderShelf();

    expect(focusNodes.libraryShelfViewAll('nintendo_gamecube')).toBe(
      'library:shelf:view-all:nintendo_gamecube',
    );
    expect(viewAll()).toHaveAccessibleName('View all 4 Nintendo GameCube games');
  });

  it('reads as navigation: a VIEW ALL label, a direction arrow, and a counted noun', () => {
    renderShelf();
    const control = viewAll();

    expect(control.querySelector('.library-shelf-view-all-label')).toHaveTextContent('VIEW ALL');
    // The count is copy, not a bare numeral that would read as a badge on a game card.
    expect(control.querySelector('.library-shelf-view-all-count')).toHaveTextContent('4 GAMES');
    expect(
      control.querySelector('.library-shelf-view-all-arrow'),
      'the direction arrow carries the navigational meaning',
    ).not.toBeNull();
  });

  it('says GAME when the system holds exactly one', () => {
    renderShelf({ total: 1 });

    expect(
      screen
        .getByRole('button', { name: 'View all 1 Nintendo GameCube games' })
        .querySelector('.library-shelf-view-all-count'),
    ).toHaveTextContent('1 GAME');
  });

  it('still invokes the system-filter action and stays outside card selection', () => {
    const { onViewAll, onOpenGame } = renderShelf();

    fireEvent.click(viewAll());

    expect(onViewAll).toHaveBeenCalledWith('nintendo_gamecube');
    expect(onOpenGame).not.toHaveBeenCalled();
    expect(viewAll()).not.toHaveAttribute('aria-pressed');
    expect(viewAll().className).not.toContain('game-card');
  });
});

describe('LibraryShelf overflow affordance', () => {
  const observers: { callback: () => void; disconnected: boolean }[] = [];

  afterEach(() => {
    observers.length = 0;
    vi.unstubAllGlobals();
  });

  function stubResizeObserver() {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        private readonly entry: { callback: () => void; disconnected: boolean };
        constructor(callback: () => void) {
          this.entry = { callback, disconnected: false };
          observers.push(this.entry);
        }
        observe() {}
        disconnect() {
          this.entry.disconnected = true;
        }
      },
    );
  }

  it('claims no hidden content at the scroll origin of a shelf that fits', () => {
    renderShelf();

    expect(track()).toHaveAttribute('data-overflow-left', 'false');
    expect(track()).toHaveAttribute('data-overflow-right', 'false');
  });

  it('reports only the right edge while the shelf sits at its scroll origin', () => {
    renderShelf();
    setTrackGeometry({ scrollLeft: 0, scrollWidth: 900, clientWidth: 600 });
    fireEvent.scroll(track());

    // The whole point of the correction: at the origin nothing is hidden to the left, so the first
    // card keeps a hard edge under the system heading.
    expect(track()).toHaveAttribute('data-overflow-left', 'false');
    expect(track()).toHaveAttribute('data-overflow-right', 'true');
  });

  it('reports both edges once the shelf has actually been scrolled away from the origin', () => {
    renderShelf();
    setTrackGeometry({ scrollLeft: 120, scrollWidth: 900, clientWidth: 600 });
    fireEvent.scroll(track());

    expect(track()).toHaveAttribute('data-overflow-left', 'true');
    expect(track()).toHaveAttribute('data-overflow-right', 'true');
  });

  it('drops the right edge at the rightmost position and leaves no stale state on return', () => {
    renderShelf();
    setTrackGeometry({ scrollLeft: 300, scrollWidth: 900, clientWidth: 600 });
    fireEvent.scroll(track());

    expect(track()).toHaveAttribute('data-overflow-left', 'true');
    expect(track()).toHaveAttribute('data-overflow-right', 'false');

    setTrackGeometry({ scrollLeft: 0, scrollWidth: 900, clientWidth: 600 });
    fireEvent.scroll(track());

    expect(track()).toHaveAttribute('data-overflow-left', 'false');
    expect(track()).toHaveAttribute('data-overflow-right', 'true');
  });

  it('re-measures when the track is resized rather than only when it is scrolled', () => {
    stubResizeObserver();
    renderShelf();
    setTrackGeometry({ scrollLeft: 0, scrollWidth: 900, clientWidth: 600 });

    expect(track(), 'no scroll happened, so only the observer can notice').toHaveAttribute(
      'data-overflow-right',
      'false',
    );
    expect(observers).toHaveLength(1);
    act(() => observers[0].callback());

    expect(track()).toHaveAttribute('data-overflow-right', 'true');
  });
});
