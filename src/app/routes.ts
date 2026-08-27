import { useCallback, useEffect, useState } from 'react';

export type AppRoute = 'library' | 'settings';
export type AppRoutePath = '/library' | '/settings';

export function routePath(route: AppRoute): AppRoutePath {
  return route === 'settings' ? '/settings' : '/library';
}

export function routeFromPath(pathname: string): AppRoute {
  return pathname === '/settings' ? 'settings' : 'library';
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
