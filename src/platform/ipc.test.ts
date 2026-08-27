import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoke, listen } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

import {
  getLibraryGameDetail,
  getScanIssuePage,
  onMetadataStateChanged,
  queryLibrary,
  setGameFavorite,
} from './ipc';

describe('M6.1 IPC contracts', () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
  });

  it('keeps bounded library command names and camel-case request shapes', async () => {
    invoke.mockResolvedValueOnce({ items: [], total: 0, offset: 0, limit: 60 });
    invoke.mockResolvedValueOnce({ issues: [], total: 0, offset: 0, limit: 50 });
    invoke.mockResolvedValueOnce(null);
    invoke.mockResolvedValueOnce({ gameId: 7, favorite: true });

    await queryLibrary({
      search: 'chrono',
      systemId: 'snes',
      favoritesOnly: true,
      availability: 'available',
      limit: 60,
      offset: 0,
    });
    await getScanIssuePage({ offset: 50, limit: 100 });
    await getLibraryGameDetail({ gameId: 7 });
    await setGameFavorite({ gameId: 7, favorite: true });

    expect(invoke).toHaveBeenNthCalledWith(1, 'query_library', {
      request: {
        search: 'chrono',
        systemId: 'snes',
        favoritesOnly: true,
        availability: 'available',
        limit: 60,
        offset: 0,
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'get_scan_issue_page', {
      request: { offset: 50, limit: 100 },
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'get_library_game_detail', {
      request: { gameId: 7 },
    });
    expect(invoke).toHaveBeenNthCalledWith(4, 'set_game_favorite', {
      request: { gameId: 7, favorite: true },
    });
  });

  it('delivers only the minimal metadata invalidation payload to subscribers', async () => {
    const unlisten = vi.fn();
    listen.mockResolvedValue(unlisten);
    const handler = vi.fn();

    const returnedUnlisten = await onMetadataStateChanged(handler);
    const callback = listen.mock.calls[0][1] as (event: {
      payload: { gameId: number; providerId: 'screenScraper' };
    }) => void;
    callback({ payload: { gameId: 42, providerId: 'screenScraper' } });

    expect(listen).toHaveBeenCalledWith('metadata-state-changed', expect.any(Function));
    expect(handler).toHaveBeenCalledWith({ gameId: 42, providerId: 'screenScraper' });
    expect(returnedUnlisten).toBe(unlisten);
  });
});
