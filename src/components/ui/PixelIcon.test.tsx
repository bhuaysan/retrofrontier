import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { PixelArrow } from './PixelIcon';
import { PixelRow } from './PixelRow';

/**
 * Each arrow row is drawn as one `Mx yh{width}v1H{x}z` segment, so the filled cells can be read
 * straight back out of the path and checked against the viewBox. The original glyph was a hollow
 * chevron outline whose left edge ran to x=-1, which the viewBox clipped into a lopsided blob.
 */
function readRows(path: string) {
  const segments = [...path.matchAll(/M(\d+) (\d+)h(\d+)v1H(\d+)z/g)];
  return segments.map(([, x, y, width, back]) => ({
    x: Number(x),
    y: Number(y),
    width: Number(width),
    back: Number(back),
  }));
}

function arrowPath(direction: 'left' | 'right') {
  const { container } = render(<PixelArrow direction={direction} />);
  const svg = container.querySelector('svg');
  const path = svg?.querySelector('path')?.getAttribute('d');
  return { viewBox: svg?.getAttribute('viewBox'), path: path ?? '' };
}

describe('PixelArrow', () => {
  it.each(['right', 'left'] as const)('draws every %s cell inside the viewBox', (direction) => {
    const { viewBox, path } = arrowPath(direction);
    expect(viewBox).toBe('0 0 5 7');
    const [, , boxWidth, boxHeight] = viewBox!.split(' ').map(Number);

    const rows = readRows(path);
    // Every row of the glyph is accounted for, so the regex did not silently skip a segment.
    expect(rows).toHaveLength(boxHeight);
    expect(path.replace(/M\d+ \d+h\d+v1H\d+z/g, '')).toBe('');

    for (const row of rows) {
      expect(row.x).toBeGreaterThanOrEqual(0);
      expect(row.width).toBeGreaterThan(0);
      expect(row.x + row.width).toBeLessThanOrEqual(boxWidth);
      expect(row.y).toBeLessThan(boxHeight);
      // The subpath must close back on its own left edge rather than running off-canvas.
      expect(row.back).toBe(row.x);
    }
  });

  it('is a solid triangle: widths rise to a single apex row and mirror back down', () => {
    const widths = readRows(arrowPath('right').path).map((row) => row.width);

    expect(widths).toEqual([...widths].reverse());
    expect(Math.max(...widths)).toBe(5);
    expect(widths.filter((width) => width === 5)).toHaveLength(1);

    const apex = widths.indexOf(5);
    for (let index = 1; index <= apex; index += 1) {
      expect(widths[index]).toBeGreaterThan(widths[index - 1]);
    }
  });

  it('mirrors the right arrow so the left variant points the other way', () => {
    const right = readRows(arrowPath('right').path);
    const left = readRows(arrowPath('left').path);

    // Right-pointing rows sit on the left edge; left-pointing rows end on the right edge.
    expect(right.every((row) => row.x === 0)).toBe(true);
    expect(left.every((row) => row.x + row.width === 5)).toBe(true);
    expect(left.map((row) => row.width)).toEqual(right.map((row) => row.width));
  });
});

describe('PixelRow focus cursor', () => {
  it('renders the A6 cursor arrow at a whole 2px per cell of the 5x7 glyph grid', () => {
    const { container } = render(
      <ul>
        <PixelRow accent="var(--accent)" label="SNES" count={2} />
      </ul>,
    );

    const cursor = container.querySelector('.pixel-row-cursor');
    const svg = cursor?.querySelector('svg');
    expect(cursor).toHaveAttribute('aria-hidden', 'true');
    // 10x14 is exactly 2px per cell, so `crispEdges` puts every step on a pixel boundary and no
    // step is wider than its neighbours.
    expect(svg).toHaveAttribute('width', '10');
    expect(svg).toHaveAttribute('height', '14');
    expect(svg).toHaveAttribute('viewBox', '0 0 5 7');
  });
});
