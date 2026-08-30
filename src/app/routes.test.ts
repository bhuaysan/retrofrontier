import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { gameRoute, isGameRoute, routeFromPath, routePath, useRoute } from './routes';

describe('game routes', () => {
  beforeEach(() => {
    window.history.replaceState({}, '', '/library');
  });

  it('retains the existing library and settings routes', () => {
    expect(routeFromPath('/library')).toBe('library');
    expect(routeFromPath('/settings')).toBe('settings');
    expect(routePath('library')).toBe('/library');
    expect(routePath('settings')).toBe('/settings');
  });

  it('parses a valid positive safe game id', () => {
    const route = routeFromPath('/games/42');

    expect(route).toEqual({ kind: 'game', gameId: 42, rawId: '42' });
    expect(isGameRoute(route)).toBe(true);
    expect(gameRoute(42)).toEqual(route);
    expect(routePath(route)).toBe('/games/42');
  });

  it.each([
    '/games/',
    '/games/not-an-id',
    '/games/0',
    '/games/-1',
    '/games/1.5',
    '/games/9007199254740992',
  ])('keeps malformed game path %s in an invalid detail route', (pathname) => {
    expect(() => routeFromPath(pathname)).not.toThrow();
    const route = routeFromPath(pathname);

    expect(route).toMatchObject({ kind: 'game', gameId: null });
    expect(isGameRoute(route)).toBe(true);
  });

  it('normalizes unrelated unknown paths to the library', () => {
    expect(routeFromPath('/not-a-retrofrontier-route')).toBe('library');
  });

  it('pushes valid game routes and follows browser history', async () => {
    const { result } = renderHook(() => useRoute());

    act(() => result.current.navigate(gameRoute(7)));
    expect(window.location.pathname).toBe('/games/7');
    expect(result.current.route).toEqual({ kind: 'game', gameId: 7, rawId: '7' });

    act(() => window.history.back());
    await waitFor(() => {
      expect(result.current.route).toBe('library');
      expect(window.location.pathname).toBe('/library');
    });
  });
});
