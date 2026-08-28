import { fireEvent, render, screen } from '@testing-library/react';
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
  render(
    <GameCard
      item={{ ...item, ...overrides }}
      systemName="Nintendo Entertainment System"
      accent="var(--accent)"
      favoritePending={favoritePending}
      onOpenGame={onOpenGame}
      onToggleFavorite={onToggleFavorite}
    />,
  );
  return { onOpenGame, onToggleFavorite };
}

describe('GameCard', () => {
  it('renders list-level metadata with accessible favorite and local availability semantics', () => {
    const { onToggleFavorite } = renderCard();

    expect(screen.getByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveAttribute(
      'href',
      '/games/1',
    );
    expect(screen.getByText('Nintendo Entertainment System')).toBeInTheDocument();
    expect(screen.getByText('1993')).toBeInTheDocument();
    expect(screen.getByText('LOCAL')).toBeInTheDocument();
    const favorite = screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' });
    expect(favorite).toHaveAttribute('aria-pressed', 'false');
    fireEvent.click(favorite);
    expect(onToggleFavorite).toHaveBeenCalledWith(item);
    expect(screen.getAllByRole('button')).toHaveLength(1);
  });

  it('opens details from the title link without nesting Favorite inside an interactive element', () => {
    const { onOpenGame } = renderCard();
    const link = screen.getByRole('link', { name: 'Open Kirby’s Adventure details' });

    fireEvent.click(link);

    expect(onOpenGame).toHaveBeenCalledWith(1);
    expect(link.closest('button')).toBeNull();
    expect(
      screen.getByRole('button', { name: 'Add Kirby’s Adventure to favorites' }),
    ).not.toContainElement(link);
  });

  it('uses the local title fallback and keeps metadata failure separate from availability', () => {
    renderCard({
      metadataTitle: null,
      displayTitle: 'Local title',
      metadataMatchState: 'failed',
      coverRef: null,
    });

    expect(screen.getByRole('heading', { name: 'Local title' })).toBeInTheDocument();
    expect(screen.getByText('METADATA UNAVAILABLE')).toBeInTheDocument();
    expect(screen.getByText('LOCAL')).toBeInTheDocument();
    expect(screen.queryByText('LOCAL FILE MISSING')).not.toBeInTheDocument();
  });

  it('shows unavailable content independently and disables an in-flight favorite', () => {
    renderCard({ availability: 'unavailable', favorite: true }, true);

    expect(screen.getByText('LOCAL FILE MISSING')).toBeInTheDocument();
    const favorite = screen.getByRole('button', {
      name: 'Updating favorite for Kirby’s Adventure',
    });
    expect(favorite).toBeDisabled();
    expect(favorite).toHaveAttribute('aria-pressed', 'true');
  });

  it('retains stale last-known-good title and cover presentation', () => {
    renderCard({ metadataMatchState: 'stale' });

    expect(screen.getByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveAttribute(
      'src',
      item.coverRef,
    );
    expect(screen.getByText('METADATA STALE')).toBeInTheDocument();
    expect(screen.getByText('LOCAL')).toBeInTheDocument();
  });

  it('uses an opaque lazy cover reference and falls back normally after image failure', () => {
    renderCard();
    const cover = screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' });
    expect(cover).toHaveAttribute('loading', 'lazy');
    expect(cover).toHaveAttribute('src', item.coverRef);

    fireEvent.error(cover);
    expect(
      screen.getByRole('img', { name: 'No cover available for Kirby’s Adventure' }),
    ).toBeInTheDocument();
    expect(screen.queryByText('METADATA UNAVAILABLE')).not.toBeInTheDocument();
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
});
