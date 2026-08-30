import { useEffect, useState } from 'react';

import { isAppWindowFocused, onAppWindowFocusChanged } from '../platform/appWindow';

/**
 * Whether the RetroFrontier application window currently owns focus.
 *
 * Controller actions are only delivered while it does, so input meant for a foreground emulator or
 * another application is never consumed by RetroFrontier. Where no native window state exists — a
 * plain browser page — the window is treated as focused so ordinary development stays usable.
 */
export function useAppWindowFocus(): boolean {
  const [focused, setFocused] = useState(true);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void isAppWindowFocused()
      .then((state) => {
        if (!disposed && state !== null) setFocused(state);
      })
      .catch(() => undefined);

    void onAppWindowFocusChanged((next) => {
      if (!disposed) setFocused(next);
    })
      .then((release) => {
        if (disposed) release();
        else unlisten = release;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return focused;
}
