import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { GameCover } from './GameCover';
import gameCoverSource from './GameCover.tsx?raw';

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
  it('stays provider- and system-agnostic: it renders a coverRef or a fallback, nothing else', () => {
    // M8.6 gives Library cards system-aware geometry. `GameCover` is shared with Game Detail, so it
    // must keep knowing nothing about systems, shelves, or cover profiles. This is an architectural
    // guard rather than a style check: the moment the shared component learns one system name, both
    // consumers start inheriting the other's presentation policy.
    for (const forbidden of [
      'systemId',
      'systemCoverPresentation',
      'CoverPresentation',
      'Shelf',
      'snes',
      'nintendo_64',
      'nintendo_gamecube',
      'playstation',
    ]) {
      expect(gameCoverSource, `GameCover must not know about ${forbidden}`).not.toContain(
        forbidden,
      );
    }
  });

  it('lets each consumer own its own presentation class', () => {
    const { rerender } = render(
      <GameCover
        accent="var(--accent)"
        className="game-card-cover"
        coverRef="rfmedia://localhost/cover/1"
        resetKey={{}}
        title="Kirby’s Adventure"
      />,
    );
    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveClass(
      'game-card-cover',
    );

    rerender(
      <GameCover
        accent="var(--accent)"
        className="game-detail-cover"
        coverRef="rfmedia://localhost/cover/1"
        resetKey={{}}
        title="Kirby’s Adventure"
      />,
    );
    const detailCover = screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' });
    expect(detailCover).toHaveClass('game-detail-cover');
    expect(detailCover).not.toHaveClass('game-card-cover');
  });
});
