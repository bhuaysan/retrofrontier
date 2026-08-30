import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { PixelArrow, PixelStar } from './PixelIcon';
import { PixelRow } from './PixelRow';

function arrowPath(direction: 'left' | 'right') {
  const { container } = render(<PixelArrow direction={direction} />);
  const svg = container.querySelector('svg');
  const path = svg?.querySelector('path')?.getAttribute('d');
  return {
    height: svg?.getAttribute('height'),
    path: path ?? '',
    shapeRendering: svg?.getAttribute('shape-rendering'),
    viewBox: svg?.getAttribute('viewBox'),
    width: svg?.getAttribute('width'),
  };
}

describe('PixelArrow', () => {
  it.each(['right', 'left'] as const)(
    'uses smooth vector geometry for the %s arrow',
    (direction) => {
      const { path, shapeRendering, viewBox } = arrowPath(direction);

      expect(viewBox).toBe('0 0 10 14');
      expect(shapeRendering).toBeNull();
      expect(path).toBe(direction === 'right' ? 'M1.5 1L8.5 7L1.5 13Z' : 'M8.5 1L1.5 7L8.5 13Z');
    },
  );

  it('keeps the existing cursor dimensions while rendering without pixel snapping', () => {
    const { height, width, shapeRendering } = arrowPath('right');

    expect(width).toBe('10');
    expect(height).toBe('14');
    expect(shapeRendering).toBeNull();
  });
});

describe('PixelStar', () => {
  it.each([true, false])('uses smooth vector geometry when filled=%s', (filled) => {
    const { container } = render(<PixelStar filled={filled} />);
    const svg = container.querySelector('svg');
    const path = svg?.querySelector('path');

    expect(svg).toHaveAttribute('viewBox', '0 0 24 24');
    expect(svg).toHaveAttribute('width', '16');
    expect(svg).toHaveAttribute('height', '16');
    expect(svg).not.toHaveAttribute('shape-rendering');
    expect(path).toHaveAttribute('fill', filled ? 'currentColor' : 'none');
    expect(path).toHaveAttribute('stroke', 'currentColor');
    expect(path).toHaveAttribute('stroke-linejoin', 'round');
  });
});

describe('PixelRow focus cursor', () => {
  it('renders the smooth arrow cursor at the existing size', () => {
    const { container } = render(
      <ul>
        <PixelRow accent="var(--accent)" label="SNES" count={2} />
      </ul>,
    );

    const cursor = container.querySelector('.pixel-row-cursor');
    const svg = cursor?.querySelector('svg');
    expect(cursor).toHaveAttribute('aria-hidden', 'true');
    expect(svg).toHaveAttribute('width', '10');
    expect(svg).toHaveAttribute('height', '14');
    expect(svg).toHaveAttribute('viewBox', '0 0 10 14');
    expect(svg).not.toHaveAttribute('shape-rendering');
  });
});
