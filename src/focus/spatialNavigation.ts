import type { DirectionalAction } from '../input/actions';

export interface NavigationRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface NavigationCandidate {
  id: string;
  rect: NavigationRect;
}

/** Guards against sub-pixel layout noise deciding whether a candidate lies "ahead". */
const EPSILON = 0.5;

/**
 * How much a candidate is penalised for being off the current row or column. Movement prefers the
 * aligned neighbour, and only leaves the row/column when nothing aligned lies ahead.
 */
const CROSS_AXIS_WEIGHT = 3;

function centerX(rect: NavigationRect) {
  return (rect.left + rect.right) / 2;
}

function centerY(rect: NavigationRect) {
  return (rect.top + rect.bottom) / 2;
}

function overlaps(aStart: number, aEnd: number, bStart: number, bEnd: number) {
  return Math.min(aEnd, bEnd) - Math.max(aStart, bStart) > EPSILON;
}

function hasArea(rect: NavigationRect) {
  return rect.right - rect.left > 0 && rect.bottom - rect.top > 0;
}

/**
 * Resolves the next focus target from the rendered geometry of the currently navigable nodes.
 *
 * The algorithm is layout-derived on purpose: the Library grid is responsive, so no fixed column
 * count exists. Left/right prefer a candidate that shares the current visual row, up/down prefer one
 * that shares the current column, and both fall back to the nearest candidate in that direction with
 * a cross-axis penalty. There is no wrapping: an edge is a stop, which keeps repeated movement
 * predictable. Ties resolve by candidate order, which callers supply in document order.
 */
export function findNextNode(
  currentId: string | null,
  candidates: readonly NavigationCandidate[],
  direction: DirectionalAction,
): string | null {
  const navigable = candidates.filter((candidate) => hasArea(candidate.rect));
  if (navigable.length === 0) return null;

  const current = navigable.find((candidate) => candidate.id === currentId);
  if (current === undefined) return navigable[0].id;

  const horizontal = direction === 'moveLeft' || direction === 'moveRight';
  const forward = direction === 'moveRight' || direction === 'moveDown';
  const originMain = horizontal ? centerX(current.rect) : centerY(current.rect);
  const originCross = horizontal ? centerY(current.rect) : centerX(current.rect);

  let best: { id: string; score: number; aligned: boolean; cross: number } | null = null;

  for (const candidate of navigable) {
    if (candidate.id === current.id) continue;

    const main = horizontal ? centerX(candidate.rect) : centerY(candidate.rect);
    const delta = forward ? main - originMain : originMain - main;
    if (delta <= EPSILON) continue;

    const cross = Math.abs((horizontal ? centerY : centerX)(candidate.rect) - originCross);
    const aligned = horizontal
      ? overlaps(current.rect.top, current.rect.bottom, candidate.rect.top, candidate.rect.bottom)
      : overlaps(current.rect.left, current.rect.right, candidate.rect.left, candidate.rect.right);
    const score = delta + CROSS_AXIS_WEIGHT * cross;

    if (best === null) {
      best = { id: candidate.id, score, aligned, cross };
      continue;
    }
    if (aligned !== best.aligned) {
      if (aligned) best = { id: candidate.id, score, aligned, cross };
      continue;
    }
    if (score < best.score || (score === best.score && cross < best.cross)) {
      best = { id: candidate.id, score, aligned, cross };
    }
  }

  return best?.id ?? null;
}
