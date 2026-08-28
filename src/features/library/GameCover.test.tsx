import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { GameCover } from './GameCover';

describe('GameCover', () => {
  it('renders an opaque cover reference and the C4 placeholder when absent', () => {
    const resetKey = {};
    const { rerender } = render(
      <GameCover
        accent="var(--accent)"
        className="test-cover"
        coverRef="rfmedia://localhost/cover/1"
        resetKey={resetKey}
        title="Kirby’s Adventure"
      />,
    );

    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveAttribute(
      'src',
      'rfmedia://localhost/cover/1',
    );

    rerender(
      <GameCover
        accent="var(--accent)"
        className="test-cover"
        coverRef={null}
        resetKey={resetKey}
        title="Kirby’s Adventure"
      />,
    );

    expect(
      screen.getByRole('img', { name: 'No cover available for Kirby’s Adventure' }),
    ).toHaveStyle({ '--system-accent': 'var(--accent)' });
  });

  it('falls back after an image error and retries after a new authoritative DTO', () => {
    const firstResetKey = {};
    const { rerender } = render(
      <GameCover
        accent="var(--accent)"
        className="test-cover"
        coverRef="rfmedia://localhost/cover/1"
        resetKey={firstResetKey}
        title="Kirby’s Adventure"
      />,
    );

    fireEvent.error(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' }));
    expect(
      screen.getByRole('img', { name: 'No cover available for Kirby’s Adventure' }),
    ).toBeInTheDocument();

    rerender(
      <GameCover
        accent="var(--accent)"
        className="test-cover"
        coverRef="rfmedia://localhost/cover/1"
        resetKey={{}}
        title="Kirby’s Adventure"
      />,
    );

    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveAttribute(
      'src',
      'rfmedia://localhost/cover/1',
    );
  });
});
