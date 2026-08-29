import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { LibraryListItem, LibraryPage } from '../platform/ipc';
import { useLibrarySelection } from './useLibrarySelection';

function listItem(gameId: number): LibraryListItem {
  return {
    gameId,
    systemId: 'nes',
    localTitle: `Local ${gameId}`,
    metadataTitle: `Game ${gameId}`,
    displayTitle: `Game ${gameId}`,
    sortTitle: `game ${gameId}`,
    availability: 'available',
    favorite: false,
    metadataMatchState: 'matched',
    releaseDate: null,
    genre: null,
    region: null,
    coverRef: null,
  };
}

function pageOf(gameIds: number[], offset = 0): LibraryPage {
  return { items: gameIds.map(listItem), total: gameIds.length, offset, limit: 60 };
}

describe('useLibrarySelection', () => {
  it('starts empty and toggles a game on and off', () => {
    const page = pageOf([1, 2, 3]);
    const { result } = renderHook(() => useLibrarySelection(page));

    expect(result.current.count).toBe(0);
    expect(result.current.isSelected(1)).toBe(false);

    act(() => result.current.toggle(1));
    expect(result.current.count).toBe(1);
    expect(result.current.isSelected(1)).toBe(true);

    act(() => result.current.toggle(1));
    expect(result.current.count).toBe(0);
    expect(result.current.isSelected(1)).toBe(false);
  });

  it('selects several visible games independently and clears all of them at once', () => {
    const page = pageOf([1, 2, 3]);
    const { result } = renderHook(() => useLibrarySelection(page));

    act(() => result.current.toggle(1));
    act(() => result.current.toggle(3));
    expect(result.current.count).toBe(2);
    expect(result.current.isSelected(2)).toBe(false);

    act(() => result.current.clear());
    expect(result.current.count).toBe(0);
    expect([...result.current.selectedIds]).toEqual([]);
  });

  it('keeps the selection when a refresh returns the same visible games', () => {
    const { result, rerender } = renderHook(({ page }) => useLibrarySelection(page), {
      initialProps: { page: pageOf([1, 2]) },
    });
    act(() => result.current.toggle(2));

    // A new page object with the same games (metadata invalidation, scan completion refresh).
    rerender({ page: pageOf([1, 2]) });

    expect(result.current.count).toBe(1);
    expect(result.current.isSelected(2)).toBe(true);
  });

  it('drops selected games that a newly committed result set no longer shows', () => {
    const { result, rerender } = renderHook(({ page }) => useLibrarySelection(page), {
      initialProps: { page: pageOf([1, 2, 3]) },
    });
    act(() => result.current.toggle(1));
    act(() => result.current.toggle(3));
    expect(result.current.count).toBe(2);

    // A committed search/filter change that only keeps game 3 visible.
    rerender({ page: pageOf([3]) });

    expect(result.current.count).toBe(1);
    expect(result.current.isSelected(1)).toBe(false);
    expect(result.current.isSelected(3)).toBe(true);
  });

  it('leaves no invisible selection behind after page navigation', () => {
    const { result, rerender } = renderHook(({ page }) => useLibrarySelection(page), {
      initialProps: { page: pageOf([1, 2], 0) },
    });
    act(() => result.current.toggle(1));
    act(() => result.current.toggle(2));
    expect(result.current.count).toBe(2);

    rerender({ page: pageOf([61, 62], 60) });

    expect(result.current.count).toBe(0);
  });

  it('does not resurrect an old selection when a game returns to the visible page', () => {
    const { result, rerender } = renderHook(({ page }) => useLibrarySelection(page), {
      initialProps: { page: pageOf([1, 2]) },
    });
    act(() => result.current.toggle(1));

    rerender({ page: pageOf([3, 4]) });
    expect(result.current.count).toBe(0);

    rerender({ page: pageOf([1, 2]) });
    expect(result.current.count).toBe(0);
    expect(result.current.isSelected(1)).toBe(false);
  });

  it('clears the selection while no result set is committed', () => {
    const { result, rerender } = renderHook(
      ({ page }: { page: LibraryPage | null }) => useLibrarySelection(page),
      { initialProps: { page: pageOf([1, 2]) as LibraryPage | null } },
    );
    act(() => result.current.toggle(1));

    rerender({ page: null });

    expect(result.current.count).toBe(0);
  });
});
