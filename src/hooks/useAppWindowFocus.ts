import { useEffect, useState } from 'react';

import {
  isAppWindowFocused,
  isDesktopRuntime,
  onAppWindowFocusChanged,
} from '../platform/appWindow';

interface NativeFocusState {
  /** `null` until the native state has actually been read. */
  focused: boolean | null;
  /** `null` until the subscription attempt has finished. */
  subscribed: boolean | null;
}

/**
 * Whether the RetroFrontier application window currently owns focus.
 *
 * Controller actions are only delivered while it does, so input meant for a foreground emulator or
 * another application is never consumed by RetroFrontier.
 *
 * The two runtimes are deliberately different, because "unknown" means different things in them:
 *
 * - **Desktop (Tauri).** This is the real ownership boundary. Unknown fails closed: until the native
 *   state has been read and says `true`, and until a focus subscription is really established, no
 *   controller input is dispatched. A window state that cannot be read, or a subscription that could
 *   not be created, must never become a permanent ownership grant — without the subscription the
 *   state could never be revoked again.
 * - **Plain browser (dev server).** There is no native window to own anything and no emulator to
 *   take input away, so the window is treated as focused and controller development stays usable.
 */
export function useAppWindowFocus(): boolean {
  const [desktop] = useState(isDesktopRuntime);
  const [state, setState] = useState<NativeFocusState>({ focused: null, subscribed: null });

  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void isAppWindowFocused()
      .then((focused) => {
        if (!disposed) setState((current) => ({ ...current, focused }));
      })
      .catch(() => {
        if (!disposed) setState((current) => ({ ...current, focused: null }));
      });

    void onAppWindowFocusChanged((focused) => {
      if (!disposed) setState((current) => ({ ...current, focused }));
    })
      .then((release) => {
        if (release === null) {
          if (!disposed) setState((current) => ({ ...current, subscribed: false }));
          return;
        }
        if (disposed) release();
        else {
          unlisten = release;
          setState((current) => ({ ...current, subscribed: true }));
        }
      })
      .catch(() => {
        if (!disposed) setState((current) => ({ ...current, subscribed: false }));
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [desktop]);

  if (!desktop) return true;
  return state.focused === true && state.subscribed === true;
}
