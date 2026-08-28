import { useCallback, useEffect, useState } from 'react';

export interface GameRoute {
  kind: 'game';
  gameId: number | null;
  rawId: string;
}

export type AppRoute = 'library' | 'settings' | GameRoute;
export type AppRoutePath = '/library' | '/settings' | `/games/${string}`;

const GAME_ROUTE_PREFIX = '/games/';

function parseGameId(rawId: string) {
  if (!/^\d+$/.test(rawId)) return null;
  const gameId = Number(rawId);
  return Number.isSafeInteger(gameId) && gameId > 0 ? gameId : null;
}

export function gameRoute(gameId: number): GameRoute {
  const valid = Number.isSafeInteger(gameId) && gameId > 0;
  return {
    kind: 'game',
    gameId: valid ? gameId : null,
    rawId: String(gameId),
  };
}

export function isGameRoute(route: AppRoute): route is GameRoute {
  return typeof route === 'object' && route.kind === 'game';
}

export function routePath(route: AppRoute): AppRoutePath {
  if (isGameRoute(route)) {
    return route.gameId === null
      ? `${GAME_ROUTE_PREFIX}${encodeURIComponent(route.rawId)}`
      : `${GAME_ROUTE_PREFIX}${route.gameId}`;
  }
  return route === 'settings' ? '/settings' : '/library';
}

export function routeFromPath(pathname: string): AppRoute {
  if (pathname === '/settings') return 'settings';
  if (pathname === '/library') return 'library';
  if (pathname === '/games' || pathname.startsWith(GAME_ROUTE_PREFIX)) {
    const rawId = pathname === '/games' ? '' : pathname.slice(GAME_ROUTE_PREFIX.length);
    return { kind: 'game', gameId: parseGameId(rawId), rawId };
  }
  return 'library';
}

export function useRoute() {
  const [route, setRoute] = useState<AppRoute>(() => routeFromPath(window.location.pathname));

  useEffect(() => {
    const path = routePath(route);
    if (window.location.pathname !== path) {
      window.history.replaceState({ route }, '', path);
    }
  }, [route]);

  useEffect(() => {
    const handlePopState = () => {
      setRoute(routeFromPath(window.location.pathname));
    };
    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, []);

  const navigate = useCallback((nextRoute: AppRoute) => {
    const path = routePath(nextRoute);
    if (window.location.pathname !== path) {
      window.history.pushState({ route: nextRoute }, '', path);
    }
    setRoute(nextRoute);
  }, []);

  return { route, navigate };
}
