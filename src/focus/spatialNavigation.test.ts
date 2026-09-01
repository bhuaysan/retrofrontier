import { describe, expect, it } from 'vitest';

import { findNextNode, type NavigationCandidate } from './spatialNavigation';

/** A responsive card grid: `columns` per row, 160×220 cards on a 20px gutter. */
function grid(count: number, columns: number, originX = 300, originY = 200) {
  const cards: NavigationCandidate[] = [];
  for (let index = 0; index < count; index += 1) {
    const column = index % columns;
    const row = Math.floor(index / columns);
    const left = originX + column * 180;
    const top = originY + row * 240;
    cards.push({
      id: `card-${index}`,
      rect: { left, top, right: left + 160, bottom: top + 220 },
    });
  }
  return cards;
}

const sidebar: NavigationCandidate[] = [
  { id: 'sidebar-0', rect: { left: 20, top: 200, right: 260, bottom: 240 } },
  { id: 'sidebar-1', rect: { left: 20, top: 250, right: 260, bottom: 290 } },
  { id: 'sidebar-2', rect: { left: 20, top: 300, right: 260, bottom: 340 } },
];

describe('findNextNode', () => {
  it('enters the first candidate in document order when nothing is focused', () => {
    expect(findNextNode(null, grid(6, 3), 'moveDown')).toBe('card-0');
    expect(findNextNode('card-does-not-exist', grid(6, 3), 'moveRight')).toBe('card-0');
  });

  it('returns null when there is nothing to focus', () => {
    expect(findNextNode(null, [], 'moveDown')).toBeNull();
  });

  it('moves along the visual row for left and right', () => {
    const cards = grid(9, 3);
    expect(findNextNode('card-0', cards, 'moveRight')).toBe('card-1');
    expect(findNextNode('card-1', cards, 'moveRight')).toBe('card-2');
    expect(findNextNode('card-4', cards, 'moveLeft')).toBe('card-3');
  });

  it('moves within the same column for up and down', () => {
    const cards = grid(9, 3);
    expect(findNextNode('card-1', cards, 'moveDown')).toBe('card-4');
    expect(findNextNode('card-7', cards, 'moveUp')).toBe('card-4');
  });

  it('reflects the rendered layout rather than a fixed column count', () => {
    const wide = grid(10, 5);
    expect(findNextNode('card-0', wide, 'moveDown')).toBe('card-5');
    const narrow = grid(10, 2);
    expect(findNextNode('card-0', narrow, 'moveDown')).toBe('card-2');
  });

  it('keeps up and down deterministic across an irregular final row', () => {
    // Seven cards over three columns: the last row holds only card-6.
    const cards = grid(7, 3);
    expect(findNextNode('card-3', cards, 'moveDown')).toBe('card-6');
    // No card sits directly below card-4, so the nearest one in the final row is taken.
    expect(findNextNode('card-4', cards, 'moveDown')).toBe('card-6');
    expect(findNextNode('card-6', cards, 'moveUp')).toBe('card-3');
  });

  it('stops at an edge instead of wrapping', () => {
    const cards = grid(9, 3);
    expect(findNextNode('card-2', cards, 'moveRight')).toBeNull();
    expect(findNextNode('card-1', cards, 'moveUp')).toBeNull();
    expect(findNextNode('card-7', cards, 'moveDown')).toBeNull();
  });

  it('leaves the grid sideways only when the row itself has no further candidate', () => {
    const cards = [...sidebar, ...grid(9, 3)];
    expect(findNextNode('card-1', cards, 'moveLeft')).toBe('card-0');
    // The tall card spans three sidebar rows; the one nearest its vertical centre wins.
    expect(findNextNode('card-0', cards, 'moveLeft')).toBe('sidebar-2');
    expect(findNextNode('sidebar-0', cards, 'moveRight')).toBe('card-0');
  });

  it('never selects a candidate that was withheld', () => {
    const cards = grid(9, 3).filter((candidate) => candidate.id !== 'card-1');
    expect(findNextNode('card-0', cards, 'moveRight')).toBe('card-2');
    expect(findNextNode('card-1', cards, 'moveRight')).toBe('card-0');
  });

  it('breaks ties by candidate order so movement is reproducible', () => {
    const tied: NavigationCandidate[] = [
      { id: 'origin', rect: { left: 100, top: 0, right: 200, bottom: 40 } },
      { id: 'left-twin', rect: { left: 0, top: 100, right: 100, bottom: 140 } },
      { id: 'right-twin', rect: { left: 200, top: 100, right: 300, bottom: 140 } },
    ];
    expect(findNextNode('origin', tied, 'moveDown')).toBe('left-twin');
    expect(findNextNode('origin', [tied[0], tied[2], tied[1]], 'moveDown')).toBe('right-twin');
  });

  it('ignores zero-area candidates', () => {
    const cards = [
      ...grid(2, 2),
      { id: 'collapsed', rect: { left: 640, top: 200, right: 640, bottom: 200 } },
    ];
    expect(findNextNode('card-1', cards, 'moveRight')).toBeNull();
  });
});
