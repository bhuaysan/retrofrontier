import { useEffect, useState } from 'react';

import {
  isAppWindowFocused,
  isDesktopRuntime,
  onAppWindowFocusChanged,
} from '../platform/appWindow';

interface NativeFocusState {
  /** `null` until the native state has actually been observed. */
  focused: boolean | null;
  /** `null` until the subscription attempt has finished. */
  subscribed: boolean | null;
}

const UNKNOWN: NativeFocusState = { focused: null, subscribed: null };

/**
 * Whether the RetroFrontier application window currently owns focus.
 *
 * Controller actions are only delivered while it does, so input meant for a foreground emulator or
 * another application is never consumed by RetroFrontier.
 *
 * The two runtimes are deliberately different, because "unknown" means different things in them:
 *
 * - **Desktop (Tauri).** This is the real ownership boundary. Unknown fails closed: until the native
 *   state has been observed and says `true`, and until a focus subscription is really established, no
 *   controller input is dispatched. A window state that cannot be read, or a subscription that could
 *   not be created, must never become a permanent ownership grant — without the subscription the
 *   state could never be revoked again.
 * - **Plain browser (dev server).** There is no native window to own anything and no emulator to
 *   take input away, so the window is treated as focused and controller development stays usable.
 *
 * ## Bootstrap ordering
 *
 * The two native observations are **sequenced, not raced**, because racing them can grant ownership
 * from an observation that was already wrong:
 *
 * 1. Start failed closed. Nothing is owned until something authoritative has been observed.
 * 2. Establish the subscription **first**. Reading first leaves a gap in which a focus change can
 *    happen with no listener attached; that change is then lost forever and the read is the only —
 *    stale — observation.
 * 3. Only once the subscription is established, read the current native state. Because the listener
 *    is already attached, nothing that happens from here on can go unobserved.
 * 4. Once subscribed, **focus events are authoritative**. An in-flight read that resolves after an
 *    event is an older observation of a state the event has already superseded, so it is discarded.
 *    `observedEvents` is the ordering evidence for that: the read remembers the event count it
 *    started at and stands down if the count moved.
 *
 * There is no fail-open mode on the desktop side, and no polling: exactly one read, one subscription,
 * and then events.
 */
export function useAppWindowFocus(): boolean {
  const [desktop] = useState(isDesktopRuntime);
  const [state, setState] = useState<NativeFocusState>(UNKNOWN);

  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    // How many authoritative focus events have been observed. It only ever grows, so comparing it
    // across an await proves whether newer information arrived while a read was in flight.
    let observedEvents = 0;

    const readCurrentState = () => {
      const eventsAtStart = observedEvents;
      void isAppWindowFocused()
        .then((focused) => {
          if (disposed) return;
          // An event that arrived while this read was in flight is newer and already applied.
          if (observedEvents !== eventsAtStart) return;
          setState({ focused, subscribed: true });
        })
        .catch(() => {
          if (disposed) return;
          if (observedEvents !== eventsAtStart) return;
          // The state could not be read and no event has spoken: fail closed, but stay subscribed
          // so a later event can still establish ownership honestly.
          setState({ focused: null, subscribed: true });
        });
    };

    void onAppWindowFocusChanged((focused) => {
      if (disposed) return;
      observedEvents += 1;
      setState((current) => ({ ...current, focused }));
    })
      .then((release) => {
        if (release === null) {
          // No subscription exists. A focus state read now could never be revoked, so it is not
          // read at all and ownership stays refused.
          if (!disposed) setState((current) => ({ ...current, subscribed: false }));
          return;
        }
        if (disposed) {
          release();
          return;
        }
        unlisten = release;
        setState((current) => ({ ...current, subscribed: true }));
        readCurrentState();
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
