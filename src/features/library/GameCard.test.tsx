import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
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

function renderCard(overrides: Partial<LibraryListItem> = {}, selected = false) {
  const onToggleSelected = vi.fn();
  const onOpenGame = vi.fn();
  const { rerender, unmount } = render(
    <GameCard
      accent="var(--accent)"
      item={{ ...item, ...overrides }}
      onOpenGame={onOpenGame}
      onToggleSelected={onToggleSelected}
      selected={selected}
      systemName="Nintendo Entertainment System"
    />,
  );
  return { onOpenGame, onToggleSelected, rerender, unmount };
}

describe('GameCard', () => {
  it('renders the compact B1 tile: title, compact system badge, and real release year', () => {
    renderCard();

    expect(screen.getByRole('heading', { name: 'Kirby’s Adventure' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveAttribute(
      'href',
      '/games/1',
    );
    expect(screen.getByText('NES')).toBeInTheDocument();
    expect(screen.getByText('1993')).toBeInTheDocument();
  });

  it('renders exactly one B1 selection control and no Library favorite action', () => {
    renderCard({ favorite: true });

    const buttons = screen.getAllByRole('button');
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveAccessibleName('Select Kirby’s Adventure');
    expect(buttons[0]).toHaveAttribute('aria-pressed', 'false');
    expect(buttons[0]).toHaveClass('game-card-select');

    // The Library card is no longer the Favorite mutation surface, even for a favorited game.
    expect(screen.queryByRole('button', { name: /favorites/i })).not.toBeInTheDocument();
    expect(document.querySelector('.game-card-favorite')).toBeNull();
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

    // The system is reported alongside the game so a Game Detail return can fall back to the shelf
    // the card was browsed on.
    expect(onOpenGame).toHaveBeenCalledWith(1, 'nes');
    expect(onOpenGame).toHaveBeenCalledTimes(1);
    expect(link.closest('button')).toBeNull();
  });

  it('exposes exactly one stretched detail target, topologically separate from selection', () => {
    renderCard();

    const card = screen.getByRole('article');
    const detailLinks = within(card).getAllByRole('link');
    const detailLink = screen.getByRole('link', { name: 'Open Kirby’s Adventure details' });
    const select = screen.getByRole('button', { name: 'Select Kirby’s Adventure' });

    expect(detailLinks).toHaveLength(1);
    expect(detailLink).toHaveClass('game-card-detail-target');
    expect(detailLink).toHaveAttribute('href', '/games/1');
    expect(detailLink).toHaveAttribute('data-game-detail-link', '1');
    // Neither control may nest inside the other: the layering, not event luck, keeps them apart.
    expect(select.closest('a')).toBeNull();
    expect(detailLink.closest('button')).toBeNull();
    expect(select).not.toContainElement(detailLink);
    expect(card).toContainElement(select);
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

  it('toggles selection exactly once without opening Game Detail', () => {
    const { onOpenGame, onToggleSelected } = renderCard();
    const select = screen.getByRole('button', { name: 'Select Kirby’s Adventure' });

    fireEvent.click(select);

    expect(onToggleSelected).toHaveBeenCalledWith(1);
    expect(onToggleSelected).toHaveBeenCalledTimes(1);
    expect(onOpenGame).not.toHaveBeenCalled();
  });

  it('toggles selection identically from the keyboard', () => {
    const { onOpenGame, onToggleSelected } = renderCard();
    const select = screen.getByRole('button', { name: 'Select Kirby’s Adventure' });

    select.focus();
    fireEvent.keyDown(select, { key: 'Enter' });
    fireEvent.keyUp(select, { key: 'Enter' });
    fireEvent.click(select);

    expect(onToggleSelected).toHaveBeenCalledTimes(1);
    expect(onOpenGame).not.toHaveBeenCalled();
  });

  it('renders the selected state with an accessible name, pressed state, and card marking', () => {
    renderCard({}, true);

    const select = screen.getByRole('button', { name: 'Deselect Kirby’s Adventure' });
    expect(select).toHaveAttribute('aria-pressed', 'true');
    // Selection is visible beyond the 22px control itself.
    expect(screen.getByRole('article')).toHaveClass('game-card--selected');
    expect(select.querySelector('svg')).not.toBeNull();
  });

  it('updates the selection control accessible name and state when selection changes', () => {
    const { rerender, onToggleSelected } = renderCard();
    expect(screen.getByRole('button', { name: 'Select Kirby’s Adventure' })).toHaveAttribute(
      'aria-pressed',
      'false',
    );

    rerender(
      <GameCard
        accent="var(--accent)"
        item={item}
        onOpenGame={vi.fn()}
        onToggleSelected={onToggleSelected}
        selected
        systemName="Nintendo Entertainment System"
      />,
    );

    const select = screen.getByRole('button', { name: 'Deselect Kirby’s Adventure' });
    expect(select).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(select);
    expect(onToggleSelected).toHaveBeenCalledWith(1);
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
          item={{ ...item, metadataMatchState: state }}
          onOpenGame={vi.fn()}
          onToggleSelected={vi.fn()}
          selected={false}
          systemName="Nintendo Entertainment System"
        />,
      );

      expect(
        screen.queryByText(/METADATA|MATCH REVIEW/, { ignore: '.visually-hidden' }),
      ).toBeNull();
      unmount();
    }
  });

  it('associates the coarse metadata state with the singular detail target', () => {
    renderCard({ metadataMatchState: 'stale' });

    const card = screen.getByRole('article');
    const state = within(card).getByText('METADATA STALE');
    const target = within(card).getByRole('link', { name: 'Open Kirby’s Adventure details' });
    expect(state).toHaveClass('visually-hidden');
    expect(state).toHaveAttribute('id', 'game-card-metadata-1');
    expect(target).toHaveAttribute('aria-describedby', 'game-card-metadata-1');
  });

  it('does not add a genre or region row to the compact tile', () => {
    renderCard();

    expect(screen.queryByText(/Platform/)).not.toBeInTheDocument();
    expect(screen.queryByText(/US/)).not.toBeInTheDocument();
  });

  it('keeps missing local content indicated, selectable, and browsable', () => {
    const { onToggleSelected } = renderCard({ availability: 'unavailable', favorite: true });

    const flag = screen.getByText('MISSING');
    expect(flag).toBeVisible();
    expect(flag.closest('.game-card-flag')).toHaveTextContent(/MISSING local content/);
    // The card stays browsable: the detail route is never disabled for a missing local file.
    expect(screen.getByRole('link', { name: 'Open Kirby’s Adventure details' })).toHaveAttribute(
      'href',
      '/games/1',
    );
    // Selection never implies launchability, so a missing file stays selectable.
    const select = screen.getByRole('button', { name: 'Select Kirby’s Adventure' });
    expect(select).not.toBeDisabled();
    fireEvent.click(select);
    expect(onToggleSelected).toHaveBeenCalledWith(1);
  });

  it('renders the compact badge but keeps the full system name accessible', () => {
    renderCard({ systemId: 'snes' });

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
        item={{ ...item, systemId: 'atari_2600' as LibraryListItem['systemId'] }}
        onOpenGame={vi.fn()}
        onToggleSelected={vi.fn()}
        selected={false}
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
    const { rerender } = render(
      <GameCard
        accent="var(--accent)"
        item={item}
        onOpenGame={vi.fn()}
        onToggleSelected={vi.fn()}
        selected={false}
        systemName="Nintendo Entertainment System"
      />,
    );
    fireEvent.error(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' }));
    rerender(
      <GameCard
        accent="var(--accent)"
        item={{ ...item, coverRef: 'rfmedia://localhost/cover/1?revision=2' }}
        onOpenGame={vi.fn()}
        onToggleSelected={vi.fn()}
        selected={false}
        systemName="Nintendo Entertainment System"
      />,
    );

    expect(screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' })).toHaveAttribute(
      'src',
      'rfmedia://localhost/cover/1?revision=2',
    );
  });

  it('retries a stable cover reference when a new authoritative DTO arrives', () => {
    const { rerender } = render(
      <GameCard
        accent="var(--accent)"
        item={item}
        onOpenGame={vi.fn()}
        onToggleSelected={vi.fn()}
        selected={false}
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
        item={{ ...item }}
        onOpenGame={vi.fn()}
        onToggleSelected={vi.fn()}
        selected={false}
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
        item={{ ...item, systemId: 'mega_drive' }}
        onOpenGame={vi.fn()}
        onToggleSelected={vi.fn()}
        selected={false}
        systemName="Sega Mega Drive"
      />,
    );

    expect(screen.getByRole('article')).toHaveAttribute('data-system-accent', 'accent-3');
  });
  it('frames the media with the system’s cover presentation profile, not one shared ratio', () => {
    renderCard({ systemId: 'snes' });
    expect(screen.getByRole('article')).toHaveAttribute('data-cover-presentation', 'landscapeBox');

    cleanup();
    renderCard({ systemId: 'nintendo_gamecube' });
    expect(screen.getByRole('article')).toHaveAttribute('data-cover-presentation', 'dvdBox');

    cleanup();
    renderCard({ systemId: 'nes' });
    expect(screen.getByRole('article')).toHaveAttribute('data-cover-presentation', 'portraitBox');
  });

  it('keeps an unknown authoritative system on the safe standard frame', () => {
    renderCard({ systemId: 'nintendo_switch_2' as LibraryListItem['systemId'] });

    expect(screen.getByRole('article')).toHaveAttribute('data-cover-presentation', 'standard');
  });

  it('carries the same profile whether the card is rendered in a shelf or in the full grid', () => {
    // The profile is a property of the system's card presentation, so it cannot depend on which
    // Library surface happens to be rendering. Nothing about the card is told where it lives.
    const { unmount } = renderCard({ systemId: 'nintendo_64' });
    const inGrid = screen.getByRole('article').getAttribute('data-cover-presentation');
    unmount();

    render(
      <div className="library-shelf-track">
        <GameCard
          accent="var(--accent)"
          item={{ ...item, systemId: 'nintendo_64' }}
          onOpenGame={vi.fn()}
          onToggleSelected={vi.fn()}
          selected={false}
          systemName="Nintendo 64"
        />
      </div>,
    );

    expect(screen.getByRole('article')).toHaveAttribute('data-cover-presentation', inGrid);
    expect(inGrid).toBe('landscapeBox');
  });

  it('shows the whole cover instead of cropping it to fill the frame', () => {
    renderCard();
    const cover = screen.getByRole('img', { name: 'Cover art for Kirby’s Adventure' });

    // The Library card's own cover class carries the containment rule. `GameCover` stays generic
    // and Game Detail keeps its separate `game-detail-cover` presentation.
    expect(cover).toHaveClass('game-card-cover');
    expect(cover).not.toHaveClass('game-detail-cover');
  });

  it('keeps the missing-cover placeholder and the selection control inside the profiled frame', () => {
    renderCard({ coverRef: null, systemId: 'snes' });

    const media = screen.getByRole('article').querySelector('.game-card-media');
    expect(media).not.toBeNull();
    expect(
      media?.querySelector('.game-card-placeholder'),
      'the C4 placeholder fills the system frame',
    ).not.toBeNull();
    expect(
      media?.querySelector('.game-card-select'),
      'selection stays a sibling of the detail target inside the frame',
    ).not.toBeNull();
  });

  it('keeps the unavailable indicator unchanged under a non-standard profile', () => {
    renderCard({ availability: 'unavailable', systemId: 'nintendo_gamecube' });

    const card = screen.getByRole('article');
    expect(card).toHaveClass('game-card--unavailable');
    expect(card).toHaveAttribute('data-cover-presentation', 'dvdBox');
    expect(screen.getByText('MISSING')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Select Kirby’s Adventure' })).toBeInTheDocument();
  });
});
