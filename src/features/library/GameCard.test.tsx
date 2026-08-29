import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { LibraryListItem } from '../../platform/ipc';
import { GameCard } from './GameCard';

const item: LibraryListItem = {
  gameId: 1,
  systemId: 'nes',
  localTitle: 'Local title',
  metadataTitle: 'Kirby’s Adventure',
  displayTitle: 'Kirby’s Adventure',
  sortTitle: 'kirby’s adventure',
  availability: 'available',
  favorite: false,
  metadataMatchState: 'matched',
  releaseDate: '1993-03-23',
  genre: 'Platform',
  region: 'US',
  coverRef: 'rfmedia://localhost/cover/1',
};

function renderCard(overrides: Partial<LibraryListItem> = {}, favoritePending = false) {
  const onToggleFavorite = vi.fn();
  const onOpenGame = vi.fn();
  const { unmount } = render(
    <GameCard
      item={{ ...item, ...overrides }}
      systemName="Nintendo Entertainment System"
      accent="var(--accent)"
      favoritePending={favoritePending}
      onOpenGame={onOpenGame}
      onToggleFavorite={onToggleFavorite}
    />,
  );
  return { onOpenGame, onToggleFavorite, unmount };
}

describe('GameCard', () => {
  it('renders the compact B1 tile: title, compact system badge, and real release year', () => {
    const { onToggleFavorite } = renderCard();

    expect(screen.getByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveAttribute(
      'href',
      '/games/1',
    );
    expect(screen.getByText('NES')).toBeInTheDocument();
    expect(screen.getByText('1993')).toBeInTheDocument();
    const favorite = screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' });
    expect(favorite).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(favorite);
    expect(onToggleFavorite).toHaveBeenCalledWith(item);
    expect(screen.getAllByRole('button')).toHaveLength(1);
  });

  it('keeps the detail target a real anchor without browser-link chrome', () => {
    const { onOpenGame } = renderCard();
    const link = screen.getByRole('link', { name: 'Open Kirby’s Adventure details' });

    expect(link.tagName).toBe('A');
    expect(link).toHaveAttribute('href', '/games/1');
    expect(link).toHaveAttribute('data-game-detail-link', '1');
    // The card owns the title's appearance; the anchor must not fall back to the user agent's
    // blue/underlined link styling.
    expect(link).toHaveClass('game-card-title-link');

    fireEvent.click(link);

    expect(onOpenGame).toHaveBeenCalledWith(1);
    expect(onOpenGame).toHaveBeenCalledTimes(1);
    expect(link.closest('button')).toBeNull();
    expect(
      screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' }),
    ).not.toContainElement(link);
  });

  it('exposes exactly one stretched detail target and keeps Favorite outside the anchor', () => {
    renderCard();

    const card = screen.getByRole('article');
    const detailLinks = within(card).getAllByRole('link');
    const detailLink = screen.getByRole('link', { name: 'Open Kirby’s Adventure details' });
    const favorite = screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' });

    expect(detailLinks).toHaveLength(1);
    expect(detailLink).toHaveClass('game-card-detail-target');
    expect(detailLink).toHaveAttribute('href', '/games/1');
    expect(detailLink).toHaveAttribute('data-game-detail-link', '1');
    expect(favorite.closest('a')).toBeNull();
    expect(card).toContainElement(favorite);
  });

  it('keeps the full-card detail target on unavailable cards', () => {
    renderCard({ availability: 'unavailable', coverRef: null });

    const card = screen.getByRole('article');
    const detailLink = within(card).getByRole('link', {
      name: 'Open Kirby’s Adventure details',
    });

    expect(detailLink).toHaveClass('game-card-detail-target');
    expect(detailLink).toHaveAttribute('href', '/games/1');
    expect(detailLink).toHaveAttribute('data-game-detail-link', '1');
    expect(
      within(card).getByRole('img', { name: 'No cover available for Kirby’s Adventure' }),
    ).toBeVisible();
  });

  it('toggles Favorite exactly once without opening Game Detail', () => {
    const { onOpenGame, onToggleFavorite } = renderCard();
    const favorite = screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' });

    fireEvent.click(favorite);

    expect(onToggleFavorite).toHaveBeenCalledWith(item);
    expect(onToggleFavorite).toHaveBeenCalledTimes(1);
    expect(onOpenGame).not.toHaveBeenCalled();
  });

  it('leaves modified clicks to the native anchor', () => {
    const { onOpenGame } = renderCard();
    const link = screen.getByRole('link', { name: 'Open Kirby’s Adventure details' });

    fireEvent.click(link, { ctrlKey: true });
    fireEvent.click(link, { metaKey: true });
    fireEvent.click(link, { shiftKey: true });
    fireEvent.click(link, { button: 1 });

    expect(onOpenGame).not.toHaveBeenCalled();
  });

  it('does not spend a card row on normal local availability', () => {
    renderCard();

    expect(screen.queryByText('LOCAL')).not.toBeInTheDocument();
    expect(screen.queryByText('LOCAL FILE MISSING')).not.toBeInTheDocument();
    expect(screen.queryByText('MISSING')).not.toBeInTheDocument();
  });

  it('does not render metadata lifecycle prose as a visible card row', () => {
    for (const state of [
      'pending',
      'noMatch',
      'ambiguous',
      'deferred',
      'failed',
      'stale',
    ] as const) {
      const { unmount } = render(
        <GameCard
          accent="var(--accent)"
          favoritePending={false}
          item={{ ...item, metadataMatchState: state }}
          onOpenGame={vi.fn()}
          onToggleFavorite={vi.fn()}
          systemName="Nintendo Entertainment System"
        />,
      );

      expect(
        screen.queryByText(/METADATA|MATCH REVIEW/, { ignore: '.visually-hidden' }),
      ).toBeNull();
      unmount();
    }
  });

  it('keeps the coarse metadata state available to assistive technology', () => {
    renderCard({ metadataMatchState: 'stale' });

    const card = screen.getByRole('article');
    expect(within(card).getByText('METADATA STALE')).toBeInTheDocument();
    expect(within(card).getByText('METADATA STALE')).toHaveClass('visually-hidden');
  });

  it('does not add a genre or region row to the compact tile', () => {
    renderCard();

    expect(screen.queryByText(/Platform/)).not.toBeInTheDocument();
    expect(screen.queryByText(/US/)).not.toBeInTheDocument();
  });

  it('keeps a visible, accessible missing-content indication for unavailable local content', () => {
    renderCard({ availability: 'unavailable', favorite: true }, true);

    const flag = screen.getByText('MISSING');
    expect(flag).toBeVisible();
    expect(flag.closest('.game-card-flag')).toHaveTextContent(/MISSING local content/);
    // The card stays browsable: the detail route is never disabled for a missing local file.
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveAttribute(
      'href',
      '/games/1',
    );
    const favorite = screen.getByRole('button', {
      name: 'Remove Kirby’s Adventure from favorites',
    });
    expect(favorite).not.toBeDisabled();
    expect(favorite).toHaveAttribute('aria-busy', 'true');
    expect(favorite).toHaveAttribute('aria-pressed', 'true');
  });

  it('renders the compact badge but keeps the full system name accessible', () => {
    renderCard({ systemId: 'snes' }, false);

    const badge = screen.getByText('SNES');
    expect(badge).toBeInTheDocument();
    expect(badge.closest('.game-card-system')).toHaveAttribute(
      'title',
      'Nintendo Entertainment System',
    );
    expect(
      within(screen.getByRole('article')).getByText('Nintendo Entertainment System'),
    ).toHaveClass('visually-hidden');
  });

  it('falls back safely for an unknown future authoritative system ID', () => {
    render(
      <GameCard
        accent="var(--accent-3)"
        favoritePending={false}
        item={{ ...item, systemId: 'atari_2600' as LibraryListItem['systemId'] }}
        onOpenGame={vi.fn()}
        onToggleFavorite={vi.fn()}
        systemName="Atari 2600"
      />,
    );

    const badge = screen.getByText('ATARI 2600');
    expect(badge).toBeVisible();
    expect(badge.textContent).not.toBe('');
  });

  it('renders no year at all when the release date is missing or not a real year', () => {
    const { unmount } = renderCard({ releaseDate: null });
    expect(screen.queryByText('UNKNOWN')).not.toBeInTheDocument();
    expect(screen.queryByText('N/A')).not.toBeInTheDocument();
    expect(screen.queryByText('----')).not.toBeInTheDocument();
    expect(document.querySelector('time')).toBeNull();
    unmount();

    renderCard({ releaseDate: 'not-a-date' });
    expect(document.querySelector('time')).toBeNull();
  });

  it('uses the local title fallback and keeps metadata failure separate from availability', () => {
    renderCard({
      metadataTitle: null,
      displayTitle: 'Local title',
      metadataMatchState: 'failed',
      coverRef: null,
    });

    expect(screen.getByRole('heading', { name: 'Local title' })).toBeInTheDocument();
    expect(screen.queryByText('MISSING')).not.toBeInTheDocument();
  });

  it('uses the local title when the authoritative display title is only whitespace', () => {
    renderCard({ displayTitle: '   ', localTitle: 'Local fallback title' });

    expect(screen.getByRole('heading', { name: 'Local fallback title' })).toBeInTheDocument();
  });

  it('retains stale last-known-good title and cover presentation', () => {
    renderCard({ metadataMatchState: 'stale' });

    expect(screen.getByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveAttribute(
      'src',
      item.coverRef,
    );
  });

  it('uses an opaque lazy cover reference and falls back to the C4 placeholder after failure', () => {
    renderCard();
    const cover = screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' });
    expect(cover).toHaveAttribute('loading', 'lazy');
    expect(cover).toHaveAttribute('src', item.coverRef);

    fireEvent.error(cover);
    const placeholder = screen.getByRole('img', {
      name: 'No cover available for Kirby’s Adventure',
    });
    expect(placeholder).toHaveClass('game-card-placeholder');
    expect(placeholder).toHaveTextContent('Kirby’s Adventure');
    expect(screen.queryByRole('img', { name: /Cover art/ })).not.toBeInTheDocument();
  });

  it('tries a changed authoritative cover reference after an earlier cover failed', () => {
    const onToggleFavorite = vi.fn();
    const { rerender } = render(
      <GameCard
        accent="var(--accent)"
        favoritePending={false}
        item={item}
        onOpenGame={vi.fn()}
        onToggleFavorite={onToggleFavorite}
        systemName="Nintendo Entertainment System"
      />,
    );
    fireEvent.error(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' }));
    rerender(
      <GameCard
        accent="var(--accent)"
        favoritePending={false}
        item={{ ...item, coverRef: 'rfmedia://localhost/cover/1?revision=2' }}
        onOpenGame={vi.fn()}
        onToggleFavorite={onToggleFavorite}
        systemName="Nintendo Entertainment System"
      />,
    );

    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveAttribute(
      'src',
      'rfmedia://localhost/cover/1?revision=2',
    );
  });

  it('retries a stable cover reference when a new authoritative DTO arrives', () => {
    const onToggleFavorite = vi.fn();
    const { rerender } = render(
      <GameCard
        accent="var(--accent)"
        favoritePending={false}
        item={item}
        onOpenGame={vi.fn()}
        onToggleFavorite={onToggleFavorite}
        systemName="Nintendo Entertainment System"
      />,
    );
    fireEvent.error(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' }));
    expect(
      screen.getByRole('img', { name: 'No cover available for Kirby’s Adventure' }),
    ).toBeInTheDocument();

    rerender(
      <GameCard
        accent="var(--accent)"
        favoritePending={false}
        item={{ ...item }}
        onOpenGame={vi.fn()}
        onToggleFavorite={onToggleFavorite}
        systemName="Nintendo Entertainment System"
      />,
    );

    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveAttribute(
      'src',
      item.coverRef,
    );
  });

  it('shows the C4 title placeholder immediately for a missing cover', () => {
    const longTitle = 'A Very Long Local Game Title That Must Remain Readable';
    renderCard({ coverRef: null, displayTitle: longTitle, metadataTitle: null });

    const placeholder = screen.getByRole('img', { name: `No cover available for ${longTitle}` });
    expect(placeholder).toHaveStyle({ '--system-accent': 'var(--accent)' });
    expect(placeholder).toHaveTextContent(longTitle);
  });

  it('keeps a long title fully available even though the visible tile shows one line', () => {
    const longTitle = 'A Very Long Local Game Title That Must Remain Readable';
    renderCard({ coverRef: null, displayTitle: longTitle, metadataTitle: null });

    const heading = screen.getByRole('heading', { name: longTitle });
    expect(heading).toHaveAttribute('title', longTitle);
    expect(screen.getByRole('link', { name: `Open ${longTitle} details` })).toHaveTextContent(
      longTitle,
    );
  });

  it('exposes the accent token backing the tile so light-theme ink can stay accessible', () => {
    render(
      <GameCard
        accent="var(--accent-3)"
        favoritePending={false}
        item={{ ...item, systemId: 'mega_drive' }}
        onOpenGame={vi.fn()}
        onToggleFavorite={vi.fn()}
        systemName="Sega Mega Drive"
      />,
    );

    expect(screen.getByRole('article')).toHaveAttribute('data-system-accent', 'accent-3');
  });
});
